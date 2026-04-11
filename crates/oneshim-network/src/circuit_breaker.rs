use parking_lot::Mutex;
use std::time::{Duration, Instant};
use tracing::warn;

// ── Config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub initial_cooldown: Duration,
    pub max_cooldown: Duration,
    pub half_open_probes: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            initial_cooldown: Duration::from_secs(30),
            max_cooldown: Duration::from_secs(300),
            half_open_probes: 1,
        }
    }
}

// ── State ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CircuitState {
    Closed,
    Open { until: Instant },
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: &'static str,
    pub consecutive_failures: u32,
    pub current_cooldown: Duration,
}

// ── Inner (behind Mutex) ────────────────────────────────────────────

struct InnerState {
    status: CircuitState,
    consecutive_failures: u32,
    current_cooldown: Duration,
}

// ── CircuitBreaker ──────────────────────────────────────────────────

pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Mutex<InnerState>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        let cooldown = config.initial_cooldown;
        Self {
            config,
            state: Mutex::new(InnerState {
                status: CircuitState::Closed,
                consecutive_failures: 0,
                current_cooldown: cooldown,
            }),
        }
    }

    /// Check current state, transitioning Open→HalfOpen if cooldown elapsed.
    pub fn check(&self) -> CircuitState {
        let mut inner = self.state.lock();
        if let CircuitState::Open { until } = &inner.status {
            if Instant::now() >= *until {
                inner.status = CircuitState::HalfOpen;
                warn!("circuit breaker: Open → HalfOpen (cooldown elapsed)");
            }
        }
        inner.status.clone()
    }

    pub fn record_success(&self) {
        let mut inner = self.state.lock();
        let was_half_open = matches!(inner.status, CircuitState::HalfOpen);
        inner.consecutive_failures = 0;
        inner.current_cooldown = self.config.initial_cooldown;
        inner.status = CircuitState::Closed;
        if was_half_open {
            warn!("circuit breaker: HalfOpen → Closed (probe success)");
        }
    }

    pub fn record_failure(&self) {
        let mut inner = self.state.lock();
        inner.consecutive_failures += 1;

        match &inner.status {
            CircuitState::Closed => {
                if inner.consecutive_failures >= self.config.failure_threshold {
                    let until = Instant::now() + inner.current_cooldown;
                    warn!(
                        failures = inner.consecutive_failures,
                        cooldown_secs = inner.current_cooldown.as_secs(),
                        "circuit breaker: Closed → Open"
                    );
                    inner.status = CircuitState::Open { until };
                }
            }
            CircuitState::HalfOpen => {
                let new_cooldown = (inner.current_cooldown * 2).min(self.config.max_cooldown);
                inner.current_cooldown = new_cooldown;
                let until = Instant::now() + new_cooldown;
                warn!(
                    cooldown_secs = new_cooldown.as_secs(),
                    "circuit breaker: HalfOpen → Open (probe failed, cooldown doubled)"
                );
                inner.status = CircuitState::Open { until };
            }
            CircuitState::Open { .. } => {
                // Already open — just count the failure
            }
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state.lock().status.clone()
    }

    pub fn stats(&self) -> CircuitBreakerStats {
        let inner = self.state.lock();
        CircuitBreakerStats {
            state: match &inner.status {
                CircuitState::Closed => "closed",
                CircuitState::Open { .. } => "open",
                CircuitState::HalfOpen => "half_open",
            },
            consecutive_failures: inner.consecutive_failures,
            current_cooldown: inner.current_cooldown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            initial_cooldown: Duration::from_millis(50),
            max_cooldown: Duration::from_millis(200),
            half_open_probes: 1,
        }
    }

    #[test]
    fn starts_closed() {
        let cb = CircuitBreaker::new(fast_config());
        assert!(matches!(cb.check(), CircuitState::Closed));
        assert_eq!(cb.stats().state, "closed");
    }

    #[test]
    fn trips_open_after_threshold() {
        let cb = CircuitBreaker::new(fast_config());
        cb.record_failure();
        cb.record_failure();
        assert!(matches!(cb.check(), CircuitState::Closed));
        cb.record_failure(); // 3rd failure = threshold
        assert!(matches!(cb.check(), CircuitState::Open { .. }));
        assert_eq!(cb.stats().state, "open");
        assert_eq!(cb.stats().consecutive_failures, 3);
    }

    #[test]
    fn open_to_half_open_after_cooldown() {
        let cb = CircuitBreaker::new(fast_config());
        for _ in 0..3 {
            cb.record_failure();
        }
        assert!(matches!(cb.check(), CircuitState::Open { .. }));
        std::thread::sleep(Duration::from_millis(60));
        assert!(matches!(cb.check(), CircuitState::HalfOpen));
        assert_eq!(cb.stats().state, "half_open");
    }

    #[test]
    fn half_open_to_closed_on_success() {
        let cb = CircuitBreaker::new(fast_config());
        for _ in 0..3 {
            cb.record_failure();
        }
        std::thread::sleep(Duration::from_millis(60));
        let _ = cb.check(); // transition to HalfOpen
        cb.record_success();
        assert!(matches!(cb.check(), CircuitState::Closed));
        assert_eq!(cb.stats().consecutive_failures, 0);
    }

    #[test]
    fn half_open_to_open_on_failure_doubles_cooldown() {
        let cb = CircuitBreaker::new(fast_config());
        for _ in 0..3 {
            cb.record_failure();
        }
        // initial cooldown = 50ms
        std::thread::sleep(Duration::from_millis(60));
        let _ = cb.check(); // HalfOpen
        cb.record_failure(); // probe failure → Open with doubled cooldown
        assert!(matches!(cb.check(), CircuitState::Open { .. }));
        // cooldown should now be 100ms
        assert_eq!(cb.stats().current_cooldown, Duration::from_millis(100));
    }

    #[test]
    fn cooldown_caps_at_max() {
        let cb = CircuitBreaker::new(fast_config());
        // Trip → HalfOpen → fail (50→100) → HalfOpen → fail (100→200) → HalfOpen → fail (200→200 capped)
        for round in 0..3 {
            for _ in 0..3 {
                cb.record_failure();
            }
            let expected_cooldown = match round {
                0 => Duration::from_millis(50),
                1 => Duration::from_millis(100),
                _ => Duration::from_millis(200),
            };
            // Wait for cooldown to elapse
            std::thread::sleep(expected_cooldown + Duration::from_millis(10));
            let _ = cb.check(); // HalfOpen
            cb.record_failure(); // probe fail → Open with doubled (or capped) cooldown
        }
        // After 3 rounds of doubling: 50→100→200→200 (capped at max_cooldown=200)
        assert_eq!(cb.stats().current_cooldown, Duration::from_millis(200));
    }

    #[test]
    fn threshold_one_trips_immediately() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..fast_config()
        };
        let cb = CircuitBreaker::new(config);
        cb.record_failure();
        assert!(matches!(cb.check(), CircuitState::Open { .. }));
    }

    #[test]
    fn success_resets_failure_count() {
        let cb = CircuitBreaker::new(fast_config());
        cb.record_failure();
        cb.record_failure();
        cb.record_success(); // reset
        cb.record_failure();
        // Only 1 failure after reset, threshold is 3
        assert!(matches!(cb.check(), CircuitState::Closed));
    }

    #[test]
    fn concurrent_failures_transition_to_open() {
        let cb = std::sync::Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            ..Default::default()
        }));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let cb = std::sync::Arc::clone(&cb);
                std::thread::spawn(move || {
                    cb.record_failure();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
        assert!(matches!(cb.check(), CircuitState::Open { .. }));
    }

    #[test]
    fn concurrent_checks_dont_panic() {
        let cb = std::sync::Arc::new(CircuitBreaker::new(Default::default()));
        let handles: Vec<_> = (0..100)
            .map(|_| {
                let cb = std::sync::Arc::clone(&cb);
                std::thread::spawn(move || {
                    let _ = cb.check();
                    cb.record_failure();
                    let _ = cb.stats();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
    }
}
