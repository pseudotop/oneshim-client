//! D13-v2b PR-B3 — accumulates dropped event counts (rate limiter rejections,
//! channel lag) and surfaces them as `DroppedEventsSignal` on a throttled
//! cadence. Per-stream state; reset after each emission.
//!
//! Invariants:
//! - Emission interval: 1 second (configurable via const).
//! - Counts are per event_type String (Frame/Idle/AiRuntimeStatus).
//! - `since` timestamp rolls forward on each emission.
//! - Reason codes: "rate_limit" | "channel_lag" (spec proto §DroppedEventsSignal).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use crate::proto::dashboard::v1::{dropped_events_signal::TypeCount, DroppedEventsSignal};

/// Emit a DroppedEventsSignal at most this often, even if drops
/// keep accumulating.
pub const DROP_EMIT_INTERVAL: Duration = Duration::from_secs(1);

pub struct DropAccumulator {
    counts_by_type: HashMap<String, u64>,
    since: DateTime<Utc>,
    last_emit_at: Option<Instant>,
    reason: &'static str, // "rate_limit" for MVP; multi-reason in v2c
}

impl DropAccumulator {
    pub fn new() -> Self {
        Self {
            counts_by_type: HashMap::new(),
            since: Utc::now(),
            last_emit_at: None,
            reason: "rate_limit",
        }
    }

    pub fn record_drop(&mut self, event_type: &str) {
        *self
            .counts_by_type
            .entry(event_type.to_string())
            .or_insert(0) += 1;
    }

    /// Emit a DroppedEventsSignal iff (a) there are accumulated drops AND
    /// (b) the throttle interval has elapsed. Caller wraps the returned
    /// signal into SubscribeEventsResponse::Payload::Dropped.
    pub fn maybe_emit(&mut self) -> Option<DroppedEventsSignal> {
        if self.counts_by_type.is_empty() {
            return None;
        }
        let now = Instant::now();
        let throttle_ok = match self.last_emit_at {
            None => true,
            Some(t) => now.duration_since(t) >= DROP_EMIT_INTERVAL,
        };
        if !throttle_ok {
            return None;
        }

        let dropped_count: u64 = self.counts_by_type.values().sum();
        let by_type: Vec<TypeCount> = self
            .counts_by_type
            .iter()
            .map(|(k, v)| TypeCount {
                event_type: k.clone(),
                count: *v,
            })
            .collect();

        let until = Utc::now();
        let signal = DroppedEventsSignal {
            dropped_count,
            since: Some(super::to_proto_ts(self.since)),
            until: Some(super::to_proto_ts(until)),
            reason: self.reason.to_string(),
            by_type,
        };

        // Reset state post-emission.
        self.counts_by_type.clear();
        self.since = until;
        self.last_emit_at = Some(now);

        Some(signal)
    }
}

impl Default for DropAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl DropAccumulator {
    /// Test-only: set last_emit_at to a past Instant to make the throttle
    /// interval test deterministic without sleeping.
    pub(super) fn set_last_emit_at_for_test(&mut self, t: Instant) {
        self.last_emit_at = Some(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_accumulator_maybe_emit_returns_none() {
        let mut acc = DropAccumulator::new();
        assert!(acc.maybe_emit().is_none(), "no drops recorded → None");
    }

    #[test]
    fn single_drop_emits_on_first_call() {
        let mut acc = DropAccumulator::new();
        acc.record_drop("frame");
        let sig = acc.maybe_emit().expect("first emission should fire");
        assert_eq!(sig.dropped_count, 1);
        assert_eq!(sig.reason, "rate_limit");
        assert_eq!(sig.by_type.len(), 1);
        assert_eq!(sig.by_type[0].event_type, "frame");
        assert_eq!(sig.by_type[0].count, 1);
        // State reset
        assert!(acc.counts_by_type.is_empty());
    }

    #[test]
    fn throttle_blocks_rapid_emit() {
        let mut acc = DropAccumulator::new();
        acc.record_drop("frame");
        assert!(acc.maybe_emit().is_some(), "first emit");
        acc.record_drop("frame");
        assert!(
            acc.maybe_emit().is_none(),
            "second emit within throttle window should be None"
        );
    }

    #[test]
    fn throttle_permits_emit_after_interval() {
        let mut acc = DropAccumulator::new();
        acc.record_drop("frame");
        let first = acc.maybe_emit().expect("first emission");
        let first_until_micros = first.until.as_ref().map(|t| (t.seconds, t.nanos));

        // Fake that the throttle interval fully elapsed by pushing last_emit_at backward.
        let past = Instant::now() - (DROP_EMIT_INTERVAL + Duration::from_millis(50));
        acc.set_last_emit_at_for_test(past);
        acc.record_drop("idle");

        let second = acc.maybe_emit().expect("second emission after interval");
        let second_since_micros = second.since.as_ref().map(|t| (t.seconds, t.nanos));

        // since of second emission should equal until of first emission (rollover).
        assert_eq!(first_until_micros, second_since_micros);
        assert_eq!(second.dropped_count, 1);
        assert_eq!(second.by_type[0].event_type, "idle");
    }
}
