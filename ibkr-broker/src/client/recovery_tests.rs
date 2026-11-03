use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
};

fn gateway(
    responses: Vec<(u16, &'static str)>,
) -> (
    String,
    mpsc::Receiver<(String, Instant)>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            tx.send((line, Instant::now())).unwrap();
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(length) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = length.trim().parse::<usize>().unwrap();
                }
            }
            let mut body_bytes = vec![0; content_length];
            std::io::Read::read_exact(&mut reader, &mut body_bytes).unwrap();
            write!(stream, "HTTP/1.1 {status} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (url, rx, server)
}

fn client(url: &str) -> IbkrClient {
    IbkrClient::from_config(ConnectionConfig::new(url, false).unwrap()).unwrap()
}

#[test]
fn recreated_concurrent_clients_share_order_pacing() {
    let (url, rx, server) = gateway(vec![(200, "{}"), (200, "{}"), (200, "{}")]);
    client(&url)
        .get_json_value("/iserver/account/orders", &[])
        .unwrap();
    let first = rx.recv().unwrap();
    let next_url = url.clone();
    let other = std::thread::spawn(move || {
        client(&next_url)
            .get_json_value("/iserver/account/orders", &[])
            .unwrap()
    });
    client(&url)
        .get_json_value("/iserver/account/orders", &[])
        .unwrap();
    other.join().unwrap();
    let second = rx.recv().unwrap();
    let third = rx.recv().unwrap();
    assert!(second.1.duration_since(first.1) >= Duration::from_secs(5));
    assert!(third.1.duration_since(second.1) >= Duration::from_secs(5));
    server.join().unwrap();
}

#[test]
fn all_endpoint_classes_share_global_request_pacing() {
    let (url, rx, server) = gateway(vec![(200, "{}"), (200, "{}"), (200, "{}")]);
    for path in [
        "/iserver/auth/status",
        "/iserver/accounts",
        "/iserver/account/order/status/1",
    ] {
        client(&url).get_json_value(path, &[]).unwrap();
    }
    let first = rx.recv().unwrap();
    let second = rx.recv().unwrap();
    let third = rx.recv().unwrap();
    assert!(second.1.duration_since(first.1) >= Duration::from_millis(100));
    assert!(third.1.duration_since(second.1) >= Duration::from_millis(100));
    server.join().unwrap();
}

#[test]
fn rate_limit_is_not_retried_and_cooldown_survives_client_recreation() {
    let (url, rx, server) = gateway(vec![(429, "{}")]);
    let first = client(&url)
        .post_json_value("/iserver/account/DU_TEST/orders", &json!({}))
        .unwrap_err();
    assert!(first.to_string().contains("429"));
    let second = client(&url)
        .post_json_value("/iserver/account/DU_TEST/orders", &json!({}))
        .unwrap_err();
    assert!(second.to_string().contains("cooldown"));
    assert!(rx.recv().unwrap().0.starts_with("POST "));
    server.join().unwrap();
    assert!(rx.try_recv().is_err());
}

#[test]
fn cancellation_checks_documented_status_identity_and_preserves_uncertainty() {
    for (status, body, success) in [
        (
            200,
            r#"{"account":"DU_TEST","order_id":"42","order_status":"Cancelled","cum_fill":"0"}"#,
            true,
        ),
        (
            200,
            r#"{"account":"DU_OTHER","order_id":"42","order_status":"Cancelled","cum_fill":"0"}"#,
            false,
        ),
        (
            200,
            r#"{"account":"DU_TEST","order_id":"99","order_status":"Cancelled","cum_fill":"0"}"#,
            false,
        ),
        (
            200,
            r#"{"account":"DU_TEST","order_id":"42","order_status":"Cancelled","cum_fill":"1"}"#,
            false,
        ),
        (404, r#"{"error":"not found"}"#, false),
    ] {
        let (url, rx, server) = gateway(vec![(200, "{}"), (status, body)]);
        let client = client(&url);
        client
            .delete_no_content("/iserver/account/DU_TEST/order/42")
            .unwrap();
        let account = Account {
            broker_account_id: Some("DU_TEST".into()),
            ..Account::default()
        };
        let result = client.confirm_order_canceled(&account, "42", "entry-ref");
        assert_eq!(result.is_ok(), success);
        if let Err(error) = result {
            assert!(!error
                .downcast_ref::<BrokerError>()
                .is_some_and(BrokerError::is_retry_safe_mutation));
        }
        assert!(rx
            .recv()
            .unwrap()
            .0
            .starts_with("DELETE /iserver/account/DU_TEST/order/42 "));
        assert!(rx
            .recv()
            .unwrap()
            .0
            .starts_with("GET /iserver/account/order/status/42 "));
        server.join().unwrap();
    }
}
