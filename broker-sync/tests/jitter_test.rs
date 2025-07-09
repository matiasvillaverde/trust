//! Comprehensive tests for backoff jitter implementation
//! Ensures proper random distribution and security properties

use broker_sync::{BackoffConfig, BrokerState};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[test]
fn test_jitter_produces_different_values() {
    // Test that jitter actually produces random values
    let config = BackoffConfig {
        base_delay_ms: 1000,
        max_delay_ms: 60_000,
        max_exponent: 6,
        jitter_percent: 20,
    };

    let mut delays = Vec::new();

    // Generate multiple backoff delays
    for _ in 0..100 {
        let state = BrokerState::ErrorRecovery {
            attempt: 1,
            next_retry: Instant::now(),
            config: config.clone(),
        };
        let delay = state.backoff_duration();
        delays.push(delay.as_millis());
    }

    // Check that we get different values (not all the same)
    let unique_delays: std::collections::HashSet<_> = delays.iter().collect();
    assert!(
        unique_delays.len() > 10,
        "Jitter should produce varied delays, got {} unique values out of 100",
        unique_delays.len()
    );

    // All should be within expected range (1000ms ± 20%)
    for delay in &delays {
        assert!(
            *delay >= 800,
            "Delay {delay} is below minimum expected 800ms"
        );
        assert!(
            *delay <= 1200,
            "Delay {delay} is above maximum expected 1200ms"
        );
    }
}

#[test]
fn test_jitter_distribution() {
    // Test that jitter is reasonably distributed across the range
    let config = BackoffConfig {
        base_delay_ms: 1000,
        max_delay_ms: 60_000,
        max_exponent: 6,
        jitter_percent: 20, // ±200ms range
    };

    let mut buckets: HashMap<u32, u32> = HashMap::new();
    let bucket_size = 50; // 50ms buckets

    // Generate many samples
    for _ in 0..1000 {
        let state = BrokerState::ErrorRecovery {
            attempt: 1,
            next_retry: Instant::now(),
            config: config.clone(),
        };
        let delay = state.backoff_duration().as_millis() as u32;
        let bucket = (delay - 800) / bucket_size;
        *buckets.entry(bucket).or_insert(0) += 1;
    }

    // Should have values in multiple buckets (800-850, 850-900, etc.)
    assert!(
        buckets.len() >= 6,
        "Should have good distribution across buckets, got {} buckets",
        buckets.len()
    );

    // No bucket should have more than 25% of values (rough uniformity check)
    for count in buckets.values() {
        assert!(
            *count < 250,
            "Bucket has {count} values, distribution seems skewed"
        );
    }
}

#[test]
fn test_zero_jitter_is_deterministic() {
    // When jitter_percent is 0, delays should be deterministic
    let config = BackoffConfig {
        base_delay_ms: 1000,
        max_delay_ms: 60_000,
        max_exponent: 6,
        jitter_percent: 0, // No jitter
    };

    let mut delays = Vec::new();

    for _ in 0..10 {
        let state = BrokerState::ErrorRecovery {
            attempt: 2,
            next_retry: Instant::now(),
            config: config.clone(),
        };
        delays.push(state.backoff_duration());
    }

    // All delays should be exactly the same (2000ms for attempt 2)
    let first = delays[0];
    for delay in &delays {
        assert_eq!(
            *delay, first,
            "With 0% jitter, all delays should be identical"
        );
    }
    assert_eq!(
        first,
        Duration::from_millis(2000),
        "Attempt 2 with no jitter should be exactly 2000ms"
    );
}

#[test]
fn test_minimum_delay_enforcement() {
    // Test that minimum 100ms is enforced even with large negative jitter
    let config = BackoffConfig {
        base_delay_ms: 200, // Low base
        max_delay_ms: 60_000,
        max_exponent: 6,
        jitter_percent: 80, // ±160ms jitter could go negative
    };

    // Run many times to catch negative jitter cases
    for _ in 0..100 {
        let state = BrokerState::ErrorRecovery {
            attempt: 1,
            next_retry: Instant::now(),
            config: config.clone(),
        };
        let delay = state.backoff_duration();
        assert!(
            delay >= Duration::from_millis(100),
            "Delay should never be less than 100ms minimum"
        );
    }
}

#[test]
fn test_maximum_delay_enforcement() {
    // Test that maximum delay is enforced even with large positive jitter
    let config = BackoffConfig {
        base_delay_ms: 1000,
        max_delay_ms: 5000, // Low max for testing
        max_exponent: 6,
        jitter_percent: 50, // ±50% could exceed max
    };

    // Test high attempt count where base delay would be at max
    for _ in 0..100 {
        let state = BrokerState::ErrorRecovery {
            attempt: 10, // Would be 1000 * 2^6 = 64000ms without cap
            next_retry: Instant::now(),
            config: config.clone(),
        };
        let delay = state.backoff_duration();
        assert!(
            delay <= Duration::from_millis(5000),
            "Delay should never exceed configured maximum"
        );
    }
}

#[test]
fn test_jitter_percentage_accuracy() {
    // Test that jitter percentage produces correct range
    let test_cases = vec![
        (10, 1000, 900, 1100),  // 10% of 1000ms = ±100ms
        (25, 2000, 1500, 2500), // 25% of 2000ms = ±500ms
        (50, 1000, 500, 1500),  // 50% of 1000ms = ±500ms
    ];

    for (jitter_percent, base_ms, min_expected, max_expected) in test_cases {
        let config = BackoffConfig {
            base_delay_ms: base_ms,
            max_delay_ms: 60_000,
            max_exponent: 6,
            jitter_percent,
        };

        let mut min_seen = u64::MAX;
        let mut max_seen = 0u64;

        // Sample many times to find range
        for _ in 0..500 {
            let state = BrokerState::ErrorRecovery {
                attempt: 1,
                next_retry: Instant::now(),
                config: config.clone(),
            };
            let delay_ms = state.backoff_duration().as_millis() as u64;
            min_seen = min_seen.min(delay_ms);
            max_seen = max_seen.max(delay_ms);
        }

        // Allow some tolerance for randomness (might not hit exact boundaries)
        assert!(
            min_seen <= min_expected + 50,
            "{jitter_percent}% jitter: minimum {min_seen} should be close to {min_expected}"
        );
        assert!(
            max_seen >= max_expected - 50,
            "{jitter_percent}% jitter: maximum {max_seen} should be close to {max_expected}"
        );
    }
}

#[test]
fn test_concurrent_jitter_different_values() {
    // Test that concurrent calls produce different jitter values
    use std::sync::{Arc, Mutex};
    use std::thread;

    let delays = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    // Spawn multiple threads to generate delays concurrently
    for _ in 0..10 {
        let delays_clone = Arc::clone(&delays);
        let handle = thread::spawn(move || {
            let config = BackoffConfig {
                base_delay_ms: 1000,
                max_delay_ms: 60_000,
                max_exponent: 6,
                jitter_percent: 30,
            };

            let state = BrokerState::ErrorRecovery {
                attempt: 1,
                next_retry: Instant::now(),
                config,
            };

            let delay = state.backoff_duration().as_millis();
            delays_clone.lock().unwrap().push(delay);
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Check that we got different values
    let delays = delays.lock().unwrap();
    let unique_delays: std::collections::HashSet<_> = delays.iter().collect();
    assert!(
        unique_delays.len() > 5,
        "Concurrent calls should produce different jitter values, got {} unique out of 10",
        unique_delays.len()
    );
}
