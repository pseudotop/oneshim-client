use std::time::Duration;

use rand::RngExt;
use reqwest::header::RETRY_AFTER;
use tokio::time::Instant;

use crate::error::NetworkError;

const DEFAULT_RETRY_AFTER_SECS: u64 = 60;
const MAX_BACKOFF_EXPONENT: u32 = 10;

pub fn extract_retry_after(response: &reqwest::Response) -> u64 {
    response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_RETRY_AFTER_SECS)
}

pub fn scale_duration(duration: Duration, factor: u32) -> Duration {
    let scaled_ms = duration
        .as_millis()
        .saturating_mul(factor as u128)
        .min(u64::MAX as u128) as u64;
    Duration::from_millis(scaled_ms)
}

pub fn jittered_backoff_delay(attempt: u32, base: Duration, max: Duration) -> Duration {
    let base_ms = base.as_millis().min(u64::MAX as u128) as u64;
    let max_ms = max.as_millis().min(u64::MAX as u128) as u64;
    if base_ms == 0 || max_ms == 0 {
        return Duration::from_millis(0);
    }

    let exp_ms = base_ms.saturating_mul(2u64.saturating_pow(attempt.min(MAX_BACKOFF_EXPONENT)));
    let jitter_max_ms = exp_ms / 4;
    let jitter_ms = if jitter_max_ms == 0 {
        0
    } else {
        let mut rng = rand::rng();
        rng.random_range(0..=jitter_max_ms)
    };

    Duration::from_millis(exp_ms.saturating_add(jitter_ms).min(max_ms))
}

#[derive(Debug, Clone)]
pub struct RetryBackoffPolicy {
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl RetryBackoffPolicy {
    pub fn new(base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            base_delay,
            max_delay,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetryBackoffGate {
    policy: RetryBackoffPolicy,
    consecutive_failures: u32,
    blocked_until: Option<Instant>,
}

impl RetryBackoffGate {
    pub fn new(policy: RetryBackoffPolicy) -> Self {
        Self {
            policy,
            consecutive_failures: 0,
            blocked_until: None,
        }
    }

    pub fn is_ready(&self, now: Instant) -> bool {
        self.blocked_until.map_or(true, |until| now >= until)
    }

    pub fn on_success(&mut self) {
        self.consecutive_failures = 0;
        self.blocked_until = None;
    }

    pub fn on_failure(&mut self, now: Instant, error: &NetworkError) -> Duration {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let delay = match error {
            NetworkError::RateLimited { retry_after_secs } => {
                Duration::from_secs(*retry_after_secs)
            }
            _ => jittered_backoff_delay(
                self.consecutive_failures.saturating_sub(1),
                self.policy.base_delay,
                self.policy.max_delay,
            ),
        };
        self.blocked_until = Some(now + delay);
        delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jittered_backoff_is_bounded() {
        let delay = jittered_backoff_delay(3, Duration::from_secs(2), Duration::from_secs(30));
        assert!(delay >= Duration::from_secs(16));
        assert!(delay <= Duration::from_secs(20));
    }

    #[test]
    fn scaled_duration_scales_millis() {
        let delay = scale_duration(Duration::from_millis(250), 8);
        assert_eq!(delay, Duration::from_secs(2));
    }

    #[tokio::test]
    async fn retry_gate_blocks_until_backoff_expires() {
        let mut gate = RetryBackoffGate::new(RetryBackoffPolicy::new(
            Duration::from_millis(10),
            Duration::from_millis(100),
        ));
        let now = Instant::now();
        let delay = gate.on_failure(now, &NetworkError::ServiceUnavailable("down".to_string()));
        assert!(delay >= Duration::from_millis(10));
        assert!(!gate.is_ready(now));
        assert!(gate.is_ready(now + delay));

        gate.on_success();
        assert!(gate.is_ready(Instant::now()));
    }
}
