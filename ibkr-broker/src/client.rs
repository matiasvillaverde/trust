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
