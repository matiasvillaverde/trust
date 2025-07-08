//! BrokerSync actor for real-time Alpaca WebSocket integration
//!
//! This crate implements the actor-based system for managing
//! WebSocket connections and synchronizing state with Alpaca.

pub mod daemon;
pub mod ipc;
pub mod mock_alpaca;
pub mod state;
pub mod websocket_sync;

// Re-export public types
pub use daemon::{is_daemon_running, BrokerSyncDaemon, DaemonClient};
pub use ipc::{DaemonCommand, DaemonConfig, DaemonResponse};
pub use mock_alpaca::{AlpacaMessage, AlpacaMessageData, MockAlpacaOrder, MockAlpacaServer};
pub use state::{BrokerState, StateTransition};
pub use websocket_sync::WebSocketSync;
