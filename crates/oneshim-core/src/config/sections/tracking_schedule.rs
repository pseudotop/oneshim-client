// Tracking schedule configuration — privacy-hardening feature (Phase 9 PR-A).
//
// Allows users to configure wall-clock windows during which telemetry/capture
// is muted. A.3 will provide the real Default + window_is_active implementations;
// the types here are stubbed with `todo!()` so that A.2's 12 contract tests
// compile cleanly and reach runtime-red via panic (TDD red state).
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::config::enums::Weekday;

// ── TrackingScheduleConfig ──────────────────────────────────────────

/// Top-level config section controlling tracking schedule muting.
///
/// When `enabled` is true and `windows` is non-empty, telemetry/capture is
/// suppressed outside (or during, depending on configuration) the configured
/// windows. `timezone` is an IANA timezone name or the special value `"Local"`
/// meaning the system local timezone.
///
/// Default: disabled, no windows, timezone `"Local"`.
// A.3 adds `pub use tracking_schedule::*;` in mod.rs which resolves dead_code.
// Until then, suppress to keep clippy -D warnings clean.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackingScheduleConfig {
    /// Master switch; false = schedule is ignored and tracking always runs.
    #[serde(default)]
    pub enabled: bool,
    /// Wall-clock windows during which tracking is allowed (or muted, per
    /// interpretation in A.5). Empty vec means no windows configured.
    #[serde(default)]
    pub windows: Vec<TrackingWindow>,
    /// IANA timezone name used for window matching, or `"Local"` for the
    /// system timezone. Default: `"Local"`.
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

/// A single wall-clock window within which tracking behaviour is altered.
///
/// `start` and `end` are `"HH:MM"` strings (24-hour). If `end < start` the
/// window wraps overnight (e.g. `22:00`–`06:00`). `days_of_week` lists the
/// days the window is active; an empty vec means the window never fires.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackingWindow {
    /// Window open time, `"HH:MM"` (24-hour). Must be a valid HH:MM string.
    pub start: String,
    /// Window close time, `"HH:MM"` (24-hour). Must be a valid HH:MM string.
    pub end: String,
    /// Days of week on which this window is active. Empty = never active.
    #[serde(default)]
    pub days_of_week: Vec<Weekday>,
    /// Optional human-readable label for display purposes.
    #[serde(default)]
    pub label: String,
}

#[allow(dead_code)]
fn default_timezone() -> String {
    "Local".to_string()
}

impl Default for TrackingScheduleConfig {
    fn default() -> Self {
        todo!("A.3 impl")
    }
}

#[allow(dead_code)]
impl TrackingWindow {
    /// Return `true` if `now` falls within this window.
    ///
    /// Overnight windows (`end < start`) wrap across midnight and match times
    /// in `[start, 24:00)` on the configured day OR `[00:00, end)` on the
    /// following day. Empty `days_of_week` always returns `false`.
    pub fn window_is_active(&self, _now: DateTime<Local>) -> bool {
        todo!("A.3 impl")
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ────────────────────────────────────────────────────────────

    /// Build a TrackingWindow without a label.
    fn window(start: &str, end: &str, days: Vec<Weekday>) -> TrackingWindow {
        TrackingWindow {
            start: start.to_string(),
            end: end.to_string(),
            days_of_week: days,
            label: String::new(),
        }
    }

    /// Convert a `chrono_tz` `DateTime` to a `DateTime<Local>`.
    fn to_local<Tz: chrono::TimeZone>(dt: chrono::DateTime<Tz>) -> DateTime<Local> {
        dt.with_timezone(&Local)
    }

    // ── 1. Default ─────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "A.3 impl")]
    fn default_is_disabled_with_empty_windows() {
        // A.3 impl will return TrackingScheduleConfig { enabled: false, windows: vec![], timezone: "Local" }.
        // Until then: todo!() panics → red state.
        let cfg = TrackingScheduleConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.windows.is_empty());
        assert_eq!(cfg.timezone, "Local");
    }

