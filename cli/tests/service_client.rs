use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    process::Command,
};
#[test]
fn default_cli_calls_the_service_and_never_prints_its_token() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut connection, _) = listener.accept().unwrap();
        connection
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(connection.try_clone().unwrap());
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            headers.push_str(&line);
        }
        assert!(headers.starts_with("POST /v1/tools/service_status HTTP/1.1"));
        assert!(headers
            .to_ascii_lowercase()
            .contains("authorization: bearer contract-test-token"));
        let body =
            r#"{"schema_version":1,"data":{"service":"trust"},"error":null,"next_actions":[]}"#;
        write!(connection,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let file = std::env::temp_dir().join(format!("trust-client-{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&file, "contract-test-token\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_trust"))
        .args([
            "--url",
            &format!("http://{address}"),
            "--token-file",
            file.to_str().unwrap(),
            "status",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(file).unwrap();
    server.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("contract-test-token"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("contract-test-token"));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["service"], "trust");
}
#[test]
fn cli_rejects_remote_plain_http_before_reading_credentials() {
    let output = Command::new(env!("CARGO_BIN_EXE_trust"))
        .args([
            "--url",
            "http://example.com",
            "--token-file",
            "/not/a/token",
            "status",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HTTPS"));
}
