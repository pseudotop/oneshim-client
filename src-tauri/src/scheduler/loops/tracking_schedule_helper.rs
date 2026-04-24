//! Tracking-schedule scheduler helpers — Phase 9 PR-A.
//!
//! This module exposes two pure functions consumed by the scheduler:
//!
//! * [`tracking_schedule_active`] — returns `true` when the current instant
//!   falls inside any configured tracking-schedule window.
//!
//! * [`capture_permitted_now`] — composes all four privacy gates per spec §3.4:
//!   `consent_granted AND active_hours AND !tracking_schedule_active AND !capture_paused`.
//!
//! A.5 will replace the `todo!()` stubs with real implementations.

use chrono::{DateTime, Local};
use oneshim_core::config::AppConfig;
use oneshim_core::consent::ConsentPermissions;

// ── Public stubs ────────────────────────────────────────────────────────────

/// Returns `true` when `now` falls inside any configured tracking-schedule
/// mute window.
///
/// When `cfg.tracking_schedule.enabled` is `false` or the `windows` list is
/// empty, the schedule is considered inactive and this returns `false`.
///
/// A.5 provides the real implementation.
#[allow(dead_code)]
pub(crate) fn tracking_schedule_active(_cfg: &AppConfig, _now: DateTime<Local>) -> bool {
    todo!("A.5 impl")
}

