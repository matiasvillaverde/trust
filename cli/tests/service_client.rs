//! Exercise the shipped HTTP client, including ambiguous mutation outcomes.
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::{Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
const TOKEN: &str = "synthetic-client-contract-token";
#[derive(Debug)]
struct Request {
    line: String,
    headers: String,
    body: Vec<u8>,
}
struct Service {
    origin: String,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Vec<Request>>>,
}
impl Service {
    fn new(response: Option<Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(15);
            while !stopped.load(Ordering::SeqCst) {
                assert!(Instant::now() < deadline, "client did not finish");
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("accept: {e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut headers = String::new();
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    assert_ne!(reader.read_line(&mut header).unwrap(), 0);
                    if header == "\r\n" {
                        break;
                    }
                    if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:")
                    {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                    headers.push_str(&header);
                    assert!(headers.len() <= 32768);
                }
                assert!(length <= 131072);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                requests.push(Request {
                    line,
                    headers,
                    body,
                });
                if let Some(response) = &response {
                    let _ = stream.write_all(response);
                }
            }
            requests
        });
        Self {
            origin,
            stop,
            worker: Some(worker),
        }
    }
    fn json(status: u16, body: Value) -> Self {
        Self::new(Some(response(status, &body.to_string(), "")))
    }
    fn finish(mut self) -> Vec<Request> {
        self.stop.store(true, Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap()
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn response(status: u16, body: &str, extra: &str) -> Vec<u8> {
    format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}", body.len()).into_bytes()
}
fn invoke(origin: &str, token: &[u8], args: &[&str], input: &[u8], from_file: bool) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let token_path = dir.path().join("token");
    let input_path = dir.path().join("input.json");
    std::fs::write(&token_path, token).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_trust"));
    command
        .args([
            "--url",
            origin,
            "--token-file",
            token_path.to_str().unwrap(),
        ])
        .args(args)
        .env_remove("TRUST_SERVICE_URL")
        .env_remove("TRUST_ACCESS_TOKEN_FILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if from_file {
        std::fs::write(&input_path, input).unwrap();
        command.args(["--input", input_path.to_str().unwrap()]);
    }
    let mut child = command.spawn().unwrap();
    if !from_file {
        if let Err(e) = child.stdin.take().unwrap().write_all(input) {
            assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe);
        }
    } else {
        drop(child.stdin.take());
    }
    let output = child.wait_with_output().unwrap();
    for bytes in [&output.stdout, &output.stderr] {
        assert!(
            !String::from_utf8_lossy(bytes).contains(TOKEN),
            "credential leaked"
        );
    }
    output
}
#[test]
fn discovery_status_skills_and_calls_preserve_the_public_http_contract() {
    let payload = json!({"account_id":"account-fixture","amount":"0.00000001","idempotency_key":"original-mutation-key"});
    for (args, path, body) in [
        (vec!["tools"], "GET /v1/tools HTTP/1.1\r\n", None),
        (
            vec!["status"],
            "POST /v1/tools/service_status HTTP/1.1\r\n",
            Some(json!({})),
        ),
        (
            vec!["skill", "trust-trading"],
            "POST /v1/tools/skill_read HTTP/1.1\r\n",
            Some(json!({"name":"trust-trading"})),
        ),
        (
            vec!["call", "capital_deposit"],
            "POST /v1/tools/capital_deposit HTTP/1.1\r\n",
            Some(payload.clone()),
        ),
    ] {
        for from_file in [false, true] {
            if from_file && args[0] != "call" {
                continue;
            }
            let expected =
                json!({"schema_version":1,"data":{"accepted":true},"error":null,"next_actions":[]});
            let service = Service::json(200, expected.clone());
            let output = invoke(
                &service.origin,
                format!("{TOKEN}\n").as_bytes(),
                &args,
                payload.to_string().as_bytes(),
                from_file,
            );
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stdout).unwrap(),
                expected
            );
            assert!(output.stderr.is_empty());
            let requests = service.finish();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].line, path);
            assert!(requests[0]
                .headers
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {TOKEN}\r\n")));
            match &body {
                Some(body) => assert_eq!(
                    serde_json::from_slice::<Value>(&requests[0].body).unwrap(),
                    *body
                ),
                None => assert!(requests[0].body.is_empty()),
            }
        }
    }
}
#[test]
fn service_rejections_fail_the_command_and_keep_actionable_guidance() {
    for status in [200, 403, 503] {
        let expected = json!({"data":null,"error":{"code":"risk_rejected","message":"Risk cap exceeded","next_action":"Reduce quantity"},"next_actions":["risk_preview"]});
        let service = Service::json(status, expected.clone());
        let output = invoke(
            &service.origin,
            TOKEN.as_bytes(),
            &["call", "trade_fund"],
            br#"{"idempotency_key":"original-key"}"#,
            false,
        );
        assert!(!output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            expected
        );
        assert_eq!(service.finish().len(), 1);
    }
}
#[test]
fn non_success_http_status_cannot_be_reported_as_success_by_a_null_error_envelope() {
    for status in [302, 401, 403, 429, 500] {
        let service = Service::json(status, json!({"data":{"accepted":false},"error":null}));
        let output = invoke(&service.origin, TOKEN.as_bytes(), &["status"], b"", false);
        assert!(
            !output.status.success(),
            "HTTP {status} was reported as success"
        );
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("rejected"));
        assert_eq!(service.finish().len(), 1);
    }
}
#[test]
fn redirects_never_forward_credentials_or_mutations_to_another_origin() {
    let destination = Service::json(200, json!({"data":"must not be contacted"}));
    let service = Service::new(Some(response(
        307,
        "{}",
        &format!("Location: {}/stolen\r\n", destination.origin),
    )));
    let output = invoke(
        &service.origin,
        TOKEN.as_bytes(),
        &["call", "capital_deposit"],
        br#"{"amount":"100","idempotency_key":"original-key"}"#,
        false,
    );
    assert!(!output.status.success());
    assert_eq!(service.finish().len(), 1);
    assert!(destination.finish().is_empty());
}
#[test]
fn lost_mutation_response_sends_once_and_requires_reconciliation_with_the_original_key() {
    let service = Service::new(None);
    let payload = json!({"amount":"100","idempotency_key":"original-key"});
    let output = invoke(
        &service.origin,
        TOKEN.as_bytes(),
        &["call", "capital_deposit"],
        payload.to_string().as_bytes(),
        false,
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("operation_get") && message.contains("original idempotency key"));
    let requests = service.finish();
    assert_eq!(requests.len(), 1, "ambiguous mutations must not be retried");
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).unwrap(),
        payload
    );
}
#[test]
fn malformed_inputs_and_credentials_are_rejected_before_any_request() {
    let service = Service::json(200, json!({"data":"must not be contacted"}));
    for input in [b"[]".as_slice(), b"null", b"{invalid", b"1", b"\"text\""] {
        let output = invoke(
            &service.origin,
            TOKEN.as_bytes(),
            &["call", "trade_fund"],
            input,
            false,
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    for tool in [
        "../status",
        "status?token=secret",
        "UPPERCASE",
        "contains-dash",
    ] {
        assert!(!invoke(
            &service.origin,
            TOKEN.as_bytes(),
            &["call", tool],
            b"{}",
            false
        )
        .status
        .success());
    }
    for token in [
        vec![],
        b"two tokens".to_vec(),
        b"two\nlines".to_vec(),
        vec![0xff],
        vec![b'x'; 16385],
    ] {
        assert!(!invoke(&service.origin, &token, &["status"], b"", false)
            .status
            .success());
    }
    for from_file in [false, true] {
        assert!(!invoke(
            &service.origin,
            TOKEN.as_bytes(),
            &["call", "trade_fund"],
            &vec![b' '; 131073],
            from_file
        )
        .status
        .success());
    }
    assert!(service.finish().is_empty());
}
#[test]
fn malformed_or_oversized_service_responses_fail_without_inventing_a_result() {
    for body in [
        "not json".to_owned(),
        format!("{{\"data\":\"{}\"}}", "x".repeat(1_048_577)),
    ] {
        let service = Service::new(Some(response(200, &body, "")));
        let output = invoke(&service.origin, TOKEN.as_bytes(), &["status"], b"", false);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
        assert_eq!(service.finish().len(), 1);
    }
}
#[test]
fn unsafe_origins_are_rejected_before_reading_credentials() {
    // Generate synthetic credentials at runtime so the fixture contains no
    // credential-bearing endpoint for scanners to attempt to authenticate.
    let password = uuid::Uuid::new_v4().to_string();
    let mut credential_origin = reqwest::Url::parse("http://localhost").unwrap();
    credential_origin.set_username("test-owner").unwrap();
    credential_origin.set_password(Some(&password)).unwrap();
    for origin in [
        "http://example.com",
        credential_origin.as_str(),
        "http://localhost/path",
        "http://localhost?token=secret",
        "http://localhost#fragment",
        "ftp://localhost",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_trust"))
            .args(["--url", origin, "--token-file", "/not/a/token", "status"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("HTTPS"), "{error}");
        assert!(!error.contains(&password) && !error.contains("token=secret"));
    }
}

#[test]
fn operator_environment_selects_service_and_token_without_exposing_them() {
    let service = Service::json(200, json!({"data":{"ready":true},"error":null}));
    let directory = tempfile::tempdir().unwrap();
    let token = directory.path().join("token");
    std::fs::write(&token, TOKEN).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_trust"))
        .arg("status")
        .env("TRUST_SERVICE_URL", &service.origin)
        .env("TRUST_ACCESS_TOKEN_FILE", &token)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["data"]["ready"],
        true
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(TOKEN));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(TOKEN));
    let requests = service.finish();
    assert_eq!(requests.len(), 1);
    assert!(requests[0]
        .headers
        .to_ascii_lowercase()
        .contains(&format!("authorization: bearer {TOKEN}\r\n")));
}
