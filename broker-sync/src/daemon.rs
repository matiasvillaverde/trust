//! Daemon process management for BrokerSync

use crate::ipc::{DaemonCommand, DaemonConfig, DaemonResponse, IpcMessage};
use crate::state::{BrokerState, StateTransition};
use crate::websocket_sync::WebSocketSync;
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info};

/// The main daemon process that hosts the BrokerSync actor
pub struct BrokerSyncDaemon {
    config: DaemonConfig,
    state: Arc<Mutex<BrokerState>>,
    start_time: Instant,
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Arc<Mutex<mpsc::Receiver<()>>>,
}

/// Handler for individual IPC connections
struct ConnectionHandler {
    state: Arc<Mutex<BrokerState>>,
    start_time: Instant,
    shutdown_tx: mpsc::Sender<()>,
}

impl BrokerSyncDaemon {
    /// Create a new daemon instance
    pub fn new(config: DaemonConfig) -> Result<Self> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Ok(Self {
            config,
            state: Arc::new(Mutex::new(BrokerState::Disconnected)),
            start_time: Instant::now(),
            shutdown_tx,
            shutdown_rx: Arc::new(Mutex::new(shutdown_rx)),
        })
    }

    /// Run the daemon process
    pub async fn run(self) -> Result<()> {
        info!("Starting BrokerSync daemon");

        // Ensure daemon directory exists
        self.config.ensure_directories()?;

        // Write PID file
        self.write_pid_file()?;

        // Clean up any existing socket
        if Path::new(&self.config.socket_path).exists() {
            fs::remove_file(&self.config.socket_path)?;
        }

        // Create Unix listener
        let listener = UnixListener::bind(&self.config.socket_path)?;
        info!("Listening on socket: {}", self.config.socket_path);

        // Create Arc for shared access
        let daemon = Arc::new(self);

        // Spawn the IPC handler
        let daemon_clone = daemon.clone();
        let ipc_handle = tokio::spawn(async move { daemon_clone.handle_ipc(listener).await });

        // Start WebSocket sync in the same task to avoid Send issues
        // This runs concurrently with IPC handling
        let mut shutdown_rx = daemon.shutdown_rx.lock().await;
        tokio::select! {
            // Run WebSocket sync
            _ = daemon.start_websocket_sync() => {
                info!("WebSocket sync finished");
            }

            // Wait for shutdown signal
            _ = shutdown_rx.recv() => {
                info!("Received shutdown signal");
            }
        }

        info!("Shutting down daemon");

        // Clean up
        daemon.cleanup()?;

        // Wait for IPC handler to finish
        let _ = ipc_handle.await;

        Ok(())
    }

    /// Handle IPC connections
    async fn handle_ipc(&self, listener: UnixListener) -> Result<()> {
        loop {
            tokio::select! {
                // Accept new connections
                Ok((stream, _)) = listener.accept() => {
                    let daemon = self.clone_for_connection();
                    tokio::spawn(async move {
                        if let Err(e) = daemon.handle_connection(stream).await {
                            error!("Error handling connection: {}", e);
                        }
                    });
                }

                // Check if we should shutdown
                _ = self.shutdown_tx.closed() => {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Clone necessary data for handling a connection
    fn clone_for_connection(&self) -> ConnectionHandler {
        ConnectionHandler {
            state: self.state.clone(),
            start_time: self.start_time,
            shutdown_tx: self.shutdown_tx.clone(),
        }
    }

    /// Write the PID file
    fn write_pid_file(&self) -> Result<()> {
        let pid = process::id();
        fs::write(&self.config.pid_path, pid.to_string())?;
        Ok(())
    }

    /// Clean up daemon files
    fn cleanup(&self) -> Result<()> {
        // Remove PID file
        if Path::new(&self.config.pid_path).exists() {
            fs::remove_file(&self.config.pid_path)?;
        }

        // Remove socket file
        if Path::new(&self.config.socket_path).exists() {
            fs::remove_file(&self.config.socket_path)?;
        }

        Ok(())
    }

    /// Start WebSocket synchronization with Alpaca
    async fn start_websocket_sync(&self) -> Result<()> {
        // TODO: Get WebSocket URL from configuration
        // For now, use localhost for testing with mock server
        let websocket_url = "ws://127.0.0.1:8080".to_string();

        // Create WebSocket sync without database for now to avoid Send issues
        // In a real implementation, you'd use a Send-safe database wrapper
        let websocket_sync = WebSocketSync::new(websocket_url, self.state.clone());

        // Start the sync with retry logic
        loop {
            tokio::select! {
                result = websocket_sync.start() => {
                    match result {
                        Ok(_) => {
                            info!("WebSocket sync completed");
                            break;
                        }
                        Err(e) => {
                            error!("WebSocket sync error: {}", e);

                            // Update state to error recovery
                            {
                                let mut state = self.state.lock().await;
                                *state = state.clone().transition(StateTransition::Error);
                            }

                            // Wait before retrying
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                            // Check if we should shutdown
                            if self.shutdown_tx.is_closed() {
                                break;
                            }
                        }
                    }
                }

                // Check for shutdown signal
                _ = self.shutdown_tx.closed() => {
                    info!("WebSocket sync shutting down");
                    break;
                }
            }
        }

        Ok(())
    }
}

impl ConnectionHandler {
    /// Handle a single IPC connection
    async fn handle_connection(&self, mut stream: UnixStream) -> Result<()> {
        // Read message length (4 bytes)
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let msg_len = u32::from_le_bytes(len_buf) as usize;

        // Read message
        let mut msg_buf = vec![0u8; msg_len];
        stream.read_exact(&mut msg_buf).await?;

        // Deserialize command
        let message: IpcMessage<DaemonCommand> = bincode::deserialize(&msg_buf)?;

        // Process command
        let response = self.handle_command(message.payload).await?;

        // Create response message
        let response_msg = IpcMessage {
            id: message.id,
            payload: response,
        };

        // Serialize response
        let response_buf = bincode::serialize(&response_msg)?;

        // Write response length and data
        let len_buf = (response_buf.len() as u32).to_le_bytes();
        stream.write_all(&len_buf).await?;
        stream.write_all(&response_buf).await?;
        stream.flush().await?;

        Ok(())
    }

    /// Handle a daemon command
    async fn handle_command(&self, command: DaemonCommand) -> Result<DaemonResponse> {
        match command {
            DaemonCommand::GetStatus => {
                let state = self.state.lock().await;
                let uptime_secs = self.start_time.elapsed().as_secs();

                Ok(DaemonResponse::Status {
                    state: format!("{:?}", *state),
                    is_connected: state.is_connected(),
                    uptime_secs,
                    active_accounts: 0, // TODO: Track active accounts
                    last_sync: None,    // TODO: Track last sync time
                })
            }

            DaemonCommand::Shutdown => {
                info!("Received shutdown command");
                self.shutdown_tx.send(()).await?;
                Ok(DaemonResponse::Ok)
            }

            DaemonCommand::ForceReconcile => {
                let mut state = self.state.lock().await;
                *state = state
                    .clone()
                    .transition(StateTransition::StartReconciliation);
                Ok(DaemonResponse::Ok)
            }
        }
    }
}

/// Check if the daemon is running
pub fn is_daemon_running(config: &DaemonConfig) -> bool {
    // Check if PID file exists
    if !Path::new(&config.pid_path).exists() {
        return false;
    }

    // Read PID
    let pid_str = match fs::read_to_string(&config.pid_path) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => return false,
    };

    // Check if process is alive (Unix-specific)
    #[cfg(unix)]
    {
        // Send signal 0 to check if process exists
        match std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output()
        {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    #[cfg(not(unix))]
    {
        // For non-Unix systems, just check if PID file exists
        // This is less reliable but better than nothing
        true
    }
}

/// Client for communicating with the daemon
pub struct DaemonClient {
    config: DaemonConfig,
}

impl DaemonClient {
    /// Create a new daemon client
    pub fn new(config: DaemonConfig) -> Self {
        Self { config }
    }

    /// Send a command to the daemon and wait for response
    pub async fn send_command(&self, command: DaemonCommand) -> Result<DaemonResponse> {
        // Connect to daemon
        let mut stream = UnixStream::connect(&self.config.socket_path).await?;

        // Create message
        let message = IpcMessage::new(command);

        // Serialize message
        let msg_buf = bincode::serialize(&message)?;

        // Send message length and data
        let len_buf = (msg_buf.len() as u32).to_le_bytes();
        stream.write_all(&len_buf).await?;
        stream.write_all(&msg_buf).await?;
        stream.flush().await?;

        // Read response length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let response_len = u32::from_le_bytes(len_buf) as usize;

        // Read response
        let mut response_buf = vec![0u8; response_len];
        stream.read_exact(&mut response_buf).await?;

        // Deserialize response
        let response_msg: IpcMessage<DaemonResponse> = bincode::deserialize(&response_buf)?;

        Ok(response_msg.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config() -> Result<(DaemonConfig, TempDir)> {
        let temp_dir = TempDir::new()?;
        let daemon_dir = temp_dir.path().join("daemon");

        Ok((
            DaemonConfig {
                socket_path: daemon_dir.join("test.sock").to_str().unwrap().to_string(),
                pid_path: daemon_dir.join("test.pid").to_str().unwrap().to_string(),
                log_path: daemon_dir.join("test.log").to_str().unwrap().to_string(),
                daemon_dir: daemon_dir.to_str().unwrap().to_string(),
            },
            temp_dir,
        ))
    }

    #[tokio::test]
    async fn test_daemon_lifecycle() {
        let (config, _temp_dir) = test_config().unwrap();
        config.ensure_directories().unwrap();

        // Daemon should not be running initially
        assert!(!is_daemon_running(&config));
    }
}