/// Returns `true` when capture is permitted right now.
///
/// Composes all 4 privacy gates per spec §3.4:
///
/// ```text
/// capture_permitted_now =
///     consent.screen_capture                    // consent top-authority gate
///     AND should_run_now_with_time(cfg, now)    // active_hours gate
///     AND !tracking_schedule_active(cfg, now)   // tracking-schedule negative gate
///     AND !capture_paused                        // user tray-toggle veto
/// ```
///
/// All four gates must be true for capture to be permitted. Any single `false`
/// propagates to `false` regardless of the other gate values (CONS-PC02).
///
/// A.5 provides the real implementation.
#[allow(dead_code)]
pub(crate) fn capture_permitted_now(
    _cfg: &AppConfig,
    _consent: &ConsentPermissions,
    _capture_paused: bool,
    _now: DateTime<Local>,
) -> bool {
    todo!("A.5 impl")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, NaiveDate, TimeZone as _, Timelike};
    use oneshim_core::config::Weekday;
    use oneshim_core::config::{TrackingScheduleConfig, TrackingWindow};

    // ── Fixture helpers ─────────────────────────────────────────────────────

    /// Build a `DateTime<Local>` for a known-Monday date (2024-01-08) at the
    /// given HH:MM, using a UTC+0 FixedOffset so wall-clock == date literal
    /// and the result is independent of the test machine's timezone.
    ///
    /// Returns `DateTime<Local>` via a cast that preserves the wall-clock value.
    fn monday_at(hour: u32, minute: u32) -> DateTime<Local> {
        // 2024-01-08 is a Monday (verified: python3 -c "import datetime; print(datetime.date(2024,1,8).strftime('%A'))" → Monday)
        fixed_at(2024, 1, 8, hour, minute) // Monday
    }

    /// Like `monday_at`, but for Wednesday 2024-01-10.
    fn wednesday_at(hour: u32, minute: u32) -> DateTime<Local> {
        fixed_at(2024, 1, 10, hour, minute) // Wednesday
    }

    /// Build a `DateTime<Local>` for any ymd/hms via FixedOffset UTC+0.
    fn fixed_at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        let utc = FixedOffset::east_opt(0).unwrap();
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(hour, minute, 0)
            .unwrap();
        let dt = utc.from_local_datetime(&naive).unwrap();
        // Cast to Local: chrono allows this via fixed_offset().with_timezone().
        // Both instants represent the same absolute UTC point; Local just
        // re-labels the timezone. For these tests the machine timezone is
        // irrelevant because we only care about the function's behavior given
        // the `now` argument — the impl must not call `Local::now()` internally.
        dt.with_timezone(&Local)
    }

    /// Build a `TrackingWindow` for use in tests (no label, panics on invalid input
    /// only if A.3 validation rejects it — all test windows here are valid).
    fn window(start: &str, end: &str, days: Vec<Weekday>) -> TrackingWindow {
        TrackingWindow {
            start: start.to_string(),
            end: end.to_string(),
            days_of_week: days,
            label: String::new(),
        }
    }

    /// Build an `AppConfig` with a custom `TrackingScheduleConfig`.
    fn cfg_with_ts(ts: TrackingScheduleConfig) -> AppConfig {
        let mut cfg = AppConfig::default_config();
        cfg.tracking_schedule = ts;
        cfg
    }

    /// Build an `AppConfig` with active_hours gate enabled for the given
    /// hour range [start, end) on all weekdays (Mon–Fri).
    ///
    /// Sets `schedule.active_hours_enabled = true`, `active_start_hour`, and
    /// `active_end_hour`. Used by the 16-row truth table to put the active_hours
    /// gate in a known state.
    #[allow(dead_code)]
    fn cfg_with_active_hours(start_h: u8, end_h: u8, days: Vec<Weekday>) -> AppConfig {
        let mut cfg = AppConfig::default_config();
        cfg.schedule.active_hours_enabled = true;
        cfg.schedule.active_start_hour = start_h;
        cfg.schedule.active_end_hour = end_h;
        cfg.schedule.active_days = days;
        cfg
    }

    /// Build consent with `screen_capture` set to `granted`.
    fn consent(screen_capture: bool) -> ConsentPermissions {
        ConsentPermissions {
            screen_capture,
            ..Default::default()
        }
    }

    // ── tracking_schedule_active tests ──────────────────────────────────────

    /// Test 1: disabled config → false regardless of `now`.
    #[test]
    fn disabled_config_returns_false() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: false,
            windows: vec![window("12:00", "13:00", vec![Weekday::Mon])],
            timezone: "Local".to_string(),
        });
        // The window would match Mon 12:30, but enabled=false must override.
        assert!(!tracking_schedule_active(&cfg, monday_at(12, 30)));
    }

    /// Test 2: enabled=true but empty windows → false.
    #[test]
    fn empty_windows_returns_false() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![],
            timezone: "Local".to_string(),
        });
        assert!(!tracking_schedule_active(&cfg, monday_at(12, 30)));
    }

    /// Test 3: single window in range → true.
    #[test]
    fn normal_window_in_range() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![window("12:00", "13:00", vec![Weekday::Mon])],
            timezone: "Local".to_string(),
        });
        // Mon 12:30 is inside [12:00, 13:00) on Monday.
        assert!(tracking_schedule_active(&cfg, monday_at(12, 30)));
    }

    /// Test 4: single window, `now` is outside range → false.
    #[test]
    fn normal_window_out_of_range() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![window("12:00", "13:00", vec![Weekday::Mon])],
            timezone: "Local".to_string(),
        });
        // Mon 13:01 is past the window end.
        assert!(!tracking_schedule_active(&cfg, monday_at(13, 1)));
    }

    /// Test 5: multiple windows, one matches → true.
    #[test]
    fn multiple_windows_one_matches() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![
                window("12:00", "13:00", vec![Weekday::Mon]),
                window("18:00", "22:00", vec![Weekday::Mon]),
            ],
            timezone: "Local".to_string(),
        });
        // Mon 19:00 is inside [18:00, 22:00).
        assert!(tracking_schedule_active(&cfg, monday_at(19, 0)));
    }

    /// Test 6: multiple windows, none match → false.
    #[test]
    fn multiple_windows_none_match() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![
                window("12:00", "13:00", vec![Weekday::Mon]),
                window("18:00", "22:00", vec![Weekday::Mon]),
            ],
            timezone: "Local".to_string(),
        });
        // Mon 10:00 is before both windows.
        assert!(!tracking_schedule_active(&cfg, monday_at(10, 0)));
    }

    /// Test 7: timezone = "Local" smoke-test — function must not panic.
    #[test]
    fn timezone_local_uses_chrono_local() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![window(
                "00:00",
                "23:59",
                vec![
                    Weekday::Mon,
                    Weekday::Tue,
                    Weekday::Wed,
                    Weekday::Thu,
                    Weekday::Fri,
                    Weekday::Sat,
                    Weekday::Sun,
                ],
            )],
            timezone: "Local".to_string(),
        });
        // With a window covering virtually all hours on all days, the result
        // must be `true` when the function runs. We just verify it doesn't
        // panic and returns a bool.
        let result = tracking_schedule_active(&cfg, monday_at(12, 0));
        assert!(
            result,
            "all-day window on all days must be active at Mon 12:00"
        );
    }

    // ── capture_permitted_now tests ─────────────────────────────────────────

    // ── 16-row truth table (Test 8) ────────────────────────────────────────

    /// Test 8: All 2⁴ combinations of the four gates.
    ///
    /// Each combination is constructed by building an `AppConfig` that puts
    /// each gate into the desired state, then asserting that
    /// `capture_permitted_now` returns `consent AND active_hours AND ts_inactive
    /// AND !capture_paused`.
    ///
    /// Gate construction:
    ///   - `consent`: `ConsentPermissions { screen_capture: true/false, .. }`
    ///   - `active_hours`: schedule.active_hours_enabled = true, window covers
    ///     the test `now` (Mon 12:30 / Wed 23:00 accordingly)
    ///   - `ts_inactive` (= !tracking_schedule_active): TS enabled with a window
    ///     that does NOT cover `now` → active = false → ts_inactive = true
    ///     OR TS disabled / empty → ts_inactive = true
    ///   - `capture_paused`: passed directly as argument
    #[test]
    fn capture_permitted_combines_all_four_gates() {
        // Fixed test instant: Monday 12:30.
        // For each combo we build cfg to achieve the desired gate state.
        let now = monday_at(12, 30);

        // Iterate all 16 (bool×bool×bool×bool) combinations.
        for consent_val in [true, false] {
            for active_hours_val in [true, false] {
                for ts_inactive_val in [true, false] {
                    for paused_val in [true, false] {
                        let cfg = build_scenario_cfg(active_hours_val, ts_inactive_val, now);
                        let c = consent(consent_val);
                        let expected =
                            consent_val && active_hours_val && ts_inactive_val && !paused_val;

                        let got = capture_permitted_now(&cfg, &c, paused_val, now);

                        assert_eq!(
                            got, expected,
                            "combo (consent={consent_val}, active_hours={active_hours_val}, \
                             ts_inactive={ts_inactive_val}, paused={paused_val}) expected \
                             {expected} but got {got}"
                        );
                    }
                }
            }
        }
    }

    /// Builds an `AppConfig` that puts active_hours and ts_inactive gates into
    /// the desired states for the given test `now`.
    ///
    /// - `active_hours=true`: schedule.active_hours_enabled + window covers `now`'s hour.
    /// - `active_hours=false`: schedule.active_hours_enabled + window does NOT cover `now`.
    /// - `ts_inactive=true` (TS not firing): TS disabled (enabled=false).
    /// - `ts_inactive=false` (TS firing): TS enabled with a window covering `now`.
    fn build_scenario_cfg(
        active_hours_val: bool,
        ts_inactive_val: bool,
        now: DateTime<Local>,
    ) -> AppConfig {
        // now = Monday 12:30 in all callers from the truth table.
        let hour = now.time().hour() as u8;

        let mut cfg = AppConfig::default_config();

        // ── Active hours gate ─────────────────────────────────────────────
        if active_hours_val {
            // Enable, window covers `now` (12:00–13:00).
            cfg.schedule.active_hours_enabled = true;
            cfg.schedule.active_start_hour = hour; // e.g. 12
            cfg.schedule.active_end_hour = hour + 1; // e.g. 13
            cfg.schedule.active_days = vec![Weekday::Mon];
        } else {
            // Enable, but window does NOT cover `now` (e.g. 14:00–15:00).
            cfg.schedule.active_hours_enabled = true;
            cfg.schedule.active_start_hour = hour + 2; // e.g. 14
            cfg.schedule.active_end_hour = hour + 3; // e.g. 15
            cfg.schedule.active_days = vec![Weekday::Mon];
        }

        // ── Tracking schedule gate ────────────────────────────────────────
        if ts_inactive_val {
            // TS not firing: disabled entirely.
            cfg.tracking_schedule = TrackingScheduleConfig {
                enabled: false,
                windows: vec![],
                timezone: "Local".to_string(),
            };
        } else {
            // TS firing: enabled with a window that covers `now` (12:00–13:00 Mon).
            cfg.tracking_schedule = TrackingScheduleConfig {
                enabled: true,
                windows: vec![window("12:00", "13:00", vec![Weekday::Mon])],
                timezone: "Local".to_string(),
            };
        }

        cfg
    }

    /// Test 9: overnight active_hours (22:00–06:00, Mon–Fri) + empty TS at
    /// Wed 23:00 → capture_permitted_now returns `true`.
    ///
    /// This exercises the post-CONS-C05 fix where `should_run_now_with_time`
    /// must correctly wrap an overnight window. Consent is granted,
    /// capture_paused is false, TS is disabled.
    #[test]
    fn capture_permitted_respects_should_run_now_wrap() {
        // Wed 23:00 is inside the overnight window [22:00, 06:00) on Wed.
        let now = wednesday_at(23, 0);

        let mut cfg = AppConfig::default_config();
        // Overnight active_hours window: 22:00–06:00 on Mon–Fri.
        // A.5's should_run_now_with_time must handle end_hour < start_hour wrap.
        cfg.schedule.active_hours_enabled = true;
        cfg.schedule.active_start_hour = 22;
        cfg.schedule.active_end_hour = 6; // overnight wrap: 22 > 6
        cfg.schedule.active_days = vec![
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ];
        // TS disabled — not firing.
        cfg.tracking_schedule = TrackingScheduleConfig::default();

        let c = consent(true); // consent granted
        let capture_paused = false;

        assert!(
            capture_permitted_now(&cfg, &c, capture_paused, now),
            "Wed 23:00 with overnight 22-06 active_hours + no TS + consent granted \
             must be permitted (CONS-C05 overnight wrap)"
        );
    }

    /// Test 10: consent revoked overrides TS inactive + active_hours active
    /// (CONS-PC02 — consent has top-authority veto).
    #[test]
    fn consent_revoked_overrides_ts_inactive_active_hours() {
        let now = monday_at(12, 30);
        let mut cfg = AppConfig::default_config();
        // Active hours cover now.
        cfg.schedule.active_hours_enabled = true;
        cfg.schedule.active_start_hour = 12;
        cfg.schedule.active_end_hour = 13;
        cfg.schedule.active_days = vec![Weekday::Mon];
        // TS not firing.
        cfg.tracking_schedule = TrackingScheduleConfig::default();

        // Consent revoked: screen_capture = false.
        let c = consent(false);
        let capture_paused = false;

        assert!(
            !capture_permitted_now(&cfg, &c, capture_paused, now),
            "consent revoked (screen_capture=false) must veto capture even when \
             active_hours and TS are both permitting (CONS-PC02)"
        );
    }

    /// Test 11: capture_paused veto — even when TS inactive + active_hours +
    /// consent granted, capture_paused=true → false (CONS-PC02).
    #[test]
    fn capture_paused_overrides_ts_inactive() {
        let now = monday_at(12, 30);
        let mut cfg = AppConfig::default_config();
        // All other gates permitting.
        cfg.schedule.active_hours_enabled = true;
        cfg.schedule.active_start_hour = 12;
        cfg.schedule.active_end_hour = 13;
        cfg.schedule.active_days = vec![Weekday::Mon];
        cfg.tracking_schedule = TrackingScheduleConfig::default();

        let c = consent(true); // consent granted
        let capture_paused = true; // user tray-toggle veto

        assert!(
            !capture_permitted_now(&cfg, &c, capture_paused, now),
            "capture_paused=true must veto capture even when all other gates permit (CONS-PC02)"
        );
    }

    // ── Clock-irregularity coverage (CONS-PI07 / spec §3.7a) ──────────────

    /// Test 12a: suspend/resume — `tracking_schedule_active` returns correct
    /// value regardless of tick timing between suspend/resume. Tested via two
    /// distinct `now` values flanking a suspend interval.
    #[test]
    fn window_active_across_suspend() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![window("12:00", "13:00", vec![Weekday::Mon])],
            timezone: "Local".to_string(),
        });

        // Pre-suspend: Mon 12:10 — inside window.
        let pre_suspend = monday_at(12, 10);
        assert!(
            tracking_schedule_active(&cfg, pre_suspend),
            "Mon 12:10 must be inside window before suspend"
        );

        // Post-resume: Mon 13:10 — outside window (machine was suspended 60 min).
        let post_resume = monday_at(13, 10);
        assert!(
            !tracking_schedule_active(&cfg, post_resume),
            "Mon 13:10 must be outside window after suspend/resume (CONS-PI07)"
        );
    }

    /// Test 12b: forward clock jump into future window.
    ///
    /// Clock jumps from Mon 11:50 to Mon 12:30 (window [12:00, 13:00)).
    /// Returns `true` after jump.
    #[test]
    fn forward_clock_jump_into_future_window() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![window("12:00", "13:00", vec![Weekday::Mon])],
            timezone: "Local".to_string(),
        });

        // Before jump: Mon 11:50 — outside window.
        let before_jump = monday_at(11, 50);
        assert!(
            !tracking_schedule_active(&cfg, before_jump),
            "Mon 11:50 must be outside [12:00, 13:00) before clock jump"
        );

        // After jump: Mon 12:30 — inside window.
        let after_jump = monday_at(12, 30);
        assert!(
            tracking_schedule_active(&cfg, after_jump),
            "Mon 12:30 must be inside [12:00, 13:00) after forward clock jump (CONS-PI07)"
        );
    }

    /// Test 12c: forward clock jump past window end.
    ///
    /// Clock jumps from Mon 12:50 to Mon 13:10.
    /// Returns `false` after jump (window [12:00, 13:00) has ended).
    #[test]
    fn forward_clock_jump_past_window_end() {
        let cfg = cfg_with_ts(TrackingScheduleConfig {
            enabled: true,
            windows: vec![window("12:00", "13:00", vec![Weekday::Mon])],
            timezone: "Local".to_string(),
        });

        // Before jump: Mon 12:50 — inside window.
        let before_jump = monday_at(12, 50);
        assert!(
            tracking_schedule_active(&cfg, before_jump),
            "Mon 12:50 must be inside [12:00, 13:00) before clock jump"
        );

        // After jump: Mon 13:10 — outside window (jumped past end).
        let after_jump = monday_at(13, 10);
        assert!(
            !tracking_schedule_active(&cfg, after_jump),
            "Mon 13:10 must be outside [12:00, 13:00) after forward clock jump past end (CONS-PI07)"
        );
    }

    // ── Additional edge-case tests ──────────────────────────────────────────

    /// Test 13: all four gates false → false.
    #[test]
    fn all_gates_false_returns_false() {
        let now = monday_at(12, 30);
        // active_hours: enabled but window does NOT cover now (14-15).
        let mut cfg = AppConfig::default_config();
        cfg.schedule.active_hours_enabled = true;
        cfg.schedule.active_start_hour = 14;
        cfg.schedule.active_end_hour = 15;
        cfg.schedule.active_days = vec![Weekday::Mon];
        // TS: enabled with window covering now → ts_active = true → ts_inactive = false.
        cfg.tracking_schedule = TrackingScheduleConfig {
            enabled: true,
            windows: vec![window("12:00", "13:00", vec![Weekday::Mon])],
            timezone: "Local".to_string(),
        };

        let c = consent(false); // revoked
        let capture_paused = true;

        assert!(
            !capture_permitted_now(&cfg, &c, capture_paused, now),
            "all four gates false must return false"
        );
    }

    /// Test 14: all four gates true → true.
    #[test]
    fn all_gates_true_returns_true() {
        let now = monday_at(12, 30);
        let mut cfg = AppConfig::default_config();
        // active_hours covers now.
        cfg.schedule.active_hours_enabled = true;
        cfg.schedule.active_start_hour = 12;
        cfg.schedule.active_end_hour = 13;
        cfg.schedule.active_days = vec![Weekday::Mon];
        // TS not firing (disabled).
        cfg.tracking_schedule = TrackingScheduleConfig::default();

        let c = consent(true); // granted
        let capture_paused = false;

        assert!(
            capture_permitted_now(&cfg, &c, capture_paused, now),
            "all four gates true must return true"
        );
    }
}
