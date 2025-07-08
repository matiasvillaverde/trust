//! Integration tests for BrokerSync daemon infrastructure

use broker_sync::{is_daemon_running, DaemonClient, DaemonCommand, DaemonConfig, DaemonResponse};
use std::fs;
use tempfile::TempDir;
// use tokio::time::{sleep, Duration};

/// Create a test configuration
fn create_test_config() -> (DaemonConfig, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let daemon_dir = temp_dir.path().join("daemon");

    let config = DaemonConfig {
        socket_path: daemon_dir.join("test.sock").to_str().unwrap().to_string(),
        pid_path: daemon_dir.join("test.pid").to_str().unwrap().to_string(),
        log_path: daemon_dir.join("test.log").to_str().unwrap().to_string(),
        daemon_dir: daemon_dir.to_str().unwrap().to_string(),
    };

    (config, temp_dir)
}

#[tokio::test]
async fn test_daemon_not_running_initially() {
    let (config, _temp_dir) = create_test_config();

    // Daemon should not be running initially
    assert!(!is_daemon_running(&config));
}

#[tokio::test]
async fn test_daemon_config_creation() {
    let config = DaemonConfig::new().unwrap();

    // Should have reasonable defaults
    assert!(config.socket_path.contains(".trust"));
    assert!(config.pid_path.contains(".trust"));
    assert!(config.log_path.contains(".trust"));
    assert!(config.daemon_dir.contains(".trust"));
}

#[tokio::test]
async fn test_daemon_config_directory_creation() {
    let (config, _temp_dir) = create_test_config();

    // Directory should not exist initially
    assert!(!std::path::Path::new(&config.daemon_dir).exists());

    // ensure_directories should create it
    config.ensure_directories().unwrap();
    assert!(std::path::Path::new(&config.daemon_dir).exists());
}

#[tokio::test]
async fn test_daemon_client_creation() {
    let (config, _temp_dir) = create_test_config();

    // Should be able to create a client
    let _client = DaemonClient::new(config);
}

#[tokio::test]
async fn test_daemon_client_connection_failure() {
    let (config, _temp_dir) = create_test_config();

    let client = DaemonClient::new(config);

    // Should fail to connect to non-existent daemon
    let result = client.send_command(DaemonCommand::GetStatus).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pid_file_detection() {
    let (config, _temp_dir) = create_test_config();
    config.ensure_directories().unwrap();

    // No PID file initially
    assert!(!is_daemon_running(&config));

    // Create a fake PID file
    fs::write(&config.pid_path, "12345").unwrap();

    // Should detect the PID file (but process might not be running)
    // This is system-dependent behavior
    let _running = is_daemon_running(&config);
}

#[tokio::test]
async fn test_message_serialization() {
    let command = DaemonCommand::GetStatus;
    let serialized = bincode::serialize(&command).unwrap();
    let deserialized: DaemonCommand = bincode::deserialize(&serialized).unwrap();

    assert!(matches!(deserialized, DaemonCommand::GetStatus));
}

#[tokio::test]
async fn test_response_serialization() {
    let response = DaemonResponse::Status {
        state: "Live".to_string(),
        is_connected: true,
        uptime_secs: 3600,
        active_accounts: 1,
        last_sync: Some(1234567890),
    };

    let serialized = bincode::serialize(&response).unwrap();
    let deserialized: DaemonResponse = bincode::deserialize(&serialized).unwrap();

    if let DaemonResponse::Status {
        is_connected,
        uptime_secs,
        ..
    } = deserialized
    {
        assert!(is_connected);
        assert_eq!(uptime_secs, 3600);
    } else {
        panic!("Expected Status response");
    }
}
