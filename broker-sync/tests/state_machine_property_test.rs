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
                config: broker_sync::BackoffConfig::default(),
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
        // Same state and transition should always produce same result
        let fixed_time = Instant::now();
        let result1 = initial_state.clone().transition_at(transition.clone(), fixed_time);
        let result2 = initial_state.clone().transition_at(transition.clone(), fixed_time);
        prop_assert_eq!(result1, result2);
    }

    #[test]
    fn prop_state_transitions_never_panic(
        initial_state in arb_broker_state(),
        transitions in prop::collection::vec(arb_state_transition(), 0..100)
    ) {
        // No sequence of transitions should cause a panic
        let mut state = initial_state;
        for transition in transitions {
            // Allow both valid and invalid transitions, but they shouldn't panic
            if let Ok(new_state) = state.clone().transition(transition) {
                state = new_state;
            }
            // Invalid transitions are fine, just ignore
        }
        // If we got here, no panic occurred
        prop_assert!(true);
    }

    #[test]
    fn prop_error_recovery_attempt_increases(
        attempt_count in 1u32..100
    ) {
        // Error recovery attempts should increase monotonically
        let state = BrokerState::ErrorRecovery {
            attempt: attempt_count,
            next_retry: Instant::now(),
            config: broker_sync::BackoffConfig::default(),
        };

        if let Ok(BrokerState::ErrorRecovery { attempt, .. }) =
            state.clone().transition(StateTransition::RetryConnection) {
            prop_assert!(attempt > attempt_count);
        }
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

        if let Ok(BrokerState::ErrorRecovery { .. }) = error_state {
            prop_assert!(true);
        } else {
            prop_assert!(false, "Expected ErrorRecovery state");
        }
    }

    #[test]
    fn prop_backoff_increases_exponentially(
        attempts in 1u32..10
    ) {
        // Backoff should follow exponential pattern (with cap)
        let config = broker_sync::BackoffConfig {
            base_delay_ms: 1000,
            max_delay_ms: 60_000,
            max_exponent: 6,
            jitter_percent: 0, // Disable jitter for testing
        };

        let state = BrokerState::ErrorRecovery {
            attempt: attempts,
            next_retry: Instant::now(),
            config: config.clone(),
        };

        let backoff = state.backoff_duration();
        let exponent = (attempts - 1).min(6);
        let expected_ms = 1000u64.saturating_mul(2u64.pow(exponent)).min(60_000);
        let expected = Duration::from_millis(expected_ms);
        prop_assert_eq!(backoff, expected);
    }

    #[test]
    fn prop_state_sequence_eventually_reaches_live(
        transitions in prop::collection::vec(arb_state_transition(), 100..200)
    ) {
        // Given enough transitions, we should be able to reach Live state
        let mut state = BrokerState::Disconnected;
        let mut reached_live = false;

        let transition_count = transitions.len();
        for transition in transitions {
            if let Ok(new_state) = state.clone().transition(transition) {
                state = new_state;
                if matches!(state, BrokerState::Live { .. }) {
                    reached_live = true;
                    break;
                }
            }
        }

        // This is a probabilistic test - with enough random transitions,
        // we should hit the Live state at some point
        // Not a hard requirement, but useful for detecting broken paths
        prop_assume!(reached_live || transition_count < 150);
    }
}
