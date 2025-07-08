//! BrokerSync actor for real-time Alpaca WebSocket integration
//!
//! This crate will implement the actor-based system for managing
//! WebSocket connections and synchronizing state with Alpaca.

mod messages;
mod state;

// Re-export public types
pub use messages::{BrokerCommand, BrokerEvent, OrderUpdate, ReconciliationStatus};
pub use state::{BackoffConfig, BrokerState, StateError, StateTransition};

/// The main BrokerSync actor struct
pub struct BrokerSync;