    // ── 2. Serde roundtrip ────────────────────────────────────────────────

    #[test]
    fn serde_roundtrip() {
        // This test exercises only Serialize + Deserialize, NOT Default or
        // window_is_active, so it must be GREEN already (derive-generated impls
        // are unconditional). A.3 may narrow serde validation but must not
        // break this roundtrip.
        let original = TrackingScheduleConfig {
            enabled: true,
            windows: vec![TrackingWindow {
                start: "09:00".to_string(),
                end: "17:00".to_string(),
                days_of_week: vec![Weekday::Mon, Weekday::Fri],
                label: "Work hours".to_string(),
            }],
            timezone: "America/New_York".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: TrackingScheduleConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    // ── 3. Missing fields default ─────────────────────────────────────────

    #[test]
    fn serde_missing_fields_default() {
        // Parsing `{}` must succeed with serde defaults (not call Default impl).
        // serde #[serde(default)] on each field drives this — no todo!() involved.
        let empty: TrackingScheduleConfig = serde_json::from_str("{}").unwrap();
        assert!(!empty.enabled);
        assert!(empty.windows.is_empty());
        assert_eq!(empty.timezone, "Local");

        // Parsing with only `enabled` set — other fields use serde defaults.
        let partial: TrackingScheduleConfig = serde_json::from_str(r#"{"enabled": true}"#).unwrap();
        assert!(partial.enabled);
        assert!(partial.windows.is_empty());
        assert_eq!(partial.timezone, "Local");
    }

    // ── 4. Overnight window wraps midnight ────────────────────────────────

    #[test]
    #[should_panic(expected = "A.3 impl")]
    fn overnight_window_wraps() {
        // Window 22:00–06:00 on Saturday.
        // Sat 23:00 → inside (Saturday in window hours 22-24)
        // Sun 01:00 → inside (Sunday in overnight carry-over hours 00-06)
        // Sat 21:00 → outside

        use chrono::NaiveDate;
        use chrono::TimeZone as _;
        use chrono_tz::UTC;

        let w = window("22:00", "06:00", vec![Weekday::Sat]);

        // 2024-11-09 is a Saturday in UTC.
        let sat_23 = to_local(
            UTC.from_utc_datetime(
                &NaiveDate::from_ymd_opt(2024, 11, 9)
                    .unwrap()
                    .and_hms_opt(23, 0, 0)
                    .unwrap(),
            ),
        );
        // 2024-11-10 is a Sunday, 01:00 UTC — carry-over from Saturday window.
        let sun_01 = to_local(
            UTC.from_utc_datetime(
                &NaiveDate::from_ymd_opt(2024, 11, 10)
                    .unwrap()
                    .and_hms_opt(1, 0, 0)
                    .unwrap(),
            ),
        );
        // 2024-11-09 Saturday 21:00 UTC — before window opens.
        let sat_21 = to_local(
            UTC.from_utc_datetime(
                &NaiveDate::from_ymd_opt(2024, 11, 9)
                    .unwrap()
                    .and_hms_opt(21, 0, 0)
                    .unwrap(),
            ),
        );

        assert!(
            w.window_is_active(sat_23),
            "Sat 23:00 should be inside overnight window"
        );
        assert!(
            w.window_is_active(sun_01),
            "Sun 01:00 should be inside overnight carry-over"
        );
        assert!(
            !w.window_is_active(sat_21),
            "Sat 21:00 should be outside window"
        );
    }

    // ── 5. Normal (non-wrapping) window ───────────────────────────────────

    #[test]
    #[should_panic(expected = "A.3 impl")]
    fn normal_window_does_not_wrap() {
        use chrono::NaiveDate;
        use chrono::TimeZone as _;
        use chrono_tz::UTC;

        // Window 12:00–13:00 on Monday only.
        let w = window("12:00", "13:00", vec![Weekday::Mon]);

        // 2024-11-11 is a Monday.
        let mon_1230 = to_local(
            UTC.from_utc_datetime(
                &NaiveDate::from_ymd_opt(2024, 11, 11)
                    .unwrap()
                    .and_hms_opt(12, 30, 0)
                    .unwrap(),
            ),
        );
        let mon_1301 = to_local(
            UTC.from_utc_datetime(
                &NaiveDate::from_ymd_opt(2024, 11, 11)
                    .unwrap()
                    .and_hms_opt(13, 1, 0)
                    .unwrap(),
            ),
        );
        let mon_1159 = to_local(
            UTC.from_utc_datetime(
                &NaiveDate::from_ymd_opt(2024, 11, 11)
                    .unwrap()
                    .and_hms_opt(11, 59, 0)
                    .unwrap(),
            ),
        );

        assert!(w.window_is_active(mon_1230), "Mon 12:30 should be active");
        assert!(
            !w.window_is_active(mon_1301),
            "Mon 13:01 should be outside window"
        );
        assert!(
            !w.window_is_active(mon_1159),
            "Mon 11:59 should be outside window"
        );
    }

    // ── 6. Empty days_of_week never active ────────────────────────────────

    #[test]
    #[should_panic(expected = "A.3 impl")]
    fn empty_days_never_active() {
        use chrono::NaiveDate;
        use chrono::TimeZone as _;
        use chrono_tz::UTC;

        let w = window("00:00", "23:59", vec![]);

        // Even a time that would match any time-of-day must be false.
        let any_time = to_local(
            UTC.from_utc_datetime(
                &NaiveDate::from_ymd_opt(2024, 11, 11)
                    .unwrap()
                    .and_hms_opt(12, 0, 0)
                    .unwrap(),
            ),
        );
        assert!(
            !w.window_is_active(any_time),
            "empty days_of_week must always return false"
        );
    }

    // ── 7. DST fall-back — ambiguous hour fires twice ─────────────────────

    #[test]
    #[should_panic(expected = "A.3 impl")]
    fn dst_fall_back_fires_twice() {
        // US/Eastern fall-back 2024-11-03: clocks go 02:00 EST → 01:00 EST.
        // A window covering 01:00–02:30 on Sunday must match BOTH the EDT
        // occurrence (01:30 EDT = 05:30 UTC) and the EST occurrence
        // (01:30 EST = 06:30 UTC).
        //
        // Per CONS-C04 / spec §3.7: window_is_active is defined on wall-clock
        // time (local HH:MM + day). Both absolute instants that share the same
        // local wall-clock value must match.

        use chrono::MappedLocalTime;
        use chrono::NaiveDate;
        use chrono::TimeZone as _;
        use chrono_tz::US::Eastern;

        let w = window("01:00", "02:30", vec![Weekday::Sun]);

        let naive_130 = NaiveDate::from_ymd_opt(2024, 11, 3)
            .unwrap()
            .and_hms_opt(1, 30, 0)
            .unwrap();

        // On fall-back day, 01:30 is ambiguous — two UTC instants share it.
        let mapped = Eastern.from_local_datetime(&naive_130);
        let (early, late) = match mapped {
            MappedLocalTime::Ambiguous(a, b) => (a, b),
            other => panic!("expected Ambiguous, got {:?}", other),
        };

        let t_early = to_local(early);
        let t_late = to_local(late);

        assert!(
            w.window_is_active(t_early),
            "01:30 EDT (early / DST occurrence) should be in window"
        );
        assert!(
            w.window_is_active(t_late),
            "01:30 EST (late / standard occurrence) should be in window"
        );
    }

    // ── 8. DST spring-forward — skipped hour never fires ─────────────────

    #[test]
    fn dst_spring_forward_window_in_skipped_hour_never_fires() {
        // US/Eastern spring-forward 2024-03-10: clocks jump 02:00 → 03:00.
        // Local time 02:30 does not exist on that day.
        // A window configured "02:30"–"02:59" on that Sunday must never match
        // any real instant because no real instant has that local time.
        //
        // This test is GREEN by construction: we build the "would-be" timestamp
        // via chrono-tz and verify MappedLocalTime::None, then skip calling
        // window_is_active (there is no valid DateTime<Local> to pass in).
        // The assertion is structural: the local time literally does not exist.

        use chrono::MappedLocalTime;
        use chrono::NaiveDate;
        use chrono::TimeZone as _;
        use chrono_tz::US::Eastern;

        let naive_0230 = NaiveDate::from_ymd_opt(2024, 3, 10)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();

        let mapped = Eastern.from_local_datetime(&naive_0230);
        // The skipped hour must produce MappedLocalTime::None — no real instant.
        assert!(
            matches!(mapped, MappedLocalTime::None),
            "02:30 on spring-forward day must be MappedLocalTime::None, got {:?}",
            mapped
        );
        // No call to window_is_active because there is no valid local instant
        // to pass in. The absence of any matching instant IS the assertion.
    }

    // ── 9. Serde rejects invalid HH:MM ────────────────────────────────────

    #[test]
    fn serde_rejects_invalid_hhmm() {
        // A.3 will add custom validation in Deserialize. Until then, the
        // derive-generated impl accepts any string — this test is RED (passes
        // without error, assertion fails). A.3 greens it by emitting
        // "validation.invalid_field" in the serde error message.
        let json = r#"{"start":"25:00","end":"08:00","days_of_week":["Mon"]}"#;
        let result = serde_json::from_str::<TrackingWindow>(json);
        assert!(
            result.is_err(),
            "deserialization of '25:00' must fail; got: {:?}",
            result.ok()
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("validation.invalid_field"),
            "expected 'validation.invalid_field' in error, got: {err_msg}"
        );
    }

    // ── 10. Serde rejects invalid IANA timezone ───────────────────────────

    #[test]
    fn serde_rejects_invalid_iana_timezone() {
        // A.3 will validate `timezone` as either "Local" or a valid IANA name
        // parseable by `chrono_tz::Tz::from_str`. Until then, derive accepts
        // any string — test is RED. A.3 greens it.
        let json = r#"{"enabled":true,"windows":[],"timezone":"Foo/Bar"}"#;
        let result = serde_json::from_str::<TrackingScheduleConfig>(json);
        assert!(
            result.is_err(),
            "deserialization of 'Foo/Bar' timezone must fail; got: {:?}",
            result.ok()
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("config.invalid"),
            "expected 'config.invalid' in error, got: {err_msg}"
        );
    }

    // ── 11. Empty end string is invalid ───────────────────────────────────

    #[test]
    fn window_with_empty_end_is_invalid() {
        // A.3 will validate HH:MM format in Deserialize; empty string is
        // structurally invalid. Until then, derive accepts it — test is RED.
        let json = r#"{"start":"09:00","end":"","days_of_week":["Mon"]}"#;
        let result = serde_json::from_str::<TrackingWindow>(json);
        assert!(
            result.is_err(),
            "deserialization of empty 'end' must fail; got: {:?}",
            result.ok()
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("validation.invalid_field"),
            "expected 'validation.invalid_field' in error, got: {err_msg}"
        );
    }

    // ── 12. Same-day end < start is invalid (not overnight) ───────────────

    #[test]
    fn window_end_before_start_not_same_day_is_invalid() {
        // start "13:00", end "12:00" with Mon-only days — this would be an
        // ambiguous 23h window. Per spec, A.3 rejects same-day configurations
        // where end <= start with only one day listed (no overnight intent
        // expressed by a multi-day window). A.3 decides the exact policy; the
        // test asserts the serde/validation path returns an error.
        //
        // Until A.3: derive accepts it — test is RED.
        let json = r#"{"start":"13:00","end":"12:00","days_of_week":["Mon"]}"#;
        let result = serde_json::from_str::<TrackingWindow>(json);
        assert!(
            result.is_err(),
            "start > end same-day window must be rejected; got: {:?}",
            result.ok()
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("validation.invalid_field"),
            "expected 'validation.invalid_field' in error, got: {err_msg}"
        );
    }
}
