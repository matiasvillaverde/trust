use crate::config::ConnectionConfig;
use crate::orders::find_live_order_by_ref;
use crate::parsing::string_field_optional;
use crate::support::broker_account_id;
use crate::{LIVE_ORDER_LOOKUP_DELAY_MS, LIVE_ORDER_LOOKUP_RETRIES};
use model::Account;
use reqwest::blocking::{Client, Response};
use serde_json::{json, Value};
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;

#[derive(Debug)]
pub(crate) struct IbkrClient {
    http: Client,
    config: ConnectionConfig,
}

impl IbkrClient {
    pub(crate) fn for_account(account: &Account) -> Result<Self, Box<dyn Error>> {
        let config = ConnectionConfig::read(&account.environment, account)?;
        let http = Client::builder()
            .cookie_store(true)
            .danger_accept_invalid_certs(config.allow_insecure_tls)
            .timeout(Duration::from_secs(20))
            .build()?;
        Ok(Self { http, config })
    }

    pub(crate) fn prepare_trading_session(
        &self,
        account: Option<&Account>,
    ) -> Result<(), Box<dyn Error>> {
        self.ensure_authenticated()?;
        let _ = self.get_json_value("/iserver/accounts", &[])?;
        if let Some(account) = account {
            if let Some(account_id) = account.broker_account_id.as_deref() {
                let _ =
                    self.post_json_value("/iserver/account", &json!({ "acctId": account_id }))?;
            }
        }
        Ok(())
    }

