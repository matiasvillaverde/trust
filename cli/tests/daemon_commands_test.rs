//! Tests for daemon CLI commands

use std::process::Command;

#[test]
fn test_daemon_help_command() {
    let output = Command::new("cargo")
        .args(["run", "-p", "cli", "--", "daemon", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Manage the BrokerSync daemon process"));
    assert!(stdout.contains("start"));
    assert!(stdout.contains("stop"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("restart"));
}

#[test]
fn test_daemon_start_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "cli", "--", "daemon", "start", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BrokerSync daemon"));
    assert!(stdout.contains("real-time"));
}

#[test]
fn test_daemon_stop_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "cli", "--", "daemon", "stop", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BrokerSync daemon"));
    assert!(stdout.contains("Gracefully"));
}

#[test]
fn test_daemon_status_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "cli", "--", "daemon", "status", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BrokerSync daemon"));
    assert!(stdout.contains("status"));
}
