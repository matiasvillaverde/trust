//! Inter-Process Communication protocol for daemon communication

use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Commands that can be sent from the CLI to the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonCommand {
    /// Request the current status of the daemon
    GetStatus,
    /// Initiate graceful shutdown
    Shutdown,
    /// Force a reconciliation with the broker
    ForceReconcile,
}

/// Responses from the daemon to CLI commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    /// Command executed successfully
    Ok,
    /// Status information
    Status {
        /// Current state of the broker connection
        state: String,
        /// Whether the WebSocket is connected
        is_connected: bool,
        /// Time the daemon has been running
        uptime_secs: u64,
        /// Number of active account syncs
        active_accounts: usize,
        /// Last successful sync timestamp
        last_sync: Option<i64>,
    },
    /// Error occurred
    Error(String),
}

/// Configuration for the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Path to the Unix socket
    pub socket_path: String,
    /// Path to the PID file
    pub pid_path: String,
    /// Path to the log file
    pub log_path: String,
    /// Directory for daemon files
    pub daemon_dir: String,
}

impl DaemonConfig {
    /// Create default daemon configuration
    pub fn new() -> anyhow::Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

        let daemon_dir = home.join(".trust").join("daemon");
        let daemon_dir_str = daemon_dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path encoding"))?
            .to_string();

        Ok(Self {
            socket_path: daemon_dir
                .join("broker-sync.sock")
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid socket path"))?
                .to_string(),
            pid_path: daemon_dir
                .join("broker-sync.pid")
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid PID path"))?
                .to_string(),
            log_path: daemon_dir
                .join("broker-sync.log")
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid log path"))?
                .to_string(),
            daemon_dir: daemon_dir_str,
        })
    }

    /// Ensure the daemon directory exists
    pub fn ensure_directories(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.daemon_dir)?;
        Ok(())
    }
}

/// IPC message wrapper for framing
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcMessage<T> {
    /// Message ID for correlation
    pub id: Uuid,
    /// The actual payload
    pub payload: T,
}

impl<T> IpcMessage<T> {
    /// Create a new IPC message with a random ID
    pub fn new(payload: T) -> Self {
        Self {
            id: Uuid::new_v4(),
            payload,
        }
    }
}

/// Timeout for IPC operations
pub const IPC_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_command_serialization() {
        let cmd = DaemonCommand::GetStatus;
        let serialized = bincode::serialize(&cmd).unwrap();
        let deserialized: DaemonCommand = bincode::deserialize(&serialized).unwrap();
        matches!(deserialized, DaemonCommand::GetStatus);
    }

    #[test]
    fn test_daemon_response_serialization() {
        let resp = DaemonResponse::Status {
            state: "Live".to_string(),
            is_connected: true,
            uptime_secs: 3600,
            active_accounts: 2,
            last_sync: Some(1234567890),
        };
        let serialized = bincode::serialize(&resp).unwrap();
        let deserialized: DaemonResponse = bincode::deserialize(&serialized).unwrap();

        if let DaemonResponse::Status { is_connected, .. } = deserialized {
            assert!(is_connected);
        } else {
            panic!("Expected Status response");
        }
    }

    #[test]
    fn test_ipc_message_creation() {
        let msg = IpcMessage::new(DaemonCommand::Shutdown);
        assert!(!msg.id.is_nil());
        matches!(msg.payload, DaemonCommand::Shutdown);
    }
}