    pub(crate) fn live_orders(&self, account: &Account) -> Result<Vec<Value>, Box<dyn Error>> {
        let account_id = broker_account_id(account)?;
        let response = self.get_json_value(
            "/iserver/account/orders",
            &[
                ("accountId", account_id.to_string()),
                ("force", "true".to_string()),
            ],
        )?;
        if let Some(array) = response.as_array() {
            return Ok(array.clone());
        }
        response
            .get("orders")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| "IBKR live orders response did not include orders".into())
    }

    pub(crate) fn resolve_live_order_id(
        &self,
        account: &Account,
        order_ref: &str,
    ) -> Result<String, Box<dyn Error>> {
        for _ in 0..LIVE_ORDER_LOOKUP_RETRIES {
            let live_orders = self.live_orders(account)?;
            if let Some(live_order) = find_live_order_by_ref(&live_orders, order_ref) {
                if let Some(order_id) = string_field_optional(live_order, "orderId")
                    .or_else(|| string_field_optional(live_order, "order_id"))
                {
                    return Ok(order_id);
                }
            }
            sleep(Duration::from_millis(LIVE_ORDER_LOOKUP_DELAY_MS));
        }

        Err(format!("IBKR order '{order_ref}' was not found in live orders").into())
    }

    pub(crate) fn account_trades(&self) -> Result<Vec<Value>, Box<dyn Error>> {
        let response = self.get_json_value("/iserver/account/trades", &[])?;
        response
            .as_array()
            .cloned()
            .ok_or_else(|| "IBKR account trades response was not an array".into())
    }

    pub(crate) fn snapshot(&self, conid: &str, fields: &[&str]) -> Result<Value, Box<dyn Error>> {
        let field_csv = fields.join(",");
        for _ in 0..LIVE_ORDER_LOOKUP_RETRIES {
            let response = self.get_json_value(
                "/iserver/marketdata/snapshot",
                &[("conids", conid.to_string()), ("fields", field_csv.clone())],
            )?;
            let snapshot = response
                .as_array()
                .and_then(|items| items.first())
                .cloned()
                .ok_or("IBKR snapshot response was empty")?;
            if fields.iter().all(|field| snapshot.get(*field).is_some()) {
                return Ok(snapshot);
            }
            sleep(Duration::from_millis(LIVE_ORDER_LOOKUP_DELAY_MS));
        }

        Err("IBKR snapshot response did not include all requested fields".into())
    }

    pub(crate) fn get_json_value(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Value, Box<dyn Error>> {
        let response = self
            .http
            .get(self.url(path))
            .query(query)
            .send()
            .map_err(|error| format!("IBKR GET {path} failed: {error}"))?;
        parse_json_response("GET", path, response)
    }

    pub(crate) fn post_json_value(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<Value, Box<dyn Error>> {
        let response = self
            .http
            .post(self.url(path))
            .json(body)
            .send()
            .map_err(|error| format!("IBKR POST {path} failed: {error}"))?;
        parse_json_response("POST", path, response)
    }

    pub(crate) fn post_json_with_replies(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<Value, Box<dyn Error>> {
        let mut response = self.post_json_value(path, body)?;

        for _ in 0..4usize {
            if response.is_array() {
                return Ok(response);
            }

            reject_dangerous_order_reply(&response)?;

            let Some(reply_id) = string_field_optional(&response, "id") else {
                return Ok(response);
            };

            response = self.post_json_value(
                &format!("/iserver/reply/{reply_id}"),
                &json!({ "confirmed": true }),
            )?;
        }

        Err("IBKR order confirmation loop exceeded the maximum reply depth".into())
    }

    pub(crate) fn delete_no_content(&self, path: &str) -> Result<(), Box<dyn Error>> {
        let response = self
            .http
            .delete(self.url(path))
            .send()
            .map_err(|error| format!("IBKR DELETE {path} failed: {error}"))?;

        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();
        let body = response.text().unwrap_or_default();
        Err(format!("IBKR DELETE {path} returned {status}: {body}").into())
    }

    fn ensure_authenticated(&self) -> Result<(), Box<dyn Error>> {
        let status = self.get_json_value("/iserver/auth/status", &[])?;
        let authenticated = status
            .get("authenticated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let connected = status
            .get("connected")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        if authenticated && connected {
            return Ok(());
        }

        Err(format!(
            "IBKR Client Portal Gateway is not ready. Authenticate in the local gateway browser session first (base URL: {}).",
            self.config.base_url
        )
        .into())
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.config.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

fn reject_dangerous_order_reply(response: &Value) -> Result<(), Box<dyn Error>> {
    let messages = reply_messages(response);
    if messages.iter().any(|message| {
        let normalized = message.to_ascii_lowercase();
        normalized.contains("buying power")
            || normalized.contains("insufficient")
            || normalized.contains("margin")
            || normalized.contains("exceeds")
    }) {
        return Err(format!("IBKR order confirmation requires manual review: {messages:?}").into());
    }
    Ok(())
}

fn reply_messages(response: &Value) -> Vec<String> {
    match response.get("message") {
        Some(Value::String(message)) => vec![message.to_string()],
        Some(Value::Array(messages)) => messages
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn parse_json_response(
    method: &str,
    path: &str,
    response: Response,
) -> Result<Value, Box<dyn Error>> {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if !status.is_success() {
        return Err(format!("IBKR {method} {path} returned {status}: {body}").into());
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).map_err(|error| {
        format!("IBKR {method} {path} returned invalid JSON: {error}: {body}").into()
    })
}

#[cfg(test)]
mod tests {
    use super::{reject_dangerous_order_reply, reply_messages, IbkrClient};
    use crate::config::ConnectionConfig;
    use model::Account;
    use reqwest::blocking::Client;
    use serde_json::json;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{Ipv4Addr, TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn reply_messages_extracts_string_array_and_ignores_missing_values() {
        assert_eq!(
            reply_messages(&json!({ "message": "Confirm outside regular trading hours" })),
            vec!["Confirm outside regular trading hours".to_string()]
        );
        assert_eq!(
            reply_messages(&json!({ "message": ["First", 42, "Second"] })),
            vec!["First".to_string(), "Second".to_string()]
        );
        assert!(reply_messages(&json!({ "warning": "none" })).is_empty());
    }

    #[test]
    fn dangerous_order_replies_require_manual_review() {
        for message in [
            "WARNING: This order exceeds your account buying power. Proceed?",
            "Insufficient settled cash for this order",
            "Margin impact is above the account limit",
        ] {
            let error = reject_dangerous_order_reply(&json!({ "message": [message] }))
                .expect_err("dangerous reply should be rejected");
            assert!(error.to_string().contains("manual review"));
        }

        reject_dangerous_order_reply(&json!({
            "message": ["Confirm order submission"],
            "id": "safe-reply"
        }))
        .expect("benign confirmation can be auto-confirmed");
    }

    #[test]
    fn client_url_joins_base_and_path_without_double_slashes() {
        let client = IbkrClient {
            http: Client::new(),
            config: ConnectionConfig::new("https://ibkr.local/v1/api/", false),
        };

        assert_eq!(
            client.url("/iserver/accounts"),
            "https://ibkr.local/v1/api/iserver/accounts"
        );
    }

    #[test]
    fn client_http_helpers_parse_successful_gateway_responses() {
        with_test_server(
            vec![
                (
                    "GET /iserver/auth/status",
                    200,
                    r#"{"authenticated":true,"connected":true}"#,
                ),
                ("GET /iserver/accounts", 200, r#"[]"#),
                (
                    "GET /iserver/account/orders",
                    200,
                    r#"[{"order_ref":"entry-ref","orderId":"9001"}]"#,
                ),
                ("GET /iserver/account/trades", 200, r#"[]"#),
                (
                    "GET /iserver/marketdata/snapshot",
                    200,
                    r#"[{"31":"101.25","_updated":1773848700000}]"#,
                ),
                ("POST /iserver/test", 200, r#"{"ok":true}"#),
                ("DELETE /iserver/delete", 200, ""),
            ],
            |base_url| {
                let client = test_client(&base_url);
                let account = Account {
                    broker_account_id: Some("U1234567".to_string()),
                    ..Account::default()
                };

                client
                    .prepare_trading_session(None)
                    .expect("authenticated session");
                assert_eq!(
                    client
                        .resolve_live_order_id(&account, "entry-ref")
                        .expect("order id"),
                    "9001"
                );
                assert!(client.account_trades().expect("account trades").is_empty());
                assert_eq!(
                    client
                        .snapshot("265598", &["31"])
                        .expect("snapshot")
                        .get("31"),
                    Some(&json!("101.25"))
                );
                assert_eq!(
                    client
                        .post_json_value("/iserver/test", &json!({"hello":"world"}))
                        .expect("post response"),
                    json!({"ok": true})
                );
                client
                    .delete_no_content("/iserver/delete")
                    .expect("delete response");
            },
        );
    }

    #[test]
    fn client_json_helpers_surface_status_and_parse_errors() {
        with_test_server(
            vec![
                ("GET /bad-status", 500, "broker down"),
                ("GET /bad-json", 200, "not-json"),
            ],
            |base_url| {
                let client = test_client(&base_url);

                let status_error = client
                    .get_json_value("/bad-status", &[])
                    .expect_err("bad status should fail");
                assert!(status_error.to_string().contains("500"));

                let json_error = client
                    .get_json_value("/bad-json", &[])
                    .expect_err("invalid json should fail");
                assert!(json_error.to_string().contains("invalid JSON"));
            },
        );
    }

    #[test]
    fn post_json_with_replies_confirms_benign_replies_until_array_response() {
        with_test_server(
            vec![
                (
                    "POST /iserver/account/U1234567/orders",
                    200,
                    r#"{"id":"reply-safe-1","message":["Confirm order submission"]}"#,
                ),
                (
                    "POST /iserver/reply/reply-safe-1",
                    200,
                    r#"[{"order_id":"9001","order_status":"Submitted"}]"#,
                ),
            ],
            |base_url| {
                let client = test_client(&base_url);

                let response = client
                    .post_json_with_replies(
                        "/iserver/account/U1234567/orders",
                        &json!({"orders":[]}),
                    )
                    .expect("benign reply confirmed");

                assert!(response.is_array());
            },
        );
    }

    #[test]
    fn session_preparation_selects_configured_broker_account() {
        with_test_server(
            vec![
                (
                    "GET /iserver/auth/status",
                    200,
                    r#"{"authenticated":true,"connected":true}"#,
                ),
                ("GET /iserver/accounts", 200, r#"[]"#),
                ("POST /iserver/account", 200, r#"{"selected":true}"#),
            ],
            |base_url| {
                let client = test_client(&base_url);
                let account = Account {
                    broker_account_id: Some("U1234567".to_string()),
                    ..Account::default()
                };

                client
                    .prepare_trading_session(Some(&account))
                    .expect("account selection should be posted");
            },
        );
    }

    #[test]
    fn session_preparation_rejects_unauthenticated_gateway() {
        with_test_server(
            vec![(
                "GET /iserver/auth/status",
                200,
                r#"{"authenticated":false,"connected":true}"#,
            )],
            |base_url| {
                let client = test_client(&base_url);

                let error = client
                    .prepare_trading_session(None)
                    .expect_err("unauthenticated gateway should fail");

                assert!(error.to_string().contains("Gateway is not ready"));
                assert!(error.to_string().contains(&base_url));
            },
        );
    }

    #[test]
    fn live_order_and_account_trade_helpers_reject_malformed_gateway_shapes() {
        with_test_server(
            vec![
                ("GET /iserver/account/orders", 200, r#"{"unexpected":[]}"#),
                ("GET /iserver/account/trades", 200, r#"{"trades":[]}"#),
            ],
            |base_url| {
                let client = test_client(&base_url);
                let account = Account {
                    broker_account_id: Some("U1234567".to_string()),
                    ..Account::default()
                };

                let orders_error = client
                    .live_orders(&account)
                    .expect_err("orders must be an array or orders object");
                assert!(orders_error.to_string().contains("did not include orders"));

                let trades_error = client
                    .account_trades()
                    .expect_err("account trades must be an array");
                assert!(trades_error.to_string().contains("was not an array"));
            },
        );
    }

    #[test]
    fn resolve_live_order_id_accepts_snake_case_order_id() {
        with_test_server(
            vec![(
                "GET /iserver/account/orders",
                200,
                r#"[{"order_ref":"entry-ref","order_id":"fallback-9001"}]"#,
            )],
            |base_url| {
                let client = test_client(&base_url);
                let account = Account {
                    broker_account_id: Some("U1234567".to_string()),
                    ..Account::default()
                };

                assert_eq!(
                    client
                        .resolve_live_order_id(&account, "entry-ref")
                        .expect("snake-case order id should be accepted"),
                    "fallback-9001"
                );
            },
        );
    }

    #[test]
    fn snapshot_retries_until_requested_fields_are_available() {
        with_test_server(
            vec![
                (
                    "GET /iserver/marketdata/snapshot",
                    200,
                    r#"[{"_updated":1773848700000}]"#,
                ),
                (
                    "GET /iserver/marketdata/snapshot",
                    200,
                    r#"[{"31":"101.25","84":"101.20","_updated":1773848701000}]"#,
                ),
            ],
            |base_url| {
                let client = test_client(&base_url);

                let snapshot = client
                    .snapshot("265598", &["31", "84"])
                    .expect("snapshot should retry until all fields arrive");

                assert_eq!(snapshot.get("31"), Some(&json!("101.25")));
                assert_eq!(snapshot.get("84"), Some(&json!("101.20")));
            },
        );
    }

    #[test]
    fn snapshot_reports_empty_gateway_response() {
        with_test_server(
            vec![("GET /iserver/marketdata/snapshot", 200, r#"[]"#)],
            |base_url| {
                let client = test_client(&base_url);

                let error = client
                    .snapshot("265598", &["31"])
                    .expect_err("empty snapshot should fail");

                assert!(error.to_string().contains("snapshot response was empty"));
            },
        );
    }

    #[test]
    fn post_json_with_replies_returns_safe_reply_without_reply_id() {
        with_test_server(
            vec![(
                "POST /iserver/account/U1234567/orders",
                200,
                r#"{"message":["Order accepted without confirmation id"]}"#,
            )],
            |base_url| {
                let client = test_client(&base_url);

                let response = client
                    .post_json_with_replies(
                        "/iserver/account/U1234567/orders",
                        &json!({"orders":[]}),
                    )
                    .expect("safe reply without id should be returned");

                assert_eq!(
                    response,
                    json!({"message":["Order accepted without confirmation id"]})
                );
            },
        );
    }

    #[test]
    fn post_json_with_replies_caps_confirmation_depth() {
        with_test_server(
            vec![
                (
                    "POST /iserver/account/U1234567/orders",
                    200,
                    r#"{"id":"reply-1","message":["Confirm order submission"]}"#,
                ),
                (
                    "POST /iserver/reply/reply-1",
                    200,
                    r#"{"id":"reply-2","message":["Confirm order submission"]}"#,
                ),
                (
                    "POST /iserver/reply/reply-2",
                    200,
                    r#"{"id":"reply-3","message":["Confirm order submission"]}"#,
                ),
                (
                    "POST /iserver/reply/reply-3",
                    200,
                    r#"{"id":"reply-4","message":["Confirm order submission"]}"#,
                ),
                (
                    "POST /iserver/reply/reply-4",
                    200,
                    r#"{"id":"reply-5","message":["Confirm order submission"]}"#,
                ),
            ],
            |base_url| {
                let client = test_client(&base_url);

                let error = client
                    .post_json_with_replies(
                        "/iserver/account/U1234567/orders",
                        &json!({"orders":[]}),
                    )
                    .expect_err("reply loop should be capped");

                assert!(error.to_string().contains("maximum reply depth"));
            },
        );
    }

    #[test]
    fn delete_no_content_surfaces_gateway_status_and_body() {
        with_test_server(
            vec![("DELETE /iserver/delete", 409, "cannot delete")],
            |base_url| {
                let client = test_client(&base_url);

                let error = client
                    .delete_no_content("/iserver/delete")
                    .expect_err("non-success delete should fail");

                assert!(error.to_string().contains("409"));
                assert!(error.to_string().contains("cannot delete"));
            },
        );
    }

    #[test]
    fn empty_success_body_parses_as_null_json_value() {
        with_test_server(vec![("GET /empty", 200, "")], |base_url| {
            let client = test_client(&base_url);

            let response = client
                .get_json_value("/empty", &[])
                .expect("empty body should map to null");

            assert!(response.is_null());
        });
    }

    fn test_client(base_url: &str) -> IbkrClient {
        IbkrClient {
            http: Client::new(),
            config: ConnectionConfig::new(base_url, false),
        }
    }

    fn with_test_server(
        responses: Vec<(&'static str, u16, &'static str)>,
        run: impl FnOnce(String),
    ) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind test server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
        let server = thread::spawn(move || {
            for (expected_request, status, body) in responses {
                let (mut stream, _) = listener.accept().expect("accept request");
                let request_line = read_request_line(&mut stream);
                assert!(
                    request_line.starts_with(expected_request),
                    "expected request prefix {expected_request:?}, got {request_line:?}"
                );
                write_response(&mut stream, status, body);
            }
        });

        run(base_url);
        server.join().expect("test server finished");
    }

    fn read_request_line(stream: &mut TcpStream) -> String {
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .expect("read request line");
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).expect("read header");
            if header == "\r\n" || header.is_empty() {
                break;
            }
        }
        request_line
    }

    fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
        let reason = if status < 400 { "OK" } else { "ERROR" };
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write response");
        stream.flush().expect("flush response");
    }
}
