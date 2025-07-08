//! Tests for BrokerState state machine
//! Following TDD - tests written first

use broker_sync::{BrokerState, StateError, StateTransition};
use std::time::{Duration, Instant};

#[test]
fn test_all_states_exist() {
    // Verify all required states exist
    let _disconnected = BrokerState::Disconnected;
    let _connecting = BrokerState::Connecting;
    let _reconciling = BrokerState::Reconciling {
        start_time: Instant::now(),
    };
    let _live = BrokerState::Live {
        connected_since: Instant::now(),
    };
    let _error_recovery = BrokerState::ErrorRecovery {
        attempt: 1,
        next_retry: Instant::now() + Duration::from_secs(5),
        config: broker_sync::BackoffConfig::default(),
    };
}

#[test]
fn test_state_implements_debug() {
    // States should be debuggable
    let state = BrokerState::Disconnected;
    let debug_str = format!("{state:?}");
    assert!(debug_str.contains("Disconnected"));
}

#[test]
fn test_state_implements_clone() {
    // States should be cloneable
    let state = BrokerState::Disconnected;
    let cloned = state.clone();
    assert!(matches!(cloned, BrokerState::Disconnected));
}

#[test]
fn test_state_implements_eq() {
    // States should be comparable
    let state1 = BrokerState::Disconnected;
    let state2 = BrokerState::Disconnected;
    assert_eq!(state1, state2);
}

#[test]
fn test_disconnected_can_transition_to_connecting() {
    let state = BrokerState::Disconnected;
    let next = state.transition(StateTransition::Connect).unwrap();
    assert!(matches!(next, BrokerState::Connecting));
}

#[test]
fn test_connecting_can_transition_to_reconciling() {
    let state = BrokerState::Connecting;
    let next = state
        .transition(StateTransition::ConnectionEstablished)
        .unwrap();
    assert!(matches!(next, BrokerState::Reconciling { .. }));
}

#[test]
fn test_reconciling_can_transition_to_live() {
    let state = BrokerState::Reconciling {
        start_time: Instant::now(),
    };
    let next = state
        .transition(StateTransition::ReconciliationComplete)
        .unwrap();
    assert!(matches!(next, BrokerState::Live { .. }));
}

#[test]
fn test_any_state_can_transition_to_error_recovery() {
    let states = vec![
        BrokerState::Disconnected,
        BrokerState::Connecting,
        BrokerState::Reconciling {
            start_time: Instant::now(),
        },
        BrokerState::Live {
            connected_since: Instant::now(),
        },
    ];

    for state in states {
        let next = state.clone().transition(StateTransition::Error).unwrap();
        assert!(matches!(next, BrokerState::ErrorRecovery { .. }));
    }
}

#[test]
fn test_error_recovery_increments_attempt_count() {
    let state = BrokerState::ErrorRecovery {
        attempt: 1,
        next_retry: Instant::now(),
        config: broker_sync::BackoffConfig::default(),
    };
    let next = state.transition(StateTransition::RetryConnection).unwrap();

    if let BrokerState::ErrorRecovery { attempt, .. } = next {
        assert_eq!(attempt, 2);
    } else {
        panic!("Expected ErrorRecovery state");
    }
}

#[test]
fn test_invalid_transition_disconnected_to_reconciling() {
    let state = BrokerState::Disconnected;
    let result = state.transition(StateTransition::ReconciliationComplete);

    assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
}

#[test]
fn test_invalid_transition_connecting_to_live() {
    let state = BrokerState::Connecting;
    let result = state.transition(StateTransition::ReconciliationComplete);

    assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
}

#[test]
fn test_live_can_transition_to_reconciling() {
    let state = BrokerState::Live {
        connected_since: Instant::now(),
    };
    let next = state
        .transition(StateTransition::StartReconciliation)
        .unwrap();
    assert!(matches!(next, BrokerState::Reconciling { .. }));
}

#[test]
fn test_is_connected() {
    // Only Live state should report as connected
    assert!(!BrokerState::Disconnected.is_connected());
    assert!(!BrokerState::Connecting.is_connected());
    assert!(!BrokerState::Reconciling {
        start_time: Instant::now()
    }
    .is_connected());
    assert!(BrokerState::Live {
        connected_since: Instant::now()
    }
    .is_connected());
    assert!(!BrokerState::ErrorRecovery {
        attempt: 1,
        next_retry: Instant::now(),
        config: broker_sync::BackoffConfig::default(),
    }
    .is_connected());
}

#[test]
fn test_connection_duration() {
    let now = Instant::now();
    let state = BrokerState::Live {
        connected_since: now - Duration::from_secs(30),
    };

    let duration = state.connection_duration();
    assert!(duration.is_some());
    assert!(duration.unwrap() >= Duration::from_secs(30));

    // Other states should return None
    assert!(BrokerState::Disconnected.connection_duration().is_none());
}

#[test]
fn test_backoff_duration() {
    // Test with default config (60 second cap)
    let config = broker_sync::BackoffConfig::default();

    let state = BrokerState::ErrorRecovery {
        attempt: 1,
        next_retry: Instant::now(),
        config: config.clone(),
    };
    // With jitter, we can't test exact values, but we can test ranges
    let backoff = state.backoff_duration();
    assert!(backoff >= Duration::from_millis(800)); // 1s - 20%
    assert!(backoff <= Duration::from_millis(1200)); // 1s + 20%

    let state = BrokerState::ErrorRecovery {
        attempt: 2,
        next_retry: Instant::now(),
        config: config.clone(),
    };
    let backoff = state.backoff_duration();
    assert!(backoff >= Duration::from_millis(1600)); // 2s - 20%
    assert!(backoff <= Duration::from_millis(2400)); // 2s + 20%

    // Test cap at 60 seconds
    let state = BrokerState::ErrorRecovery {
        attempt: 10,
        next_retry: Instant::now(),
        config: config.clone(),
    };
    let backoff = state.backoff_duration();
    assert!(backoff <= Duration::from_secs(60)); // Should not exceed max
}

#[test]
fn test_error_transition_from_disconnected() {
    // This test verifies the proptest regression case
    let state = BrokerState::Disconnected;
    let result = state.transition(StateTransition::Error);
    assert!(result.is_ok());

    if let Ok(BrokerState::ErrorRecovery { attempt, .. }) = result {
        assert_eq!(attempt, 1);
    } else {
        panic!("Expected ErrorRecovery state");
    }
}

#[test]
fn test_custom_backoff_config() {
    // Test with custom config
    let config = broker_sync::BackoffConfig {
        base_delay_ms: 500,   // 500ms base
        max_delay_ms: 30_000, // 30s max
        max_exponent: 5,      // 2^5 = 32x base
        jitter_percent: 10,   // +/- 10%
    };

    let state = BrokerState::ErrorRecovery {
        attempt: 1,
        next_retry: Instant::now(),
        config: config.clone(),
    };

    let backoff = state.backoff_duration();
    // 500ms base with 10% jitter
    assert!(backoff >= Duration::from_millis(450));
    assert!(backoff <= Duration::from_millis(550));

    // Test that config is preserved through transitions
    let next = state.transition(StateTransition::RetryConnection).unwrap();
    if let BrokerState::ErrorRecovery {
        config: next_config,
        ..
    } = next
    {
        assert_eq!(next_config, config);
    }
}
