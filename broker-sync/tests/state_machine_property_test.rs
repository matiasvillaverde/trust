//! Property-based tests for BrokerState state machine
//! These tests verify state machine invariants

use broker_sync::{BrokerState, StateTransition};
use proptest::prelude::*;
use std::time::{Duration, Instant};

// Generate arbitrary state transitions
prop_compose! {
    fn arb_state_transition()(choice in 0..7) -> StateTransition {
        match choice {
            0 => StateTransition::Connect,
            1 => StateTransition::ConnectionEstablished,
            2 => StateTransition::ReconciliationComplete,
            3 => StateTransition::Error,
            4 => StateTransition::RetryConnection,
            5 => StateTransition::StartReconciliation,
            _ => StateTransition::Disconnect,
        }
    }
}

// Generate arbitrary initial states
prop_compose! {
    fn arb_broker_state()(choice in 0..5) -> BrokerState {
        match choice {
            0 => BrokerState::Disconnected,
            1 => BrokerState::Connecting,
            2 => BrokerState::Reconciling {
                start_time: Instant::now(),
            },
            3 => BrokerState::Live {
                connected_since: Instant::now(),
            },
            _ => BrokerState::ErrorRecovery {
                attempt: 1,
                next_retry: Instant::now() + Duration::from_secs(5),
                config: broker_sync::BackoffConfig {
                    base_delay_ms: 1000,
                    max_delay_ms: 60_000,
                    max_exponent: 6,
                    jitter_percent: 0, // No jitter for deterministic tests
                },
            },
        }
    }
}

