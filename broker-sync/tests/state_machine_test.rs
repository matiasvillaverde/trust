//! Tests for BrokerState state machine
//! Following TDD - tests written first

use broker_sync::{BrokerState, StateTransition};
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
    let next = state.transition(StateTransition::Connect);
    assert!(matches!(next, BrokerState::Connecting));
}

#[test]
fn test_connecting_can_transition_to_reconciling() {
    let state = BrokerState::Connecting;
    let next = state.transition(StateTransition::ConnectionEstablished);
    assert!(matches!(next, BrokerState::Reconciling { .. }));
}

#[test]
fn test_reconciling_can_transition_to_live() {
    let state = BrokerState::Reconciling {
        start_time: Instant::now(),
    };
    let next = state.transition(StateTransition::ReconciliationComplete);
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
        let next = state.clone().transition(StateTransition::Error);
        assert!(matches!(next, BrokerState::ErrorRecovery { .. }));
    }
}

#[test]
fn test_error_recovery_increments_attempt_count() {
    let state = BrokerState::ErrorRecovery {
        attempt: 1,
        next_retry: Instant::now(),
    };
    let next = state.transition(StateTransition::RetryConnection);

    if let BrokerState::ErrorRecovery { attempt, .. } = next {
        assert_eq!(attempt, 2);
    } else {
        panic!("Expected ErrorRecovery state");
    }
}

#[test]
fn test_error_recovery_can_transition_to_connecting() {
    let state = BrokerState::ErrorRecovery {
        attempt: 1,
        next_retry: Instant::now(),
    };
    let next = state.transition(StateTransition::Connect);
    assert!(matches!(next, BrokerState::Connecting));
}

#[test]
fn test_live_can_transition_to_reconciling() {
    let state = BrokerState::Live {
        connected_since: Instant::now(),
    };
    let next = state.transition(StateTransition::StartReconciliation);
    assert!(matches!(next, BrokerState::Reconciling { .. }));
}

#[test]
fn test_invalid_transitions_return_same_state() {
    // Test that invalid transitions don't change state
    let state = BrokerState::Disconnected;
    let next = state
        .clone()
        .transition(StateTransition::ReconciliationComplete);
    assert_eq!(state, next);
}

#[test]
fn test_state_has_is_connected_method() {
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
        next_retry: Instant::now()
    }
    .is_connected());
}

#[test]
fn test_state_has_connection_duration() {
    let now = Instant::now();
    let state = BrokerState::Live {
        connected_since: now - Duration::from_secs(60),
    };

    let duration = state.connection_duration();
    assert!(duration.is_some());
    assert!(duration.unwrap() >= Duration::from_secs(60));
}

#[test]
fn test_error_recovery_backoff_calculation() {
    let state = BrokerState::ErrorRecovery {
        attempt: 1,
        next_retry: Instant::now(),
    };

    let backoff = state.backoff_duration();
    assert_eq!(backoff, Duration::from_secs(1)); // 2^0 * base

    let state = BrokerState::ErrorRecovery {
        attempt: 2,
        next_retry: Instant::now(),
    };

    let backoff = state.backoff_duration();
    assert_eq!(backoff, Duration::from_secs(2)); // 2^1 * base

    let state = BrokerState::ErrorRecovery {
        attempt: 5,
        next_retry: Instant::now(),
    };

    let backoff = state.backoff_duration();
    assert_eq!(backoff, Duration::from_secs(16)); // 2^4 * base, capped
}
