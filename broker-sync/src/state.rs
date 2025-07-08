//! State machine for BrokerSync actor

use std::time::{Duration, Instant};
use thiserror::Error;

/// Errors that can occur during state transitions
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum StateError {
    #[error("Invalid transition: {from:?} cannot transition via {transition:?}")]
    InvalidTransition {
        from: BrokerState,
        transition: StateTransition,
    },
}

/// States for the broker connection lifecycle
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerState {
    /// Not connected to WebSocket
    Disconnected,
    /// Attempting to establish WebSocket connection
    Connecting,
    /// Connected, reconciling existing orders
    Reconciling { start_time: Instant },
    /// Fully operational, receiving real-time updates
    Live { connected_since: Instant },
    /// Connection failed, waiting to retry
    ErrorRecovery { attempt: u32, next_retry: Instant },
}

/// State transitions for the broker state machine
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateTransition {
    /// Start connection attempt
    Connect,
    /// WebSocket connection established
    ConnectionEstablished,
    /// Reconciliation completed successfully
    ReconciliationComplete,
    /// Error occurred
    Error,
    /// Retry connection after error
    RetryConnection,
    /// Start reconciliation process
    StartReconciliation,
    /// Disconnect from broker
    Disconnect,
}

impl BrokerState {
    /// Transition to a new state based on the given event
    pub fn transition(self, event: StateTransition) -> Result<Self, StateError> {
        self.transition_at(event, Instant::now())
    }

    /// Transition to a new state with a specific timestamp (for testing)
    pub fn transition_at(self, event: StateTransition, now: Instant) -> Result<Self, StateError> {
        match (&self, &event) {
            // From Disconnected
            (BrokerState::Disconnected, StateTransition::Connect) => Ok(BrokerState::Connecting),

            // From Connecting
            (BrokerState::Connecting, StateTransition::ConnectionEstablished) => {
                Ok(BrokerState::Reconciling { start_time: now })
            }

            // From Reconciling
            (BrokerState::Reconciling { .. }, StateTransition::ReconciliationComplete) => {
                Ok(BrokerState::Live {
                    connected_since: now,
                })
            }

            // From Live
            (BrokerState::Live { .. }, StateTransition::StartReconciliation) => {
                Ok(BrokerState::Reconciling { start_time: now })
            }

            // From ErrorRecovery
            (BrokerState::ErrorRecovery { attempt, .. }, StateTransition::RetryConnection) => {
                Ok(BrokerState::ErrorRecovery {
                    attempt: attempt + 1,
                    next_retry: now + Self::calculate_backoff(attempt + 1),
                })
            }
            (BrokerState::ErrorRecovery { .. }, StateTransition::Connect) => {
                Ok(BrokerState::Connecting)
            }

            // Error transition from any state
            (_, StateTransition::Error) => Ok(BrokerState::ErrorRecovery {
                attempt: 1,
                next_retry: now + Self::calculate_backoff(1),
            }),

            // Invalid transitions return error
            (state, transition) => Err(StateError::InvalidTransition {
                from: state.clone(),
                transition: transition.clone(),
            }),
        }
    }

    /// Check if the broker is connected to the WebSocket
    pub fn is_connected(&self) -> bool {
        matches!(self, BrokerState::Live { .. })
    }

    /// Get the duration since connection was established
    pub fn connection_duration(&self) -> Option<Duration> {
        match self {
            BrokerState::Live { connected_since } => Some(connected_since.elapsed()),
            _ => None,
        }
    }

    /// Get the backoff duration for error recovery
    pub fn backoff_duration(&self) -> Duration {
        match self {
            BrokerState::ErrorRecovery { attempt, .. } => Self::calculate_backoff(*attempt),
            _ => Duration::from_secs(0),
        }
    }

    /// Calculate exponential backoff with cap
    fn calculate_backoff(attempt: u32) -> Duration {
        let base = 1; // 1 second base
        let exponent = (attempt - 1).min(4); // Cap at 2^4 = 16 seconds
        Duration::from_secs(base * 2u64.pow(exponent))
    }
}