proptest! {
    #[test]
    fn prop_state_transitions_are_deterministic(
        initial_state in arb_broker_state(),
        transition in arb_state_transition()
    ) {
        // Test that transition logic is consistent (same state + transition → same result type)
        // Note: Due to jitter, exact timing may vary, but the result structure should be identical
        let fixed_time = Instant::now();

        let result1 = initial_state.clone().transition_at(transition.clone(), fixed_time);
        let result2 = initial_state.clone().transition_at(transition.clone(), fixed_time);

        // For results with jitter, we can't test exact equality, but we can test structure
        match (&result1, &result2) {
            (Ok(state1), Ok(state2)) => {
                // Both should be the same state type
                prop_assert_eq!(std::mem::discriminant(state1), std::mem::discriminant(state2));

                // For ErrorRecovery states, verify attempt numbers match
                if let (BrokerState::ErrorRecovery { attempt: a1, .. },
                       BrokerState::ErrorRecovery { attempt: a2, .. }) = (state1, state2) {
                    prop_assert_eq!(a1, a2);
                }
            }
            (Err(e1), Err(e2)) => {
                // Both should be the same error type
                prop_assert_eq!(std::mem::discriminant(e1), std::mem::discriminant(e2));
            }
            _ => {
                // Both should be the same result type (Ok or Err)
                prop_assert_eq!(result1.is_ok(), result2.is_ok());
            }
        }
    }

    #[test]
    fn arbitrary_sequences_cannot_skip_connection_or_reconciliation(
        initial_state in arb_broker_state(),
        transitions in prop::collection::vec(arb_state_transition(), 0..100)
    ) {
        let now = Instant::now();
        let mut state = initial_state;
        for transition in transitions {
            // Public lifecycle contract: these are the only routes to usable connectivity.
            let expected = match (&state, &transition) {
                (_, StateTransition::Error) => Some("recovery"),
                (BrokerState::Disconnected | BrokerState::ErrorRecovery { .. }, StateTransition::Connect) => Some("connecting"),
                (BrokerState::Connecting, StateTransition::ConnectionEstablished) => Some("reconciling"),
                (BrokerState::Reconciling { .. }, StateTransition::ReconciliationComplete) => Some("live"),
                (BrokerState::Live { .. }, StateTransition::StartReconciliation) => Some("reconciling"),
                (BrokerState::ErrorRecovery { .. }, StateTransition::RetryConnection) => Some("recovery"),
                _ => None,
            };
            let actual = state.clone().transition_at(transition, now);
            match actual {
                Ok(next) => {
                    let kind = match &next {
                        BrokerState::Disconnected => "disconnected",
                        BrokerState::Connecting => "connecting",
                        BrokerState::Reconciling { .. } => "reconciling",
                        BrokerState::Live { .. } => "live",
                        BrokerState::ErrorRecovery { .. } => "recovery",
                    };
                    prop_assert_eq!(expected, Some(kind));
                    prop_assert_eq!(next.is_connected(), kind == "live");
                    state = next;
                }
                Err(_) => prop_assert!(expected.is_none(), "A permitted recovery step must succeed"),
            }
        }
    }

    #[test]
    fn prop_error_recovery_attempt_increases(
        attempt_count in 1u32..100
    ) {
        // Error recovery attempts should increase monotonically
        let state = BrokerState::ErrorRecovery {
            attempt: attempt_count,
            next_retry: Instant::now(),
            config: broker_sync::BackoffConfig {
                base_delay_ms: 1000,
                max_delay_ms: 60_000,
                max_exponent: 6,
                jitter_percent: 0, // No jitter for deterministic tests
            },
        };

        let recovered = state.transition(StateTransition::RetryConnection)?;
        prop_assert!(matches!(recovered, BrokerState::ErrorRecovery { attempt, .. } if attempt > attempt_count), "Retry must increase the attempt");
    }

    #[test]
    fn prop_only_live_state_is_connected(
        state in arb_broker_state()
    ) {
        // Only Live state should report as connected
        let is_connected = state.is_connected();
        let is_live = matches!(state, BrokerState::Live { .. });
        prop_assert_eq!(is_connected, is_live);
    }

    #[test]
    fn prop_connection_duration_only_for_live(
        state in arb_broker_state()
    ) {
        // Only Live state should have connection duration
        let has_duration = state.connection_duration().is_some();
        let is_live = matches!(state, BrokerState::Live { .. });
        prop_assert_eq!(has_duration, is_live);
    }

    #[test]
    fn prop_error_state_always_reachable(
        initial_state in arb_broker_state()
    ) {
        // Error state should be reachable from any state
        let error_state = initial_state.transition(StateTransition::Error);
        prop_assert!(error_state.is_ok());

        prop_assert!(matches!(error_state, Ok(BrokerState::ErrorRecovery { .. })), "Errors always enter recovery");
    }

    #[test]
    fn backoff_is_bounded_even_at_integer_boundaries(
        attempt in any::<u32>(),
        base_delay_ms in any::<u64>(),
        max_delay_ms in any::<u64>(),
        max_exponent in any::<u32>(),
        jitter_percent in any::<u32>(),
    ) {
        let state = BrokerState::ErrorRecovery {
            attempt, next_retry: Instant::now(),
            config: broker_sync::BackoffConfig { base_delay_ms, max_delay_ms, max_exponent, jitter_percent },
        };
        prop_assert!(state.backoff_duration() <= Duration::from_millis(max_delay_ms));
    }

    #[test]
    fn successful_reconnection_after_any_valid_prefix_reaches_live(
        transitions in prop::collection::vec(arb_state_transition(), 0..200)
    ) {
        let now = Instant::now();
        let mut state = BrokerState::Disconnected;
        for transition in transitions {
            if let Ok(next) = state.clone().transition_at(transition, now) {
                state = next;
            }
        }
        // A healthy connection must be usable after recovery from any prior history.
        state = state.transition_at(StateTransition::Error, now)?;
        prop_assert!(!state.is_connected());
        state = state.transition_at(StateTransition::Connect, now)?;
        prop_assert!(!state.is_connected());
        state = state.transition_at(StateTransition::ConnectionEstablished, now)?;
        prop_assert!(!state.is_connected(), "Orders must reconcile before becoming live");
        state = state.transition_at(StateTransition::ReconciliationComplete, now)?;
        prop_assert_eq!(state, BrokerState::Live { connected_since: now });
    }
}

#[test]
fn retry_counter_saturates_without_losing_recovery_state() {
    let state = BrokerState::ErrorRecovery {
        attempt: u32::MAX,
        next_retry: Instant::now(),
        config: broker_sync::BackoffConfig::default(),
    };
    assert!(matches!(
        state.transition(StateTransition::RetryConnection).unwrap(),
        BrokerState::ErrorRecovery {
            attempt: u32::MAX,
            ..
        }
    ));
}
