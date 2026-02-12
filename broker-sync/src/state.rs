//! State machine for BrokerSync actor

use std::time::{Duration, Instant};
use thiserror::Error;

/// Configuration for backoff behavior
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackoffConfig {
    /// Base delay in milliseconds
    pub base_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Maximum exponent for exponential backoff
    pub max_exponent: u32,
    /// Jitter percentage (0-100)
    pub jitter_percent: u32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base_delay_ms: 1000,  // 1 second
            max_delay_ms: 60_000, // 60 seconds
            max_exponent: 6,      // 2^6 = 64x base
            jitter_percent: 20,   // +/- 20%
        }
    }
}

/// Errors that can occur during state transitions
#[derive(Debug, Clone, Error, PartialEq)]
pub enum StateError {
    #[error("Invalid transition: {from:?} cannot transition via {transition:?}")]
    InvalidTransition {
        from: BrokerState,
        transition: StateTransition,
    },
}

/// States for the broker connection lifecycle
#[derive(Debug, Clone, PartialEq)]
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
    ErrorRecovery {
        attempt: u32,
        next_retry: Instant,
        config: BackoffConfig,
    },
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
            (
                BrokerState::ErrorRecovery {
                    attempt, config, ..
                },
                StateTransition::RetryConnection,
            ) => Ok(BrokerState::ErrorRecovery {
                attempt: attempt + 1,
                next_retry: now + Self::calculate_backoff_with_config(attempt + 1, config),
                config: config.clone(),
            }),
            (BrokerState::ErrorRecovery { .. }, StateTransition::Connect) => {
                Ok(BrokerState::Connecting)
            }

            // Error transition from any state
            (_, StateTransition::Error) => {
                let config = BackoffConfig::default();
                Ok(BrokerState::ErrorRecovery {
                    attempt: 1,
                    next_retry: now + Self::calculate_backoff_with_config(1, &config),
                    config,
                })
            }

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
            BrokerState::ErrorRecovery {
                attempt, config, ..
            } => Self::calculate_backoff_with_config(*attempt, config),
            _ => Duration::from_secs(0),
        }
    }

    /// Calculate exponential backoff with configuration
    fn calculate_backoff_with_config(attempt: u32, config: &BackoffConfig) -> Duration {
        // Calculate exponential delay with cap
        let exponent = (attempt - 1).min(config.max_exponent);
        let delay_ms = config
            .base_delay_ms
            .saturating_mul(2u64.pow(exponent))
            .min(config.max_delay_ms);

        // Add jitter to prevent thundering herd
        let jitter_range = (delay_ms * config.jitter_percent as u64) / 100;
        let final_delay = Self::apply_jitter(delay_ms, jitter_range, config.max_delay_ms);

        Duration::from_millis(final_delay)
    }

    /// Apply jitter to delay value
    /// Returns a value with random jitter applied, clamped to [0, max_delay]
    fn apply_jitter(delay_ms: u64, jitter_range: u64, max_delay_ms: u64) -> u64 {
        if jitter_range == 0 {
            return delay_ms.min(max_delay_ms);
        }

        // For deterministic testing, use a simpler approach when jitter is disabled
        #[cfg(test)]
        if std::env::var("BROKER_SYNC_DETERMINISTIC").is_ok() {
            return delay_ms.min(max_delay_ms);
        }

        // Generate random jitter in range [-jitter_range, +jitter_range]
        let jitter_i64 = rand::random_range(-(jitter_range as i64)..=(jitter_range as i64));

        // Apply jitter and clamp to valid range
        let jittered_delay = if jitter_i64 < 0 {
            delay_ms.saturating_sub((-jitter_i64) as u64)
        } else {
            delay_ms.saturating_add(jitter_i64 as u64)
        };

        // Ensure we don't exceed max delay and have minimum of 100ms
        jittered_delay.max(100).min(max_delay_ms)
    }
}
