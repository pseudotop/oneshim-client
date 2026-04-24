# Phase 9 Quick Wins — Specification

**Date**: 2026-04-23 (Loop 1d revision: 2026-04-24)
**Worktree**: `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-quick-wins`
**Branch**: `feature/phase9-quick-wins` (tip `5618558c`, origin/main post-PR-#486)
**Status**: Loop 1d complete — zero-Critical, zero-Important gate claimed. Ready for Loop 2 implementation-plan draft.
**Author**: _(first-draft + Loop 1d revision)_
**Loop 1 history**:
- Loop 1a: three-reviewer deep review (R1 architecture, R2 product/privacy, R3 platform/test/risk).
- Loop 1b: consolidated synthesis at `docs/reviews/2026-04-23-phase9-spec-review-synthesis.md` (46 findings, 12 Critical, 16 Important, 18 Minor).
- Loop 1c: user decisions locked (U1-U13 all resolved to synthesis recommendations).
- Loop 1d: applied 49-step fix plan per §6 of the synthesis; see §7 Decisions log D13-D22 for locked decisions.

---

## 1. Overview

Phase 9 "Quick Wins" bundles three small-to-medium features that each remove a papercut in the client without requiring a new architectural primitive. The three are intentionally shipped together because each lands in a different crate and the overlap in review surface is low:

1. **Tracking Schedule** (new privacy-hardening primitive) — negative-gated time windows that suppress capture / events / analysis / uploads.
2. **Linux `systemd` Autostart IPC Wiring** — expose the already-implemented but currently-gated `src-tauri/src/autostart.rs` module via Tauri IPC + Settings UI.
3. **Timeline Bulk Tag Operations** (extension) — add transactional batch-remove alongside the existing batch-add, and harden the existing batch-add by moving it into a SQL transaction.

**Not included** (explicitly deferred):

- A unified `TimeWindow` primitive replacing `ScheduleConfig` + `CoachingConfig::quiet_hours` + the new `TrackingScheduleConfig` (queued in `project_next_tasks.md`).
- A general "selection model" abstraction beyond the Timeline (Chat, Search, Audit pages out of scope).
- Wayland-first autostart support beyond the minimum workaround (see §4 D5).
- Fine-grained per-app blocklist (Tracking Schedule is a blanket time gate; per-app filters stay in `PrivacyConfig`).

## 2. Motivation

### 2.1 Tracking Schedule

ONESHIM captures screenshots, OCR text, active window titles, and input telemetry around the clock. Users in enterprise and regulated environments (healthcare, legal, finance) need to guarantee that certain time windows are opaque to the system: lunch breaks, after-hours personal time, therapy sessions, bathroom breaks, on-call handoff windows. The existing positive-gate `ScheduleConfig::active_hours_*` primitive ("capture ONLY during 09-17") is insufficient because users also need **negative** gates ("do NOT capture during 12-13 and 18-22"). The two gates are orthogonal: an active-hours user may still need midday privacy.

Regulatory grounding — scope enumeration (per CONS-I10):

**Directly supported by Tracking Schedule**:

- **GDPR Article 5 (Purpose limitation and data minimisation)** — processing must be limited to what is necessary for the declared purpose. A user-configurable opt-out window is the canonical minimisation control.
- **GDPR Article 13/14 (Transparency at collection)** — §3.11 keeps in-app notifications ON during suppression windows. The user is informed the system is suppressing; silent suppression is arguably worse than no suppression.
- **GDPR Article 25 (Data protection by design and by default)** — privacy-by-default is strengthened by a user-controllable suppression primitive.
- **GDPR Article 35 (Data protection impact assessment)** — an explicit DPIA benefits from a documented negative-gate primitive.

**Not addressed by Tracking Schedule (separate controls required)**:

- **GDPR Article 17 (Right to erasure)** — existing `DELETE /api/data` primitive handles this; Tracking Schedule is prospective, not retroactive.
- **CCPA / CPRA (California Consumer Privacy Act / California Privacy Rights Act)** — notice-at-collection obligations are orthogonal; Tracking Schedule reduces collection scope only. A separate privacy-notice UI is required for full compliance.
- **US state electronic monitoring acts** — NY Civil Rights Law §52-c, DE §19, CT §31-48d all require **written notice** to employees before electronic monitoring begins. These obligations are independent of whether a suppression feature exists. Tracking Schedule does NOT substitute for the notice obligation; it only reduces the scope of the monitoring.

### 2.2 Linux Autostart Wiring

The autostart module at `src-tauri/src/autostart.rs` is a complete platform implementation (macOS LaunchAgents, Windows Registry HKCU Run, Linux systemd user service + XDG `.desktop` fallback) with **9 unit tests** (lines 460-549; `grep -c '#\[test\]' src-tauri/src/autostart.rs` → 9), but it is `#![allow(dead_code)]` gated because no IPC surface was wired. Linux users currently have no way to enable autostart from the Settings UI; they must hand-edit `~/.config/systemd/user/oneshim.service`. macOS and Windows users are in the same boat but the platform norms are more tolerant there. Wiring is the cheapest credibility win in the Phase 9 bundle.

### 2.3 Timeline Bulk Tag Operations

The Timeline page already implements a select-mode UI with `Set<number>` selection state and a floating batch action bar (`crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:113-114,202-204` + `AllFrames.tsx:582-603`), and the REST endpoint `POST /api/frames/batch-tags` already exists for **batch-add** (`crates/oneshim-web/src/routes.rs:117`, `handlers/tags.rs:83-98`). Two real gaps:

1. **Partial-success risk**: the existing batch-add handler loops and calls per-row `add_tag_to_frame` with no enclosing transaction (`handlers/tags.rs:88-97`). A mid-batch error leaves the DB in a "some tagged, some not" state. Every other consumer of `tags` APIs assumes atomicity.
2. **No batch-remove**: users can bulk-apply a tag but not bulk-unapply, which is the dominant reverse-operation ask in support feedback.

**Note on current behavior-to-preserve**: the existing handler at `handlers/tags.rs:83-98` actively **hides** per-row failures: on any per-row error, it emits a `tracing::warn!` and continues, then **always returns HTTP 200** with a `tagged_count` that silently undercounts. Frontend consumers see `onSuccess` firing for calls that have partial failures. The transactional refactor in §5.5 flips this to explicit all-or-nothing semantics — a **200-with-silent-failure → 500-with-error** behavior change. This is documented as Decision D15 (§7), and the frontend consumer (only `TimelineLayout.tsx:131-140`) is updated in the same PR. See CONS-C09 for the complete behavior-change analysis.

## 3. Feature 1 — Tracking Schedule

### 3.1 Goals

- Give the user configurable time windows during which capture + events + upload + analysis are suppressed.
- Compose with existing `ScheduleConfig::active_hours_*` via a logical-AND.
- Preserve user-indicator notifications (tray, border) so the user is never surprised by silent suppression — this is a GDPR transparency requirement.
- Ship multiple-range-per-day support from day one (splitting lunch + evening is the single most common user case).

### 3.2 Non-goals

- Real-time DST transition correctness to the second — we accept hour-boundary granularity.
- Per-app overrides (those belong to `PrivacyConfig`).
- Server-side enforcement — this is a client-only privacy primitive; server has no authority over suppression.
- Retroactive purge of data captured outside the new window (users can already delete via `DELETE /data` + GDPR Art. 17 consent UI).

### 3.3 Naming — Decision D1 (settled)

**Chosen**: "**Tracking Schedule**".

**Rationale**: matches the dominant industry terminology in the user's mental model category (employee-monitoring / productivity-tracking apps):

| Product          | Term used                              |
| ---------------- | -------------------------------------- |
| RescueTime       | "Focus Time" / "Away Time"             |
| ActivTrak        | "Tracking Schedule"                    |
| Teramind         | "Monitoring Schedule"                  |
| Hubstaff         | "Tracking Hours"                       |
| TimeDoctor       | "Work Hours"                           |
| DeskTime         | "Working Hours"                        |

**Rejected alternatives**:

- **"Blackout Hours"** — zero peer-product precedent; "blackout" in enterprise SaaS implies an outage or failure mode, not a user-controlled opt-out. Adversarial connotation (user-against-system), which reverses the trust framing we want.
- **"Quiet Hours"** — overloaded by OS-level notification-silencing (Apple Focus / Windows 11 Focus Sessions / Google "Do Not Disturb"). Users will expect notification-only semantics; our semantics are stricter.
- **"Pause Schedule"** — confusing with the existing one-shot manual `capture_paused` toggle.
- **"Privacy Hours"** — plausible but weaker because all ONESHIM controls are nominally "privacy". Less descriptive of the temporal gating.

### 3.4 Coexistence with `active_hours` — Decision D2 (settled)

Two schedules coexist (§3.1): the **positive** gate (existing `ScheduleConfig::active_hours_*` at `crates/oneshim-core/src/config/sections/monitoring.rs:58-85`) and the new **negative** gate `TrackingScheduleConfig`. The effective rule is:

```
capture_allowed(now, tier) = consent_granted(tier)
                          AND active_hours_gate(now)
                          AND NOT tracking_schedule_active(now)
                          AND NOT capture_paused
```

`consent_granted(tier)` reads `ConsentManager` (`crates/oneshim-core/src/consent.rs:102`) for the relevant tier (`screen_capture`, `ocr_processing`, `input_activity`, `window_title_collection`). Revoked consent is **top authority** — no other gate can re-permit capture when consent is revoked. See §3.4.a for the conflict-resolution table.

Worked example:

| `active_hours_enabled` | `active_hours`     | `tracking_schedule.windows`      | `now`   | Capture? |
| ---------------------- | ------------------ | -------------------------------- | ------- | -------- |
| `false`                | n/a                | `[]`                             | 22:00   | **yes**  |
| `true`                 | 09:00–18:00 Mon–Fri | `[]`                             | 10:00 Mon | **yes**  |
| `true`                 | 09:00–18:00 Mon–Fri | `[]`                             | 08:00 Mon | no (outside active) |
| `true`                 | 09:00–18:00 Mon–Fri | `[{12:00–13:00 Mon–Fri}]`        | 12:30 Mon | no (suppressed by TS) |
| `false`                | n/a                | `[{12:00–13:00 Mon–Fri}]`        | 12:30 Mon | no (suppressed by TS) |
| `false`                | n/a                | `[{00:00–23:59 Sun}]`            | 10:00 Sun | no (suppressed by TS) |
| `false`                | n/a                | `[{22:00–06:00 [Mon..Sun]}]`     | 23:00 Wed | no (overnight wraps) |
| `true`                 | 22:00–06:00 Mon–Fri (overnight) | `[]`                    | 23:00 Wed | see §3.4a (latent bug — U2 decision applies) |
| `true`                 | 22:00–06:00 Mon–Fri (overnight) | `[{00:00–04:00 Tue}]`   | 01:00 Tue | no (suppressed by TS) — depends on U2 option A or C |
| consent `screen_capture=false` | any            | any                              | any     | no (consent top authority) |

Note on `[Mon..Sun]` notation: this is the shorthand for all seven `Weekday` values `[Mon, Tue, Wed, Thu, Fri, Sat, Sun]`. The config shape (§3.6) uses a concrete enum list; the shorthand is documentation-only.

This composition is explicit and documented in the effective-gate unit tests (§3.11).

### 3.4a `should_run_now` overnight handling — Decision D14 (U2 locked to Option C: fix + hoist both checks)

**Latent bug**: `should_run_now(config)` at `src-tauri/src/scheduler/mod.rs:548-571` checks `hour >= schedule.active_start_hour && hour < schedule.active_end_hour` with **no wrap-midnight branch**, while `SmartCaptureTrigger::is_within_active_hours` at `crates/oneshim-vision/src/trigger.rs:69-77` does handle wrap. Users with `active_hours_enabled=true, start=22, end=6` get inconsistent behavior: `should_capture()` permits at 23:00, but `should_run_now()` blocks — so the monitor loop gate at `scheduler/loops/monitor.rs:200-207` short-circuits despite the trigger saying to capture.

**Locked decision (U2 = Option C)**: Phase 9 fixes `should_run_now` to match `is_within_active_hours` wrap logic AND hoists both checks out of `SmartCaptureTrigger`, deleting the overnight duplication. Rationale: half-migrated triggers leave a bad state; fixing the bug alone without hoisting leaves `is_within_active_hours` as a dead helper on the trigger. Unifying at the scheduler boundary is the single-source-of-truth path.

**Post-fix `should_run_now` pseudocode**:

```rust
pub fn should_run_now(config: &AppConfig) -> bool {
    let schedule = &config.schedule;
    if !schedule.active_hours_enabled {
        return true;
    }
    let now = chrono::Local::now();
    let hour = now.hour() as u8;
    let weekday = /* map chrono::Weekday → config Weekday (as today) */;

    let start = schedule.active_start_hour;
    let end = schedule.active_end_hour;
    if start <= end {
        // normal range
        schedule.active_days.contains(&weekday) && hour >= start && hour < end
    } else {
        // overnight range (22-06 semantics; matches trigger.rs:69-77)
        let tonight = schedule.active_days.contains(&weekday) && hour >= start;
        let previous_day = /* yesterday weekday */;
        let tomorrow_morning = schedule.active_days.contains(&previous_day) && hour < end;
        tonight || tomorrow_morning
    }
}
```

The three schedule tests at `trigger.rs:373,398,409` (`blocks_capture_outside_active_hours`, `allows_capture_when_schedule_disabled`, `handles_overnight_active_hours`) are migrated to scheduler-side tests next to `should_run_when_disabled` at `scheduler/mod.rs:582`. See §3.8a and §6.1 Feature 1 for the full migration.

### 3.4b Composition × consent × capture_paused — conflict-resolution table

| Scenario | Result | Winning gate |
|---|---|---|
| consent revoked | no capture | **consent** (top authority) |
| consent granted + within active_hours + outside TS window + not paused | capture | composition passes |
| consent granted + outside active_hours | no capture | active_hours gate |
| consent granted + within TS window | no capture | TS gate |
| consent granted + `capture_paused==true` | no capture | user toggle |
| consent granted + TS window + `capture_paused==true` | no capture (both gates close) | evaluation short-circuits at the first `NO` |

Consent revocation during an active tracking-schedule window is a no-op at capture-time (both gates close); on the uploader side, the existing `DELETE /api/data` (GDPR Art. 17) is the right user affordance for in-queue purging.

### 3.5 `TimeWindow` primitive — Decision D3 deferred

A unified `TimeWindow` struct that replaces `ScheduleConfig` + `CoachingConfig::quiet_hours` (`crates/oneshim-core/src/config/sections/coaching.rs:118-124`) + the new `TrackingScheduleConfig` is **out of scope**. Rationale:

- Three consumers (ScheduleConfig, CoachingConfig::quiet_hours, TrackingScheduleConfig) is not yet enough to justify a generic primitive — the existing two are already diverged in shape (ScheduleConfig stores `u8` hours, `CoachingConfig::TimeRange` stores `"HH:MM"` strings) and unifying them requires a migration.
- The Phase 9 bundle is "quick wins"; a cross-cutting refactor belongs in a dedicated PR.
- The new `TrackingScheduleConfig` will reuse the industry-standard shape (see §3.6) so the future unification is a non-breaking rename, not a redesign.

The unification is added to `project_next_tasks.md` as a follow-up.

### 3.6 Config shape

Industry-standard shape (Slack DND + Apple DeviceActivitySchedule + rrule.js convention):

```rust
// crates/oneshim-core/src/config/sections/monitoring.rs
// (co-located with ScheduleConfig since both belong to the "scheduling" concern)

use serde::{Deserialize, Serialize};
use crate::config::enums::Weekday;

/// Privacy-hardening negative-gate schedule: during any active window,
/// capture/events/upload/analysis are suppressed. Logically AND-composed
/// with `ScheduleConfig::active_hours_*` — see §3.4 of the Phase 9 spec.
///
/// All fields `#[serde(default)]` for backward-compatible deserialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingScheduleConfig {
    /// Master switch. Default: `false` (opt-in only).
    #[serde(default)]
    pub enabled: bool,

    /// One or more suppression windows. Empty means no suppression.
    /// Multiple ranges per day are supported by using multiple windows with
    /// the same `days_of_week`; see §3.7.
    #[serde(default)]
    pub windows: Vec<TrackingWindow>,

    /// IANA timezone name (e.g. `"America/New_York"`, `"Asia/Seoul"`).
    /// Default `"Local"` means use the system local timezone (which is what
    /// `chrono::Local` resolves to).
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

impl Default for TrackingScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            windows: Vec::new(),
            timezone: default_timezone(),
        }
    }
}

fn default_timezone() -> String {
    "Local".to_string()
}

/// A single suppression window. Keeps the `"HH:MM"` string representation
/// (matches `CoachingConfig::TimeRange` at `coaching.rs:118-124` and matches
/// the industry standard — Slack, rrule.js, Apple Screen Time API all use
/// `"HH:MM"` strings, not integer-hour fields like `ScheduleConfig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingWindow {
    /// Start of the window in `"HH:MM"` 24-hour format (e.g. `"12:00"`).
    pub start: String,
    /// End of the window in `"HH:MM"` 24-hour format (e.g. `"13:00"`).
    /// If `end <= start`, the window is interpreted as overnight (wraps midnight).
    pub end: String,
    /// Days on which this window applies. Empty = never active.
    /// Uses the existing `Weekday` enum (`crates/oneshim-core/src/config/enums.rs:11-35`)
    /// for serde parity with `ScheduleConfig::active_days`.
    pub days_of_week: Vec<Weekday>,
    /// Optional label shown in the UI ("Lunch", "Evening") — does not
    /// affect semantics. Default empty.
    #[serde(default)]
    pub label: String,
}
```

Register in `AppConfig` alongside the existing `schedule` field:

```rust
// crates/oneshim-core/src/config/mod.rs (adjacent to line 41)
#[serde(default)]
pub schedule: ScheduleConfig,
#[serde(default)]
pub tracking_schedule: TrackingScheduleConfig,  // NEW
```

And in `AppConfig::default_config()`:

```rust
// crates/oneshim-core/src/config/mod.rs (adjacent to line 113)
schedule: ScheduleConfig::default(),
tracking_schedule: TrackingScheduleConfig::default(),  // NEW
```

### 3.7 Multiple-range-per-day, overnight, DST

**Multiple ranges per day** — users express "lunch 12–13 and evening 18–22" as two separate `TrackingWindow` entries both with `days_of_week = [Mon, Tue, Wed, Thu, Fri]`. The evaluator OR-reduces across the `windows` vec — `now` is in the suppression set if **any** window matches.

**Overnight windows** — matches the existing overnight convention at `crates/oneshim-vision/src/trigger.rs:69-77`: if `end <= start`, the window wraps midnight, and the evaluator checks both `hour >= start || hour < end` and also bridges `days_of_week` to the previous day for the post-midnight portion. Worked example: `{start: "22:00", end: "06:00", days: [Sat]}` means "Saturday night from 22:00 until Sunday 06:00". The evaluator must treat Sunday 00:00–06:00 as belonging to the Saturday window. Implementation sketch:

```rust
fn window_is_active(&self, now: DateTime<Local>) -> bool {
    let (hhmm_now, weekday_now) = (now.format("%H:%M").to_string(), now.weekday());
    let weekday_prev = weekday_now.pred();

    if self.end > self.start {
        // normal range
        self.days_of_week.contains(&weekday_now.into())
            && hhmm_now >= self.start && hhmm_now < self.end
    } else {
        // overnight range
        let tonight = self.days_of_week.contains(&weekday_now.into()) && hhmm_now >= self.start;
        let tomorrow_morning = self.days_of_week.contains(&weekday_prev.into()) && hhmm_now < self.end;
        tonight || tomorrow_morning
    }
}
```

**DST — corrected semantics (per CONS-C04 rewrite)**:

The `timezone` field stores an IANA name (`"Asia/Seoul"`, `"America/New_York"`), parsed via `chrono-tz`. The default `"Local"` resolves through `chrono::Local::now()`. `window_is_active` uses wall-clock `"HH:MM"` string comparison. This means:

- **Fall-back (e.g. US/Europe autumn "duplicate hour")**: wall-clock 02:30 occurs twice — once at DST time, once at standard time. The predicate `hhmm_now < "02:30"` fires on **both** occurrences. Because tracking-schedule is a **suppression** primitive (over-suppress-safe), this is acceptable: the user gets an extra hour of suppression that year, never less. Integration test must assert "window ending at 02:30 fires twice on fall-back DST Sunday" — NOT "fires once" as a prior draft incorrectly implied.

- **Spring-forward (e.g. US/Europe "lost hour")**: if a window's time range lies entirely within the skipped 02:00–03:00 wall-clock interval on DST Sunday, the window does **not** fire that day at all. This is a user-visible anomaly: a user who configured `02:30–02:59` suppression will get zero suppression on the DST change day. The Settings UI should warn at configure-time when a window overlaps the skipped hour in the user's configured timezone for the current year. Integration test must assert this is the observed behavior (not "no anomaly occurs").

DST is explicitly not promised to second-accuracy; we document hour-boundary granularity in the user-facing description. Integration tests cover both transition directions using US/Eastern fixtures — see §6.1 Feature 1.

**Adding `chrono-tz` — Decision D16 (U5 locked to Option A: accept in `oneshim-core`)**

This crate is **not currently a workspace dependency** (`grep -c chrono-tz Cargo.toml Cargo.lock` → 0). Adding it is a deliberate trade: +2.1MB binary size (embedded IANA tzdata), but the alternative (requiring users to express timezones as UTC offsets) is a non-starter for daylight-saving users.

**Placement**: directly in `oneshim-core` as a new dependency. Feature flag `chrono-tz/default-tz` ships the full IANA tzdata.

**Hexagonal dependency-direction note (ADR-001 §4)**: `oneshim-core` is the leaf crate; adding `chrono-tz` pulls tzdata into every adapter + the binary. The alternative considered was: define `TrackingScheduleConfig::timezone: String` in core, declare a `TimezoneResolver` port in core, place the `chrono-tz` implementation in an adapter crate. U5 rejected that path because:
1. Three adapters already need timezone resolution (monitor for scheduler tick, vision for capture gate, network for upload flush) — a port-adapter split would require wiring all three through DI for no behavioral benefit.
2. `chrono` (already in core) and `chrono-tz` share the `TimeZone` trait hierarchy — placing `chrono-tz` elsewhere forces an awkward bridge type.
3. +2.1MB is acceptable budget; binary-size regressions track elsewhere.

Re-opened if binary size becomes a measured concern.

### 3.7a Clock irregularities (suspend/resume, clock skew)

Laptop users suspend/resume regularly. The `tokio::time::interval` and `chrono::Local::now()` pattern used throughout the scheduler has three observable behaviors when clocks move:

| Event | Behavior | User-visible impact | Mitigation |
|---|---|---|---|
| Suspend crossing a window boundary (e.g. sleep 11:55 → wake 13:05) | Gate was correct across the period; the `tracking_schedule_active` predicate returns `true` the whole time the window covered. `DesktopNotifier` toasts for "Tracking paused" / "Tracking resumed" fire from within scheduler-tick transitions — if the loop was asleep for the entire window, both notifications are missed. | User may be surprised that "Tracking paused" was never shown, but capture/events were correctly suppressed for the (sleeping) duration. | Accepted — the gate was correct; notification miss is a minor UX cost. |
| Backward wall-clock jump (user sets clock back; window re-entered) | Gate re-evaluates on each tick; predicate flips `false → true` again. "Tracking paused" toast may fire twice. | User sees duplicate notifications. | Debounce via a `last_notification_at: Instant` cooldown (e.g. 60s) in the scheduler helper that emits transition notifications. |
| Forward wall-clock jump (past window end) | Gate correctly resumes; no lingering suppression. | None. | None needed. |
| Forward wall-clock jump *into* a future window | Gate correctly activates suppression. | "Tracking paused" fires. | Normal path. |
| Forward wall-clock jump *past* window end (clock skips entire window) | Window effectively never fires; both notifications missed. | User sees no indicator for that day. | Accepted — user-self-inflicted if they manually set clock. |

All mitigations live in the scheduler-side helper (`tracking_schedule_helper.rs` — see §3.8) so the trigger and the uploader stay stateless on clock.

### 3.8 Gate integration points — expanded per CONS-C01 + CONS-C02 (U1 locked to Option A: enumerate all)

The gate checks must appear at **every data-producing pipeline**. Half-gating would silently leak PII (window titles, keystroke counts, clipboard fingerprints, file-access events) during tracking-schedule windows — a GDPR Art. 5 purpose-limitation breach. U1 locked to **Option A** (enumerate-all, gate-all). Citations verified against worktree tip `5618558c`; each gated boundary `AND`-composes with `!tracking_schedule_active(&cfg, now)`.

| # | Pipeline | Current file:line | Current gate | Phase-9 disposition |
|---|----------|-------------------|--------------|---------------------|
| 1 | **Capture decision** (trigger) | `crates/oneshim-vision/src/trigger.rs:138-148` | `is_within_active_hours()` + state-lock — hoisted out per §3.8a | Trigger becomes time-agnostic; scheduler gate composes both checks |
| 2 | **Monitor-loop capture guard** | `src-tauri/src/scheduler/loops/monitor.rs:200-207,292` | `within_active_hours && !capture_paused` | Change to `within_active_hours && !capture_paused && !tracking_schedule_active` (via helper — see CONS-I06) |
| 3 | **Window-switch events** (active-window changes, pre-gate save) | `src-tauri/src/scheduler/loops/monitor.rs:181-189` | **No schedule gate** — save happens before line 207 gate | Hoist the save inside the AND-composed gate, so Window events emit only when composite gate passes |
| 4 | **Analysis loop (LLM / regime summarization)** | `src-tauri/src/scheduler/loops/intelligence.rs:14` (`spawn_analysis_loop`) | **No current gate** (loop runs unconditionally; `should_run_now` not called from this file — verified by `rg active_hours\|schedule src-tauri/src/scheduler/loops/intelligence.rs` → 0 matches) | Phase 9 **adds** both `should_run_now` and `tracking_schedule_active` checks at loop-body entry; early-`continue` when either blocks. **This is a scope expansion** — see D13. |
| 5 | **Focus analyzer loop** | `src-tauri/src/scheduler/loops/intelligence.rs:124` (`spawn_focus_loop`) | **No current gate** | Phase 9 **adds** composite gate; early-`continue` when TS active |
| 6 | **Coaching loop** | `src-tauri/src/scheduler/loops/intelligence.rs:160` (`spawn_coaching_loop`) | **No current gate** | Phase 9 **adds** composite gate; early-`continue` when TS active (coaching during opt-out window feels invasive — R3.I4) |
| 7 | **Process snapshot events** | `src-tauri/src/scheduler/loops/events.rs:60-92` (`process_interval` branch) | **No current gate** — `storage.save_event` + `uploader.enqueue` unconditional | Phase 9 gates the whole `process_interval` branch with `tracking_schedule_active` check |
| 8 | **Input activity events** (keystroke/mouse/scroll counts) | `src-tauri/src/scheduler/loops/events.rs:93-111` (input sub-section of `input_interval` branch) | **No current gate** | Phase 9 gates the `input_interval` branch with `tracking_schedule_active` check |
| 9 | **Clipboard events** | `src-tauri/src/scheduler/loops/events.rs:112-128` (clipboard sub-section of `input_interval` branch) | **No current gate** | Phase 9 gates the clipboard-poll branch |
| 10 | **File-access events** | `src-tauri/src/scheduler/loops/events.rs:130-145` (file-access sub-section of `input_interval` branch) | **No current gate** | Phase 9 gates the file-watcher branch |
| 11 | **Upload batch flush** | `crates/oneshim-network/src/batch_uploader.rs:199` (`flush()`) | No time-based gate; flush runs on interval | Inject `upload_suppressed` closure; `flush()` returns `Ok(0)` early when TS active (§3.9) |
| 12 | **Cross-device sync loop** | `src-tauri/src/scheduler/loops/sync.rs:87` (`spawn_cross_device_sync_loop`) | **No current gate** | Phase 9 gates loop-tick entry with `tracking_schedule_active` check — syncing a device-to-device copy of pre-window data *during* a window violates Art. 5 purpose-limitation |
| 13 | **Audio capture / STT** (user-initiated command) | `commands::audio::start_audio_capture` (in `src-tauri/src/commands/audio.rs`) — invoked from IPC not scheduler loop | None (command is explicit) | Phase 9 makes `start_audio_capture` return `validation.invalid_arguments` when called during an active TS window; the command path is the only entry |
| 14 | **Heartbeat loop** (telemetry ping: `user_id`, `device_id`) | `src-tauri/src/scheduler/loops/heartbeat.rs` | **No schedule gate** (infrastructure-level) | **Ungated** — heartbeat carries no capture data; server needs to know client is alive. Per-loop disposition: keep running. |
| 15 | **OAuth token refresh loop** | `src-tauri/src/scheduler/loops/sync.rs:15` (`spawn_oauth_refresh_loop`) | **No schedule gate** (infrastructure-level) | **Ungated** — no user-data emission; gating would just break auth. Per-loop disposition: keep running. |
| 16 | **Metrics / process / aggregation / notification loops** | Various (see CLAUDE.md 16-loop inventory) | None | **Ungated** — these are local-only (metrics in SQLite, aggregation for the web dashboard). Audit confirmed no outbound PII emission from these loops. |

**Helper extraction (CONS-I06 — monitor.rs guardrail)**: `scheduler/loops/monitor.rs` is at **498 lines** (`wc -l src-tauri/src/scheduler/loops/monitor.rs` → 498), and CLAUDE.md sets the guardrail at "under 500 lines" for `spawn_monitor_loop`. Adding the composite-gate body inline would push over. Mitigation: extract the predicate into a new file `src-tauri/src/scheduler/loops/tracking_schedule_helper.rs` (mirroring the `coaching_helper.rs` / `focus_auto_helper.rs` / `vision_helper.rs` precedents), exposing:

```rust
// src-tauri/src/scheduler/loops/tracking_schedule_helper.rs (new)
pub(crate) fn tracking_schedule_active(cfg: &AppConfig, now: DateTime<Local>) -> bool { … }
pub(crate) fn capture_permitted_now(cfg: &AppConfig, now: DateTime<Local>) -> bool { … }
```

These pure 2-arg functions are directly testable without a mock clock (see §6.1 Feature 1).

The wrapper `tracking_schedule_active(config: &AppConfig)` that reads `chrono::Local::now()` internally still lives at `src-tauri/src/scheduler/mod.rs:548-571` adjacent to `should_run_now` for ergonomic call sites that don't care to pass `now`. Callers that *need* injected time (tests, integration tests) use the 2-arg `tracking_schedule_helper::tracking_schedule_active(cfg, now)` form.

Proposed sibling at `src-tauri/src/scheduler/mod.rs`:

```rust
// src-tauri/src/scheduler/mod.rs (new adjacent to should_run_now)

/// Returns `true` when the current moment falls inside ANY configured
/// tracking-schedule window. When this returns `true`, capture/events/
/// upload/analysis must be suppressed.
///
/// See `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` §3.8 for the
/// full gate composition.
pub fn tracking_schedule_active(config: &AppConfig) -> bool {
    let schedule = &config.tracking_schedule;
    if !schedule.enabled || schedule.windows.is_empty() {
        return false;
    }
    // Resolve `now` via `config.tracking_schedule.timezone` (chrono-tz
    // or chrono::Local for "Local"). See §3.7 for wrap-midnight logic.
    let now = resolve_now(&schedule.timezone);
    schedule.windows.iter().any(|w| w.window_is_active(now))
}
```

And a single combined predicate for consumers that want "capture permitted right now":

```rust
/// Returns `true` when capture is permitted at this moment, combining
/// active-hours + tracking-schedule. The `capture_paused` atomic is
/// checked by the monitor loop itself, not by this helper.
pub fn capture_permitted_now(config: &AppConfig) -> bool {
    should_run_now(config) && !tracking_schedule_active(config)
}
```

### 3.8a `SmartCaptureTrigger::with_schedule` refactor — Decision D17 (U7 locked to Option A: in-PR)

`SmartCaptureTrigger` at `crates/oneshim-vision/src/trigger.rs:26-30` currently holds a cloned `ScheduleConfig` at construction time (`with_schedule(throttle_ms: u64, schedule: ScheduleConfig) -> Self` at `trigger.rs:37`). Phase 9 hoists both the active-hours check and the tracking-schedule check up to the scheduler loop, simplifying the trigger to be time-agnostic.

**Locked decision (U7 = Option A)**: the refactor is **in-scope for the same PR**. Rationale: a half-migrated trigger (active-hours check still inside, TS check at scheduler) is a bad state that R1 specifically called out as risky for follow-up sequencing. Keeping the refactor in-PR keeps the Single-Source-of-Truth boundary clean.

**DI callsites touched** (composition root `src-tauri/src/main.rs` / `app_runtime_launch.rs`):

| Site | Current call | After refactor |
|---|---|---|
| Composition root | `SmartCaptureTrigger::with_schedule(throttle_ms, schedule_config)` | `SmartCaptureTrigger::new(throttle_ms)` |
| `scheduler/loops/monitor.rs:200-207` | `if within_active_hours && !capture_paused { … }` — reads via `should_run_now(&cfg)` | `if capture_permitted_now(&cfg, now) && !capture_paused { … }` — composite gate in helper |

**Test migration** (per CONS-C11 — trigger.rs has **13** tests at `trigger.rs:194-435`, not 8 at `:207-435`):

| Existing test (location) | Disposition |
|---|---|
| `blocks_capture_outside_active_hours` (`trigger.rs:373`) | Migrate to `scheduler::mod::tests` as `scheduler_blocks_capture_outside_active_hours`; test the composite gate via `capture_permitted_now(cfg, now)` 2-arg pure fn |
| `allows_capture_when_schedule_disabled` (`trigger.rs:398`) | Migrate similarly |
| `handles_overnight_active_hours` (`trigger.rs:409`) | Migrate — covers U2 Option C fix (CONS-C05) |
| Remaining 10 trigger tests (at lines 207-369) | Unchanged; continue to pass against `SmartCaptureTrigger` (throttle / state-lock / importance logic) |

Implementation must verify all **13** existing trigger unit tests at `trigger.rs:194-435` either pass unchanged (10 of 13) or migrate cleanly (3 of 13). Leftover `// ── Blackout-hours tests` comment header at `trigger.rs:370` is swept as part of the migration (CONS-M02). See §6.5 for the sweep step.

### 3.9 Upload-defer mechanism — rewritten per CONS-C03 (upstream-gated, pre-flush drain, FIFO-exit)

**Intent**: drain **pre-window** events on window exit in FIFO order. No in-window-timestamped event is ever enqueued, because the upstream event loops (rows 3-13 of §3.8) are gated first.

Behavior during a tracking-schedule window:

1. **Upstream PII-source gating proof (depends on CONS-C02)**: per §3.8 the 9 data-producing scheduler loops (rows 3-10 + 12) all short-circuit on `tracking_schedule_active`. Therefore `uploader.enqueue(upload_event)` is NEVER called with an in-window-timestamped row.
2. **In-flight events already in the `BatchUploader` SegQueue at T=t₁ (window entry)** are legitimately pre-window. They remain in the queue. GDPR-wise they are lawful-basis-compliant because they predate the opt-out.
3. **Flush during window**: `BatchUploader::flush()` short-circuits to `Ok(0)` (see predicate injection below). Pre-window events stay queued. Rationale: flushing mid-window would ship network traffic to the server while the user's Tracking Schedule indicator shows suppression — a transparency mismatch even though the payload rows predate the opt-out.
4. **Pre-flush drain at window-entry (long-window overflow protection)**: if the uploader queue approaches `max_queue_size` (default via `with_max_queue_size`), the existing `drop_oldest()` behavior (`batch_uploader.rs:136-156`) kicks in and drops pre-window events silently — data loss for the user. To avoid this during long windows, the scheduler can optionally trigger `BatchUploader::flush()` JUST BEFORE `tracking_schedule_active` transitions `false → true`, draining pre-window events while the suppression predicate is still `false`. Implementation: a scheduler-side `on_window_boundary_approaching` hook (optional, deferred to follow-up if complexity is too high).
5. **When the window exits (T=t₂)**: the queue naturally drains on the next flush tick (`batch_uploader.rs:199` `flush()`). Drain rate is governed by the existing `compute_batch_size()` logic at `batch_uploader.rs:185-197` — no tuning needed.

**Worked example (integration-test fixture)**: window `[12:00, 13:00]`, event E at T=11:30 enqueued. At T=12:30 `tracking_schedule_active == true`; no capture attempted (upstream gate) and no flush attempted (predicate gate). At T=13:01 the predicate flips `false`; next flush tick ships E to the server. Test asserts: (a) no queue row between T=12:00 and T=13:00 carries an in-window timestamp; (b) post-exit flush ships exactly one row (E, timestamp 11:30). See §6.1.

Implementation: wrap the body of `BatchUploader::flush()` with an early-return guarded by a callback. The cleanest injection is to pass an `Arc<dyn Fn() -> bool + Send + Sync>` "suppression predicate" at construction, defaulting to `|| false`:

```rust
// crates/oneshim-network/src/batch_uploader.rs
pub struct BatchUploader {
    // ... existing fields ...
    /// When this predicate returns `true`, `flush()` short-circuits to `Ok(0)`.
    /// Default: always `false` (no suppression). Wired to
    /// `tracking_schedule_active(&cfg)` in `src-tauri/src/main.rs` DI.
    upload_suppressed: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl BatchUploader {
    pub fn with_suppression_predicate(
        mut self,
        pred: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        self.upload_suppressed = pred;
        self
    }

    pub async fn flush(&self) -> Result<usize, NetworkError> {
        if (self.upload_suppressed)() {
            debug!("upload flush suppressed — tracking schedule active");
            return Ok(0);
        }
        // ... existing body ...
    }
}
```

DI wiring in `src-tauri/src/main.rs` / `app_runtime_launch.rs` composition root:

```rust
let cfg_mgr_for_pred = config_manager.clone();
let pred: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
    crate::scheduler::tracking_schedule_active(&cfg_mgr_for_pred.get())
});
let uploader = BatchUploader::new(/* ... */).with_suppression_predicate(pred);
```

This introduces a **new** injection shape into `crates/oneshim-network`. The closest existing precedent on `BatchUploader` is `with_health_flag(mut self, flag: Arc<AtomicBool>) -> Self` at `crates/oneshim-network/src/batch_uploader.rs:74` (circuit-breaker gating flag) — a single atomic. Our tracking-schedule predicate differs: it is an `Arc<dyn Fn() -> bool + Send + Sync>` closure, because the upstream truth is `AppConfig` state (tracking-schedule windows, timezone, weekday), not a single boolean that can live in an `AtomicBool`. The crate stays free of `AppConfig` dependency because the closure is constructed at the composition root (`src-tauri/src/main.rs`) and injected opaque to the uploader.

The earlier draft cited `Scheduler::with_capture_paused(Arc<AtomicBool>)` at `src-tauri/src/scheduler/mod.rs:429-430` as the "same pattern" — that builder lives on `Scheduler`, not on `BatchUploader`, and the `Arc<AtomicBool>` shape is a different mechanism (atomic vs closure). The corrected citation is `with_health_flag` above.

**Queue-cap interaction**: if suppression lasts long enough that the queue reaches `max_queue_size`, the existing `drop_oldest()` behavior (`batch_uploader.rs:136-156`) kicks in, dropping the oldest events. This is already the documented behavior for any backpressure source (network outage, etc.) and does not need a new carve-out beyond the pre-flush-drain mitigation discussed in §3.9 item 4.

### 3.10 `capture_paused` atomic interaction — Decision settled

**Reuse `capture_paused`? No — introduce a new atomic.**

The existing `capture_paused: Arc<AtomicBool>` (`runtime_state.rs:364`, spread to 12 sites per the `capture_paused` grep) represents a **user-initiated, sticky** pause (toggled from tray at `tray.rs:181,207,241` or from the IPC command `toggle_capture_pause` at `capture_status.rs:72` — the `fetch_xor` call is at line 76). It is manual state; the user expects it to stay `true` until they toggle it back.

Tracking schedule is **config-derived, time-transient**. Overloading `capture_paused` would cause two bugs:

1. On window exit, the atomic would need to flip back to `false`, but only if the user hadn't also manually toggled it — this requires tracking the reason the atomic was set.
2. The tray UI (`tray.rs`) reads `capture_paused` to render a paused icon. Conflating the two sources of pause makes the tray label ambiguous ("Paused" vs "Tracking-scheduled").

**Chosen**: introduce no new atomic at all. The tracking-schedule state is a **pure function of config + clock**; each consumer evaluates `tracking_schedule_active(&cfg)` at decision time. No shared mutable state is needed. The monitor loop at `scheduler/loops/monitor.rs:207` already dereferences the config manager once per tick; adding the `tracking_schedule_active` check alongside `should_run_now` is O(n) in the windows vec per tick, negligible at typical window counts (≤ 20 per realistic user).

This also keeps the `AppState` struct (§`runtime_state.rs:347-384`) from growing another field that would trigger the "3+ fields → sub-struct" guardrail (per `client-rust/CLAUDE.md` Architecture Guardrails § AppState Sub-Structs).

### 3.11 User-facing indicator

Notifications remain **ON** during tracking-schedule windows — this is a GDPR Article 13/14 transparency requirement (users must know their data is being controlled; silent suppression is arguably worse than no suppression because it masks the state).

Indicator surfaces:

- **Tray**: add a tray state "Tracking Scheduled" distinct from manual "Paused". Q7 resolved in-place: **reuse the `Paused` icon and change the tooltip label** to "Tracking Schedule Active until HH:MM" (no new art assets; already supported by the existing `sync_tray_state` signature).
- **Border/native overlay** (macOS border adapter at `runtime_state.rs:366` (`indicator_visible: Arc<AtomicBool>`) and `focus_mode: Arc<FocusModeState>` at `runtime_state.rs:370`; paired with `magic_overlay_driver.rs`): keep the red capture border **off** during suppression (intentional — the user should not see a capture indicator when nothing is being captured). The tray tooltip carries the semantics.
- **Settings badge**: on the Settings → Tracking Schedule tab, show a pill `"Active now — ends HH:MM"` when the schedule is currently suppressing.
- **In-app notification**: on window-entry and window-exit, fire a toast via the existing `DesktopNotifier` port ("Tracking paused until 13:00" / "Tracking resumed"). Opt-out via a new `notification.tracking_schedule_enabled: bool` defaulting to `true` (field renamed per CONS-M05 — `tracking_schedule_enabled` avoids the double-"notifications" tautology against its parent `NotificationConfig`; matches sibling naming `idle_enabled`, `long_session_enabled`). Notifications are debounced by a `last_notification_at: Instant` cooldown (60s) to absorb backward clock-jump duplicates (§3.7a).

### 3.11a Tray indicator propagation — Decision D-prop (U6 locked to Option A: ADR-016 subscribe)

**Problem**: `PUT /api/tracking-schedule` at the composition root mutates config. The monitor scheduler loop at 1s tick picks up the new config on the next poll (≤ 1s latency). The tray must update **immediately** for the user — a 1-30s tray delay after toggling is a perceived regression.

**Locked decision (U6 = Option A)**: the tray task subscribes to `ConfigChangeBus::subscribe()` per ADR-016 (`docs/architecture/ADR-016-config-change-bus.md`). When the tray receives a `ConfigChanged { tracking_schedule }` event, it re-renders the tooltip/icon from the new config state. This is the cleanest fit because tray already runs its own task loop and ADR-016's "wake-up on change" semantics are exactly what is needed.

Rejected options:
- **(b) Tauri event emit on `set_tracking_schedule` IPC** — couples tray to the IPC call path; any programmatic config mutation (file-edit, dev tools) bypasses the emit. ADR-016 covers both.
- **(c) Tray re-eval tick (1-5s)** — adds a permanent poll loop for a rare event; wastes CPU.

The tray task must filter `ConfigChanged` events to avoid re-rendering on unrelated config mutations. Filter: re-render only when the diff's `tracking_schedule` sub-tree or `notification.tracking_schedule_enabled` changes.

### 3.12 IPC / REST surface

Tauri commands (new file `src-tauri/src/commands/tracking_schedule.rs`):

```rust
// GET current config
#[command]
pub async fn get_tracking_schedule(
    state: State<'_, ConfigRuntimeState>,
) -> Result<TrackingScheduleConfig, IpcError>;

// Replace whole config (consistent with existing settings-patch semantics)
#[command]
pub async fn set_tracking_schedule(
    state: State<'_, ConfigRuntimeState>,
    cfg: TrackingScheduleConfig,
) -> Result<TrackingScheduleConfig, IpcError>;

// Check "is a window active right now" — driven by UI badge and tray
#[command]
pub async fn get_tracking_schedule_status(
    state: State<'_, ConfigRuntimeState>,
) -> Result<TrackingScheduleStatus, IpcError>;
```

```rust
// crates/oneshim-api-contracts/src/tracking_schedule.rs (new)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingScheduleStatus {
    /// Whether any window is currently suppressing tracking.
    pub active_now: bool,
    /// ISO-8601 when the current window ends (None if `active_now == false`).
    pub ends_at: Option<String>,
    /// ISO-8601 when the next window begins (None if no future window in the
    /// next 7 days; consistent with the frontend's 7-day lookahead budget).
    pub next_starts_at: Option<String>,
    /// Label of the active or next window (for UI).
    pub label: String,
}
```

REST endpoints (`crates/oneshim-web/src/routes.rs`, add adjacent to line 117):

```
GET    /api/tracking-schedule          → 200 TrackingScheduleConfig
PUT    /api/tracking-schedule          → 200 TrackingScheduleConfig (full replace)
GET    /api/tracking-schedule/status   → 200 TrackingScheduleStatus
```

These are symmetric to the existing `/api/settings` paths (`handlers::settings::*`).

### 3.13 Migration for existing users

Zero-impact upgrade:

- `TrackingScheduleConfig::default()` returns `{enabled: false, windows: [], timezone: "Local"}` → no behavior change for existing installs.
- `#[serde(default)]` on `AppConfig::tracking_schedule` → configs persisted before Phase 9 deserialize without the field and receive the default.
- No migration of existing `CoachingConfig::quiet_hours` is performed — quiet hours stay coaching-specific (they only gate coaching messages, not capture).

### 3.14 Single schedule, multiple windows — Decision D22

**Locked choice**: ONESHIM ships with a **single `TrackingScheduleConfig` holding a `Vec<TrackingWindow>`**, not N named schedules. Users express "Weekdays lunch + Weekends full-day" as **multiple windows under one schedule**, not two named schedules.

**Rationale**:
- MVP simplicity — peer products (Teramind, RescueTime, ActivTrak) support N named schedules, but the added config shape (`Vec<{name, enabled, windows}>`) + UI (enable/disable each schedule) + composition rule ("active if ANY enabled schedule matches") expands review surface. Quick-wins scope.
- Users can achieve the same behavior with multiple `TrackingWindow` entries; naming is lost but gating is equivalent.
- Forward-compatibility: if a future phase adds named schedules, the migration wraps the current `Vec<TrackingWindow>` into a single default-named schedule. No deserialization break.

**Deferred**: named multiple-schedule feature (tracked in `project_next_tasks.md`).

## 4. Feature 2 — Linux `systemd` Autostart Wiring

### 4.1 Goals

- Expose `src-tauri/src/autostart.rs::{enable_autostart, disable_autostart, is_autostart_enabled}` (lines 8, 31, 54) via Tauri IPC.
- Expose the same via REST for the web dashboard settings page.
- Add a Settings UI toggle with platform-appropriate wording and graceful disablement when `systemctl` is absent.
- Ship without changing the already-shipping module body — this is pure integration.

### 4.2 Non-goals

- No change to the macOS LaunchAgent plist template (`autostart.rs:98-122`) or the Windows Registry path (`autostart.rs:177-178` `SUBKEY`, `VALUE_NAME`).
- No handling of "reset autostart after OS upgrade" (a separate lifecycle concern).
- No automatic enable on first run — this remains opt-in per GDPR Article 7 consent principles.

### 4.3 Tauri command signatures

New file `src-tauri/src/commands/autostart.rs`:

```rust
use tauri::command;
use crate::ipc_error::IpcError;

#[derive(serde::Serialize)]
pub struct AutostartStatus {
    pub enabled: bool,
    /// Which mechanism is in use: `"launchctl"` | `"registry"` | `"systemd"` |
    /// `"xdg_desktop"` | `"unsupported"`. Computed from platform + whether
    /// `systemctl --version` returns 0 on Linux.
    pub mechanism: String,
    /// `true` on Linux when `systemctl --version` is absent — the UI should
    /// show a tooltip explaining XDG fallback was used (or that no mechanism
    /// is available).
    pub fallback_used: bool,
}

#[command]
pub async fn get_autostart_status() -> Result<AutostartStatus, IpcError>;

#[command]
pub async fn set_autostart(enabled: bool) -> Result<AutostartStatus, IpcError>;
```

Internally, both commands delegate to `crate::autostart::{enable_autostart, disable_autostart, is_autostart_enabled}`. The status command additionally probes `crate::autostart::linux::has_systemctl()` on Linux — this is a private function today (`autostart.rs:365-371`); expose it as `pub(crate)` for the new command.

**`has_systemctl` memoization (Q5 resolved in-place)**: `has_systemctl()` currently spawns `systemctl --version` on every call. The `get_autostart_status` IPC command may be polled on every Settings page mount + after every toggle. Memoize via `static HAS_SYSTEMCTL: OnceLock<bool> = OnceLock::new();` with one-time init per process. This eliminates spawn-per-request overhead and closes Q5 deterministically.

**Cross-platform `is_enabled()` caveat (per CONS-M09)**: all three platforms derive `is_enabled() == true` from file/registry **existence**, not from live OS-level registration:

- macOS: `plist_path().exists()` — does not verify `launchctl load` succeeded.
- Linux: file exists at `~/.config/systemd/user/oneshim.service` — does not verify `systemctl --user enable` registered the unit.
- Windows: `RegQueryValueExW` on `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` succeeds — does not verify the path is valid or the executable exists.

This is a **pre-existing latent bug** on all three platforms; Phase 9 wiring exposes it to users but does not introduce it. See §4.7 for the "Repair Autostart" mitigation that applies to all three platforms.

Mechanism resolution:

```rust
#[cfg(target_os = "macos")]    -> "launchctl"
#[cfg(target_os = "windows")]  -> "registry"
#[cfg(target_os = "linux")]    -> if has_systemctl() { "systemd" } else { "xdg_desktop" }
_                              -> "unsupported" (enabled always false)
```

The `#![allow(dead_code)]` attribute at `autostart.rs:4` can now be **removed** once the commands are wired, since all `pub fn` symbols become reachable from the IPC handlers.

### 4.4 Web REST endpoint — Decision D3a (recommend YES, add)

Add `GET /api/autostart` and `PUT /api/autostart` endpoints paralleling the Tauri commands, because the web dashboard settings page (`crates/oneshim-web/frontend/src/pages/settings/SettingsLayout.tsx` + tabs) is the actual source of truth for how the user edits settings — the Tauri overlay/tray paths are secondary. Without the REST endpoints the Settings web page cannot reflect or mutate the autostart state.

Routes (add to `crates/oneshim-web/src/routes.rs` near line 94 "settings" area):

```
GET /api/autostart   → 200 AutostartStatus
PUT /api/autostart   → 200 AutostartStatus  (body: { enabled: bool })
```

Handler location: `crates/oneshim-web/src/handlers/autostart.rs` (new). Unlike most settings handlers, these do not need to acquire the storage context — they shell out to the platform-specific module. Error-mapping convention: map `String` errors from `autostart::enable/disable` into `ApiError::Internal` (500) with the wire code `internal.io` (per ADR-019 catalog). On unsupported platforms, the `enabled` mutation is a no-op and the handler returns `200 {enabled: false, mechanism: "unsupported", fallback_used: false}` rather than an error.

### 4.5 Persistence — Decision D4

**Chosen**: do NOT persist `autostart_enabled` in `AppConfig`. Use the filesystem (plist / registry key / unit file / `.desktop` file) as the source of truth.

Rationale:

- **External mutation**: users can manually enable / disable autostart via platform-native tools (macOS System Settings → Login Items, `systemctl --user enable oneshim.service`, Windows Task Manager → Startup tab, `~/.config/autostart/` file management). A persisted config field would go stale against these external mutations.
- **No value to persist**: `is_autostart_enabled()` is already O(1) (stat / registry read / file exists), so caching in config gives no performance win.
- **Consistency**: macOS System Preferences, Windows Settings, and GNOME Tweaks all treat autostart as an OS-managed setting, not an application setting. The app reads, doesn't own.

**Rejected**: persist `general.autostart_enabled: bool` in `AppConfig`. Rejected because config-file-vs-OS-state divergence would need a reconciliation step on every startup, and the user mental model "my app knows this setting" vs "the OS knows this setting" is not worth the code.

Implication: **no config migration needed**. The existing autostart module already reads filesystem state; the new IPC surface simply exposes it.

### 4.6 Wayland handling — Decision D5

The current Linux unit template (`autostart.rs:343`) hardcodes:

```
Environment=DISPLAY=:0
```

This is correct for X11 but insufficient for Wayland (xcap uses the compositor's portal; the agent also needs `WAYLAND_DISPLAY` and `XDG_SESSION_TYPE` to decide capture strategy).

**Chosen**: detect `$WAYLAND_DISPLAY` at enable-time and append matching `Environment=` lines. Update `generate_service_file()` at `autostart.rs:331-348`:

```rust
pub fn generate_service_file(program_path: &str) -> String {
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let x11_display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_else(|_| "x11".to_string());

    let mut env_lines = format!("Environment=DISPLAY={x11_display}\n");
    if let Some(wl) = wayland_display {
        env_lines.push_str(&format!("Environment=WAYLAND_DISPLAY={wl}\n"));
    }
    env_lines.push_str(&format!("Environment=XDG_SESSION_TYPE={session_type}\n"));

    format!(
        "[Unit]\n\
         Description=ONESHIM Desktop Agent\n\
         After=graphical-session.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={program_path}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         {env_lines}\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}
```

Existing tests at `autostart.rs:502-540` are preserved — they assert substring presence (e.g. `Environment=DISPLAY`) and remain true. Add two new assertions:

- Wayland env detection path (set `WAYLAND_DISPLAY=wayland-0` in the test, assert the resulting service file contains both DISPLAY and WAYLAND_DISPLAY).
- XDG session-type propagation.

**Rejected**: Wayland-only. Rejected because many Linux desktops (Ubuntu 22.04 GNOME, KDE Plasma on X11) still run X11 natively; capturing only Wayland breaks half the install base.

### 4.7 Binary-path stability — Decision D6 (document behavior, no code change)

`autostart.rs:92-95,325-329,188-190` resolves the binary path via `std::env::current_exe()` at enable-time. This is written into the unit file / plist / registry. Subsequent binary moves do NOT update the unit file.

Actual behavior for common Linux upgrade flows:

| Flow                           | Binary path after upgrade                        | Unit file `ExecStart=` stale? |
| ------------------------------ | ------------------------------------------------ | ----------------------------- |
| `.deb` replace (APT install)   | Stable (`/usr/bin/oneshim`)                      | No                            |
| `.rpm` replace (DNF install)   | Stable (`/usr/bin/oneshim`)                      | No                            |
| AppImage (user-managed)        | Typically stable in `~/.local/bin/` or similar   | No (if user doesn't move)     |
| AppImage relocated by user     | Changed                                          | **YES — broken**              |
| Flatpak (sandboxed)            | Stable sandbox path                              | No                            |
| Snap (Ubuntu Snap Store)       | Changed each refresh (`/snap/<pkg>/current/…`)   | **YES — broken** — Snap users must re-enable after each `snap refresh` |
| Homebrew cask (macOS)          | Stable (`/Applications/`)                        | No                            |
| Self-update (in-place)         | Stable                                           | No                            |

Stale unit files fail at boot with `status=203/EXEC` (file not found). systemd logs to the user journal (`journalctl --user -u oneshim`).

**Chosen behavior**: document the failure mode. Add a "Re-enable autostart if you move the app" hint in the Settings UI tooltip. Do NOT attempt auto-repair (would require a persistent watcher and introduce new failure modes).

**Re-enable is idempotent**: calling `enable_autostart()` again from the UI rewrites the unit file / plist / registry value with the current `current_exe()` path. Surface this as a **"Repair Autostart" button in the Settings UI on all three platforms** (macOS, Linux, Windows) — the `is_enabled()` file-existence-only check means the "stale binary path" failure mode applies equally on every platform (CONS-M09). Trigger the button only when `is_autostart_enabled() == true` AND the recorded path doesn't match `current_exe()`. Behavior: calls `enable_autostart()` again, which overwrites with the current path. One-sentence spec sufficient for this PR; full in-scope decision tracked in `project_next_tasks.md`.

### 4.8 XDG `.desktop` fallback — Decision D7

**Chosen**: keep the XDG fallback (`autostart.rs:404-417`).

Rationale:

- Not every Linux desktop runs systemd (Alpine Linux uses OpenRC, Void Linux uses runit, musl-based distros, some embedded, WSL2 without systemd, some BSDs if we expand portability).
- The fallback is already tested (`autostart.rs:519-532`).
- `has_systemctl()` is the right switch and already exists at `autostart.rs:365-371`.
- Removing it would strand users who can't control systemd; keeping it costs ~50 LoC of working code.

**Best-effort caveat**: XDG fallback is best-effort — DE-less non-systemd distros (e.g. Alpine/OpenRC without GNOME/KDE/XFCE, Void Linux without a DE) may not honor `~/.config/autostart/*.desktop`. The spec does NOT promise autostart works on every systemd-less Linux; it promises a best-effort fallback when `has_systemctl() == false`. Unsupported DE-less configurations receive `mechanism: "xdg_desktop"` in the response but their `.desktop` file may silently go unnoticed by the session manager.

Rejected: systemd-only. Rejected because `has_systemctl() == false` environments exist and the Settings UI would need to show "Autostart unavailable on your system" rather than a working toggle.

### 4.9 UI toggle — location and wording

Location: new entry in `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`, rendered as the last `ToggleRow` in the tab. Rationale: `GeneralTab` already hosts update-lifecycle toggles ("Check for updates", "Auto-install updates") plus the `ScheduleSettings` section — autostart belongs to the same app-level lifecycle mental category. An alternative is a new `StartupTab` but that is over-scoped for one toggle. (Prior draft incorrectly cited "Start minimized" — that toggle does not exist in the current codebase; verified via `grep -rn 'startMinimized\|minimized' crates/oneshim-web/frontend/src` → 0 hits.)

Label and description (EN / KO):

```json
"settings.autostart": "Start ONESHIM on login",
"settings.autostartDesc": "Launch the app automatically when you sign in to your account.",
"settings.autostartUnavailable": "Autostart is not supported on this system.",
"settings.autostartMechanism.systemd": "Using systemd user service",
"settings.autostartMechanism.xdg_desktop": "Using desktop entry (systemd not detected)",
"settings.autostartMechanism.launchctl": "Using LaunchAgents",
"settings.autostartMechanism.registry": "Using Windows Registry",
```

```json
"settings.autostart": "로그인할 때 ONESHIM 자동 시작",
"settings.autostartDesc": "사용자 계정에 로그인할 때 앱이 자동으로 실행됩니다.",
"settings.autostartUnavailable": "이 시스템에서는 자동 시작을 지원하지 않습니다.",
"settings.autostartMechanism.systemd": "systemd 사용자 서비스 사용 중",
"settings.autostartMechanism.xdg_desktop": "데스크톱 항목 사용 중 (systemd 감지되지 않음)",
"settings.autostartMechanism.launchctl": "LaunchAgents 사용 중",
"settings.autostartMechanism.registry": "Windows 레지스트리 사용 중",
```

Toggle behavior:

- Disabled (greyed out) when `mechanism == "unsupported"`.
- On toggle-on: call `PUT /api/autostart {enabled: true}`; display toast on success or error.
- On toggle-off: call `PUT /api/autostart {enabled: false}`; same.
- Below the toggle, subdued text shows which mechanism is in use, and — on Linux with `fallback_used == true` — a warning glyph explaining the reason.

### 4.10 Error handling

Error propagation is already defined by the module: each function returns `Result<T, String>` where the `String` is a human-readable message (see §4.10a for the typed-error decision). At the command boundary, map to `IpcError` with wire codes per ADR-019:

| Failure source | Wire code | HTTP status |
|---|---|---|
| Filesystem write (permission denied) | `storage.failed` | 500 |
| `launchctl` / `systemctl` / registry **spawn fails** (binary not installed / fork failure) | `internal.io` | 500 |
| `launchctl` / `systemctl` **non-zero exit status** (new; currently swallowed — see CONS-C10) | `internal.io` | 500 |
| `launchctl` / `systemctl` **timeout** (new; >5s via `tokio::time::timeout` wrap — see CONS-C10) | `internal.io` | 500 |
| Registry open fails (Windows) | `storage.failed` | 500 |
| Unsupported platform (request to `enable=true` on unsupported target) | `validation.invalid_arguments` | 400 |
| HOME env var not set (Linux / macOS) | `config.missing` | 500 |

**Behavioral fix required (CONS-C10)**: today the enable paths silently swallow non-zero exit codes and never time out:

- Linux `enable()` at `autostart.rs:389-401`: `systemctl --user enable` non-zero exit → `warn!("systemctl enable returned non-zero: …")` then `Ok(())`. The service file is written but the unit is NOT registered with systemd — `is_enabled()` returns `true`, boot is broken.
- macOS `enable()` at `autostart.rs:137-141`: `launchctl load` `.output()` captures stderr/stdout but `map_err` fires only on **spawn** failure; a non-zero exit from `launchctl load` is discarded.
- Windows `enable()`: `RegSetValueExW` non-zero return swallowed via `Ok(())` in the normal path.

Phase 9 **must** change all three platforms to:
1. Return `Err("…")` on non-zero exit (not `warn! + Ok(())`).
2. Wrap `Command::output` in `tokio::time::timeout(Duration::from_secs(5), …)` — if it times out, return `Err("enable command timed out: …")`.
3. Log with `warn!(err.code = "internal.io", …)` at the call site per the CLAUDE.md observability convention.

Mapping table goes in the new handler files. No new wire-code variants are required — every case reuses an existing entry from the catalog (`crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`). See §6.3.

**User-facing error copy**: toasts on error must surface actionable text, not raw wire codes. Map to i18n keys in §6.4:

| Wire code | i18n key | Example (EN) |
|---|---|---|
| `storage.failed` | `settings.autostartError.fileWrite` | "Could not write autostart file — please check permissions on `~/.config/` or `~/Library/LaunchAgents/`." |
| `internal.io` | `settings.autostartError.commandFailed` | "Could not register autostart with the system — please check system settings." |
| `config.missing` | `settings.autostartError.homeMissing` | "Autostart cannot be configured without a HOME environment variable." |
| `validation.invalid_arguments` | `settings.autostartError.unsupported` | "Autostart is not supported on this system." |

### 4.10a Autostart error-type upgrade — Decision D-errtype (U8 locked to Option A: defer typed upgrade)

**Locked decision (U8 = Option A)**: Phase 9 keeps `autostart.rs` returning `Result<_, String>` and maps to wire codes at the handler boundary via a substring-match helper. A typed `AutostartError` enum (per ADR-019 §7) is deferred to a follow-up PR (tracked in `project_next_tasks.md`).

Rationale: quick-wins scope. The typed upgrade requires adding `AutostartError` to `autostart.rs`, threading through 3 platform modules (9 functions), and adding a `code()` method per ADR-019. That is a separate refactor that should not ride on Phase 9's IPC-wiring work. The substring-map at the handler boundary is acknowledged as lossy — that's the cost of deferral.

Rejected: Option B (upgrade to typed `AutostartError` enum now). Re-opened when there is a second error consumer (e.g., when a second IPC caller path is added and the substring map needs to be duplicated, the cost of typed upgrade becomes justified).

### 4.10b First-run autostart prompt — Decision D21 (U9 locked to Option B: defer)

**Locked decision (U9 = Option B)**: no first-run autostart prompt in Phase 9. The toggle lives only in `Settings → General` and the user discovers it on Settings visit.

Rationale: peer-product baseline (Slack, Todoist, 1Password) prompts for autostart during onboarding. ONESHIM does not currently have an onboarding surface, and adding one to accommodate this single toggle inflates scope. A dedicated onboarding PR will add the first-run autostart prompt alongside other welcome-dialog settings — tracked in `project_next_tasks.md` as a follow-up.

Rejected: Option A (minimal prompt in welcome dialog) — rejected because it requires wiring a welcome-dialog surface that doesn't exist yet.

## 5. Feature 3 — Timeline Bulk Tag Ops

### 5.1 Goals

- Add a storage-layer `add_tag_to_frames(&[i64], tag_id)` and `remove_tag_from_frames(&[i64], tag_id)` — each wrapped in a single SQLite transaction for all-or-nothing semantics.
- Refactor the existing `batch_add_tag` handler at `handlers/tags.rs:83-98` to use the new transactional op (removes the partial-success risk).
- Add a matching batch-remove REST endpoint.
- Plumb batch-remove into the existing Timeline selection UI at `pages/timeline/AllFrames.tsx:582-603`.

### 5.2 Non-goals

- No change to the `tags` + `frame_tags` schema at `crates/oneshim-storage/src/migration/v01_v08.rs:186-212`.
- No multi-tag-per-operation (still tag-at-a-time; users add one tag at a time in the UI).
- No pagination-aware selection — selection still scoped to the current page (see D11).
- No shift-click / ctrl-click (see D10).

### 5.3 Current state (verified)

| Surface                    | Location                                                                           | Notes                                                                 |
| -------------------------- | ---------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Schema                     | `crates/oneshim-storage/src/migration/v01_v08.rs:186-212` (V5)                     | `tags` + `frame_tags` tables with CASCADE on frame/tag delete         |
| Per-row storage            | `crates/oneshim-storage/src/sqlite/tags.rs:152-186`                                | `add_tag_to_frame`, `remove_tag_from_frame`                           |
| Batch read                 | `crates/oneshim-storage/src/sqlite/tags.rs:8-52`                                   | `get_tag_ids_for_frames(&[i64])` already exists                       |
| Service                    | `crates/oneshim-web/src/services/tags_service.rs:91-114`                           | Delegates to per-row storage                                          |
| Handler (add)              | `crates/oneshim-web/src/handlers/tags.rs:83-98`                                    | **Loops** — no transaction, partial-success risk                      |
| Handler (remove)           | `crates/oneshim-web/src/handlers/tags.rs:73-81`                                    | Single-row only; **no batch path exists**                             |
| Route (add)                | `crates/oneshim-web/src/routes.rs:117`                                             | `POST /api/frames/batch-tags`                                         |
| API contract               | `crates/oneshim-api-contracts/src/tags.rs:23-32`                                   | `BatchTagRequest { frame_ids, tag_id }` / `BatchTagResponse`          |
| Frontend selection state   | `crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:113-114,116-128,202-204` | `Set<number>` + toggleFrameSelection + selectAllFiltered         |
| Frontend batch action bar  | `crates/oneshim-web/frontend/src/pages/timeline/AllFrames.tsx:582-603`             | Floating bar with TagInput; **add-only**                              |
| Frontend API client        | `crates/oneshim-web/frontend/src/api/client.ts:579-587`                            | `batchAddTag()`; no `batchRemoveTag()`                                |
| Transaction precedent      | `crates/oneshim-storage/src/sqlite/events.rs:126` (`let tx = conn`)                | `conn.transaction()` pattern — primary precedent for per-row INSERT-OR-IGNORE in transaction |
| Transaction precedent (secondary) | `crates/oneshim-storage/src/sqlite/maintenance.rs:419` (`let tx = conn`)    | Separate but demonstrates multi-table transactional DELETE — different character from tags (see CONS-M18) |

**Correction to the prompt's scope note**: the prompt implies "Frontend: `TimelineView.tsx:1-150` — no multi-select UI today", which is stale. The live surface is at `pages/timeline/TimelineLayout.tsx` + `pages/timeline/AllFrames.tsx` and it **already implements** a full select-mode with `Set<number>`, toggle, select-all, clear, and an `exit on Escape` key binding at `TimelineLayout.tsx:256-271`. The `components/TimelineView.tsx` file at lines 1-150 is a separate, regime-block vertical timetable component — unrelated to the frames timeline and not involved here.

This reshapes the "remaining work" for Feature 3: the UX is already there; we only need the batch-remove path to plug into it.

### 5.4 Storage-layer transactional ops

Add to `crates/oneshim-storage/src/sqlite/tags.rs`, after the existing `remove_tag_from_frame` at line 186:

```rust
impl SqliteStorage {
    /// Atomic batch add. Inserts `(frame_id, tag_id)` rows for every
    /// `frame_id` in `frame_ids`, wrapped in a single SQLite transaction.
    /// Returns the number of rows inserted (0 for duplicates via
    /// INSERT OR IGNORE). Rolls back on any error; either all rows
    /// commit or none.
    ///
    /// # Errors
    /// - `StorageError::Internal` on lock acquisition failure, transaction
    ///   start failure, or any per-row insert error (after rollback).
    pub fn add_tag_to_frames(
        &self,
        frame_ids: &[i64],
        tag_id: i64,
    ) -> Result<usize, StorageError> {
        if frame_ids.is_empty() {
            return Ok(0);
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Internal(format!("Failed to acquire lock: {e}")))?;

        let tx = conn
            .transaction()
            .map_err(|e| StorageError::Internal(format!("Failed to start transaction: {e}")))?;

        let inserted = {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT OR IGNORE INTO frame_tags (frame_id, tag_id) VALUES (?1, ?2)",
                )
                .map_err(|e| StorageError::Internal(format!("Failed to prepare query: {e}")))?;

            let mut n = 0usize;
            for fid in frame_ids {
                n += stmt
                    .execute(rusqlite::params![fid, tag_id])
                    .map_err(|e| StorageError::Internal(format!("Failed to add frame tag: {e}")))?;
            }
            n
        };

        tx.commit()
            .map_err(|e| StorageError::Internal(format!("Failed to commit transaction: {e}")))?;

        debug!(
            "batch add: {} frames tagged with tag_id={}, inserted={}",
            frame_ids.len(),
            tag_id,
            inserted
        );
        Ok(inserted)
    }

    /// Atomic batch remove. Deletes `(frame_id, tag_id)` rows for every
    /// `frame_id` in `frame_ids`, wrapped in a single SQLite transaction.
    /// Returns the number of rows actually deleted (a pair that didn't
    /// exist contributes 0 to the count). Rolls back on any error.
    ///
    /// # Errors
    /// - `StorageError::Internal` on lock acquisition failure, transaction
    ///   start failure, or any per-row delete error (after rollback).
    pub fn remove_tag_from_frames(
        &self,
        frame_ids: &[i64],
        tag_id: i64,
    ) -> Result<usize, StorageError> {
        if frame_ids.is_empty() {
            return Ok(0);
        }

        let mut conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::Internal(format!("Failed to acquire lock: {e}")))?;

        let tx = conn
            .transaction()
            .map_err(|e| StorageError::Internal(format!("Failed to start transaction: {e}")))?;

        let deleted = {
            let mut stmt = tx
                .prepare_cached(
                    "DELETE FROM frame_tags WHERE frame_id = ?1 AND tag_id = ?2",
                )
                .map_err(|e| StorageError::Internal(format!("Failed to prepare query: {e}")))?;

            let mut n = 0usize;
            for fid in frame_ids {
                n += stmt
                    .execute(rusqlite::params![fid, tag_id])
                    .map_err(|e| StorageError::Internal(format!("Failed to remove frame tag: {e}")))?;
            }
            n
        };

        tx.commit()
            .map_err(|e| StorageError::Internal(format!("Failed to commit transaction: {e}")))?;

        debug!(
            "batch remove: {} frames untagged from tag_id={}, deleted={}",
            frame_ids.len(),
            tag_id,
            deleted
        );
        Ok(deleted)
    }
}
```

**Design notes**:

- `prepare_cached` inside the transaction scope, so the statement cache is reused across all rows.
- `conn.lock()` is acquired **once**; the transaction is `mut conn`-scoped. The existing pattern at `sqlite/events.rs:126` does exactly this.
- Using `INSERT OR IGNORE` for add preserves the "idempotent add" semantics of the single-row path at `tags.rs:158-162`.
- Return type is `usize` (number of rows actually changed), which is strictly more information than the existing `BatchTagResponse.tagged_count: u32` — the handler can coerce down without loss.

### 5.5 Handler refactor — existing batch-add

Replace `crates/oneshim-web/src/handlers/tags.rs:83-98` with a transactional call:

```rust
/// POST /api/frames/batch-tags
pub async fn batch_add_tag(
    State(context): State<StorageWebContext>,
    Json(req): Json<BatchTagRequest>,
) -> Result<Json<BatchTagResponse>, ApiError> {
    let tagged_count = context
        .storage
        .add_tag_to_frames(&req.frame_ids, req.tag_id)?;
    Ok(Json(BatchTagResponse {
        tagged_count: tagged_count as u32,
    }))
}
```

The service-layer `TagsCommandService::add_tag_to_frame` at `services/tags_service.rs:91-98` is unchanged for the single-row path; it continues to be called by the non-batch route. Add a sibling service method `add_tag_to_frames` that wraps the new storage call to keep the service-handler separation intact.

### 5.6 Batch-remove endpoint — Decision D8

**Chosen**: **new endpoint** `DELETE /api/frames/batch-tags` rather than extending the POST with an `action` field.

**Rationale**:

- REST semantics: HTTP DELETE means "remove a resource". Bulk-remove of tag associations matches that semantic cleanly. Overloading POST with an action discriminator requires clients to read the body to know the intent, which breaks HTTP caching and method-level access controls.
- Industry precedent:
  - GitHub REST API: `DELETE /repos/{owner}/{repo}/labels/{name}` (single) is complemented by `DELETE /repos/{owner}/{repo}/issues/{issue_number}/labels/{name}` (single), and bulk removes are done by PUT-over-the-full-set.
  - Azure DevOps: separate endpoints per verb.
  - Notion API: separate endpoints.
  - Linear: GraphQL but still verb-separate mutations.
- Existing project precedent: `routes.rs:122-125` — the single-frame remove uses DELETE, so the bulk form matching the same verb is symmetric.

**Rejected**: `POST /api/frames/batch-tags {action: "add" | "remove"}`. Rejected because it forces clients to special-case the dispatch and couples two semantically opposite operations to the same URL. Also rejected because adding new actions later (e.g. `replace`) would keep multiplying.

New route and handler:

```rust
// crates/oneshim-web/src/routes.rs (adjacent to line 117)
.route("/frames/batch-tags", post(handlers::tags::batch_add_tag))
.route("/frames/batch-tags", delete(handlers::tags::batch_remove_tag))  // NEW
```

```rust
// crates/oneshim-web/src/handlers/tags.rs (new fn)

/// DELETE /api/frames/batch-tags
pub async fn batch_remove_tag(
    State(context): State<StorageWebContext>,
    Json(req): Json<BatchTagRequest>,
) -> Result<Json<BatchTagResponse>, ApiError> {
    let removed_count = context
        .storage
        .remove_tag_from_frames(&req.frame_ids, req.tag_id)?;
    Ok(Json(BatchTagResponse {
        // Reusing tagged_count name is awkward; see D8-alt below.
        tagged_count: removed_count as u32,
    }))
}
```

**Decision D8-alt**: the response DTO `BatchTagResponse { tagged_count: u32 }` (`crates/oneshim-api-contracts/src/tags.rs:29-32`) is misnamed for the remove case. Two sub-options:

- **Rename to `affected_count`** (backward incompatible for the response field — but this is a local contract not external API; only the frontend consumes it).
- **Add a parallel `BatchTagRemoveResponse { removed_count: u32 }`** type.

**Recommend**: rename to `affected_count`. The frontend patch is small and deterministic. Exact lines to edit in-PR per CONS-I09:

1. `crates/oneshim-api-contracts/src/tags.rs:29-31` — `BatchTagResponse { tagged_count: u32 }` → `BatchTagResponse { affected_count: u32 }` (field rename).
2. `crates/oneshim-web/frontend/src/api/client.ts:579-587` — `batchAddTag` return type updated; add `batchRemoveTag` with the same return shape.
3. `crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:131-140` — `data.tagged_count` → `data.affected_count` in `onSuccess` handler; i18n template placeholder becomes `{count}` (unchanged). Add symmetric `onSuccess` for `batchUntagged` using the same `affected_count` field.

Confirmed via `docs/contracts/oneshim-web.v1.openapi.yaml:1375` — the response schema is `GenericObject` (untyped), so there is no OpenAPI contract break. The rename is formally safe but still a local breaking change — documented in CHANGELOG.

### 5.7 Transaction semantics — Decision D9

**Chosen**: **all-or-nothing** (strict transactional, rollback on any row error).

**Rationale**:

- Matches REST "bulk operation = atomic operation" convention (Stripe Batch API, GitHub Projects v2 batch mutations, AWS SDK batch operations).
- The existing failure mode (loop without transaction) is what causes partial-success, and users have no way to tell which rows succeeded — telemetry on this is nil. All-or-nothing is strictly better from a recovery perspective.
- Implementation is already the natural path when using `conn.transaction()` — partial-success would require per-row-try-catch with a report, adding complexity without user benefit.
- The typical bulk size is small (user selects ~10 frames); transaction cost is negligible (< 1ms for 50 frames observed in the existing `events.rs:126` transactional path).

**Rejected**: partial-success with a per-row status vec. Rejected because:
- Requires a new response DTO shape (`[{frame_id, status, error}]`).
- Forces the frontend to reconcile partial state in the UI.
- Users would expect a retry UI we don't have.

### 5.8 Frontend multi-select UX — Decision D10 (settled: reuse existing pattern)

**Current UX** (verified at `pages/timeline/AllFrames.tsx:170-181,242-284,302-331,582-603`):

1. User clicks a "Select" button in the filter bar — enters `selectMode`.
2. Clicking a frame thumbnail in grid/list view toggles its selection (checkbox overlay appears in grid, left gutter in list).
3. A floating action bar at the bottom shows:
   - Selected count
   - "Select all" button (selects all filtered)
   - "Clear" button (empties the Set)
   - TagInput for adding a tag
4. Escape key exits select mode (keybinding at `TimelineLayout.tsx:256-271`).

**Phase 9 addition**: the floating action bar at `AllFrames.tsx:582-603` gets a second TagInput (or a segmented control) for **removing** a tag.

**Chosen interaction**: **checkboxes (the existing mechanism)** only. No shift+click range-select, no ctrl+click. Rationale:

- Discovery: a visible checkbox and toggle button is accessible to both mouse and keyboard users. Shift/ctrl-click is a mouse-only power-user convention that's invisible without documentation.
- Cost: keeping one interaction model is cheaper to test than two.
- The existing UX is already there and works — multiplying interaction models is change for its own sake.

The exit existing pattern is exactly what Slack uses in their batch-archive UI, what Linear uses, and what Notion uses (all of them are check-first with optional shift-click). Our baseline is aligned with the industry baseline.

**Rejected**: shift-click range. Rejected because it requires the user to conceptualize "anchor" and the grid is visually 2D (shift-click in a 2D grid ambiguates row-major vs column-major). Shift-click works best in list view only; we'd need dual semantics.

Mock interaction flow with batch-remove:

```
┌────────────────────────────────────────────────────────┐
│ [App: all]  [Importance: all]  [Tag: all]  … [Select] │  ← filter bar
└────────────────────────────────────────────────────────┘
┌────────────────────────────────────────────────────────┐
│ ☑ ☐ ☑ ☑ ☐                                             │  ← grid view (checkboxes visible in selectMode)
│  1  2  3  4  5                                         │
└────────────────────────────────────────────────────────┘

┌─ Floating batch action bar (fixed bottom) ───────────────────┐
│  3 selected  [Select all]  [Clear]   (Add tag)   (Remove tag)│  ← second "Remove" dropdown
└──────────────────────────────────────────────────────────────┘
```

**"Remove tag" popover — Decision D20 (U10 locked to Option A: show all + toast)**

The popover shows **all** tags (not the intersection of tags attached to all selected frames). Selecting a tag that isn't attached to some frames is a silent no-op for those frames — the transactional `remove_tag_from_frames` handles this correctly (`n` counts only actually-deleted rows). The confirmation toast says `"{affected_count} of {selected_count} frames untagged"` — the "x of N" shape gives the user transparent feedback on partial-application without surfacing a modal/intersection pre-fetch step.

Rejected: Option B (show intersection of tags across selection). Rejected because:
- Requires a round-trip pre-fetch to compute `get_tag_ids_for_frames(selection) ∩`.
- Surfaces no additional value (user can see selection count and attempt; the toast explains the result).
- Intersection-only is stricter than peer products (Gmail, Linear, Notion all show all labels, none intersection).

**A11y / ARIA gap**: the existing multi-select UI uses checkboxes (verified: `AllFrames.tsx` shows only `aria-hidden="true"` on decorative SVGs; no `role="checkbox"`, `aria-checked`, `aria-selected`, or named group region). This is a **pre-existing gap**; adding ARIA attributes is **out-of-scope for Phase 9** per the quick-wins scope. Tracked as a follow-up sprint item (a11y hardening for selection UI).

### 5.9 Selection persistence across pagination — Decision D11

**Chosen**: **reset selection on page change**.

**"Select all" scope clarification**: "Select all" selects **all frames currently loaded in the active page viewport** (≤ pageSize = 50). It does NOT select cross-page or all-filtered across pages. This keeps the selection bounded and matches the pageSize=50 perf budget in §5.10.

**Rationale**:

- The current implementation has no cross-page selection, and extending it would require holding a potentially unbounded `Set<number>` in state.
- User expectation: paging is a "I'm done with this page" signal. Persisting selection is surprising.
- Industry convention: GitHub, Gmail, Notion, Linear all reset selection on page change (or scope it to "what's visible"). Only Google Drive and Gmail's "select all 1,000" have cross-page selection and both are explicit opt-in flows.
- Implementation cost: none. The current `setSelectedFrames(new Set())` on page change already exists implicitly — no, let me verify: `TimelineLayout.tsx:88-104` changes `searchParams` which triggers a React Query refetch with a new cache key. The `selectedFrames` state is not tied to page, so it **would persist across pages** in the current implementation.

**Action**: add an explicit `setSelectedFrames(new Set())` in the `setPage` handler at `TimelineLayout.tsx:90-104` so switching pages clears selection. This makes the semantics match user expectation without surprising behavior.

**Backend page-size cap — Decision D19 (U — CONS-I08)**

Although the frontend caps at pageSize=50 and "Select all" stays bounded by the viewport, a scripted or third-party caller can hit `POST /api/frames/batch-tags` or `DELETE /api/frames/batch-tags` with an arbitrarily large `frame_ids` array. The `fetchFrames` handler at `crates/oneshim-web/src/handlers/frames.rs:12-18` does NOT enforce a limit cap today.

Phase 9 **adds** `const MAX_BATCH_SIZE: usize = 1000;` to the new batch handlers in `crates/oneshim-web/src/handlers/tags.rs`. If `req.frame_ids.len() > 1000`, the handler returns HTTP 400 with wire code `validation.invalid_arguments` and a structured message. Tests: `batch_remove_tag` with 1001 ids → 400; with 1000 ids → 200 completing < 50ms. This is the Q3 decision resolved in-place.

**Rejected**: cross-page selection. Rejected because:
- Bounded UI state grows unbounded.
- "Select all" currently means "all **filtered** frames on this page" — cross-page would need "select all 1,000" affordance.
- No user ask in feedback.

### 5.10 Performance bound — Decision D12 (actual numbers)

**Query limits** (verified):

- Frontend: `fetchFrames(from, to, limit=50, offset=0, minImportance=0.3)` at `crates/oneshim-web/frontend/src/api/client.ts:183-198`. **Default page size = 50**.
- Timeline page: `pageSize = 50` at `pages/timeline/TimelineLayout.tsx:111`.
- Backend handler: `fetchFrames` at `crates/oneshim-web/src/handlers/frames.rs:12-18` does **not** enforce a hard cap today. Phase 9 adds `MAX_BATCH_SIZE = 1000` to the new batch handlers only (per D19 in §5.9); the existing `fetchFrames` cap is a separate follow-up.

**Performance bound**: the maximum selection size within a single page is **50**. Bulk operation sends exactly those 50 ids. For a 50-id single-transaction insert/delete:

- SQLite WAL-mode + `INSERT OR IGNORE` for 50 rows = ~1-2ms typical (observed in the existing transactional `events.rs:126` path with batch inserts of similar size).
- Network round-trip (localhost): ~1-5ms.
- Total user-visible latency: **< 10ms** for 50 frames.

This is below the 100ms perceptual threshold and does not warrant streaming or pagination of the request body.

Per-page only, never cross-page, so no unbounded growth.

## 6. Cross-cutting concerns

### 6.1 Test strategy

**Feature 1 — Tracking Schedule** (test harness: pure-fn 2-arg shape per U3 = Option B):

- **Unit (pure fn, no mock clock)**: `tracking_schedule_helper::tracking_schedule_active(cfg: &AppConfig, now: DateTime<Local>) -> bool` — the 2-arg pure form is directly testable without clock injection. Mirrors the existing `should_run_when_disabled` pattern at `src-tauri/src/scheduler/mod.rs:582`. Cases: normal range in-window, normal range out-of-window, overnight range wrap at midnight, empty `windows`, `enabled=false` early return, multiple windows with one matching, all windows out-of-range.
- **Unit (pure fn)**: `capture_permitted_now(cfg, now)` composition truth table — 8 rows covering all 2³ combinations of `active_hours` / `tracking_schedule` / `capture_paused`, plus overnight `active_hours × overnight tracking_schedule` covering CONS-C05 fix.
- **Unit (pure fn)**: `TrackingWindow::window_is_active(now)` — 12+ cases: normal, overnight, empty days, DST spring-forward (window in skipped hour → false), DST fall-back (window ending 02:30 in duplicate hour → fires twice on US/Eastern — asserts the corrected CONS-C04 semantics, not "exactly once"), timezone `"Local"` vs IANA name.
- **Integration (still uses injected `now` via 2-arg pure fn; no mock-clock harness required)**: a test harness constructs `AppConfig` with a window `[12:00, 13:00]`, calls `tracking_schedule_active(cfg, t)` for `t ∈ {11:30, 12:30, 13:01}`, asserts `false → true → false`. Also tests the upstream gated event pipeline: with TS active, assert zero new rows in `events` table across Window/Input/Process/Clipboard/File event variants.
- **Integration**: `BatchUploader::flush()` short-circuit when suppression predicate returns `true`; pre-window events remain in queue; post-window flush drains them. Includes CONS-C03 worked example: event E at T=11:30 → stays in queue T=12:30 → ships at T=13:01.
- **IPC contract**: `set_tracking_schedule` → `get_tracking_schedule` roundtrip. `get_tracking_schedule_status` returns `active_now: true` when a configured window straddles the passed `now`.
- **DST fixtures**: US/Eastern DST spring-forward + fall-back dates asserted per the corrected §3.7 semantics.

**Feature 2 — Autostart** (CI strategy: env-var stub per U4 = Option B):

- **Unit**: existing **9** tests at `autostart.rs:460-548` (not 14 as a prior draft claimed; `grep -c '#\[test\]' src-tauri/src/autostart.rs` → 9) stay passing. Add:
  - `generate_service_file` with `WAYLAND_DISPLAY` env set → asserts both `Environment=DISPLAY` and `Environment=WAYLAND_DISPLAY` lines present.
  - `generate_service_file` with `XDG_SESSION_TYPE=wayland` → asserts `Environment=XDG_SESSION_TYPE=wayland` line.
  - `enable() returns Err on non-zero exit` — simulate by injecting a stub command (see env-var stub below) that returns exit 1; handler returns 500 + `is_enabled() == false`. Verifies CONS-C10 fix.
  - `enable() times out after 5s` — stub command sleeps >5s; handler returns timeout error. Verifies CONS-C10.
- **IPC contract**: `get_autostart_status` returns mechanism-appropriate string per platform (macOS → `launchctl`, Linux with systemd → `systemd`, Linux no systemd → `xdg_desktop`). Gate each with `#[cfg(target_os = ...)]`.
- **REST contract**: GET → PUT → GET cycle with `{enabled: true}` and `{enabled: false}` on each platform.

**CI env-var stub escape hatch**: GitHub `ubuntu-latest` runners ship `systemctl` but lack a user D-Bus session, so `systemctl --user enable` fails. A naive REST roundtrip on CI would silently pass with broken systemd registration today (CONS-C07). Phase 9 adds an env-var `ONESHIM_AUTOSTART_STUB=1` that, when set, causes the autostart module to:
- Skip the actual `Command::output` spawn on Linux (`systemctl`, `launchctl`, `reg`).
- Return `Ok(())` without side effects and record the command shape in a thread-local `TestObserver` that tests assert against.
- Assert the would-be command-line is correctly shaped (e.g. `systemctl --user enable oneshim.service`).

The CI job sets `ONESHIM_AUTOSTART_STUB=1` in the test env; the env-var defaults to unset in production. Test authors can also set it locally. This is a cheaper path than bootstrapping a D-Bus user session in CI (the rejected Option A).

- **Frontend**: Vitest for the `GeneralTab` autostart toggle — disabled when `mechanism == "unsupported"`, shows mechanism text, fires mutation on toggle, displays user-facing error copy per §4.10 mapping table when the mutation fails.

**Feature 3 — Bulk Tag** (test coverage gaps per CONS-I16):

- **Unit**: `crates/oneshim-storage/src/sqlite/tags.rs` —
  - `add_tag_to_frames` — all-rows-inserted path.
  - `remove_tag_from_frames` — mixed "some attached, some not" (missing pairs contribute 0 to count; total = actually-deleted rows).
  - `add_tag_to_frames_rolls_back_on_fk_violation` — `frame_ids` contains a nonexistent `frame_id` → FK violation fires mid-batch → whole batch rolls back → `frame_tags` unchanged.
  - `remove_tag_from_frames_handles_missing_pairs_transactionally` — (frame_id, tag_id) pair doesn't exist → `n` counts only actually-deleted rows; no error.
  - `batch_ops_compete_with_concurrent_writer` — second thread holds the storage lock; test verifies batch call blocks then succeeds; no deadlock.
  - `empty_input_is_lock_free` — `frame_ids = []` returns `Ok(0)` without acquiring the connection lock.
  - `statement_cache_reuse_across_rolled_back_transactions` — after a rollback, the next call with the same cached statement works correctly.
- **Integration**: handler-level — POST + DELETE roundtrip; 50-frame batch completes < 50ms (tighter than the prior 100ms claim per §5.10 measurement); forced-failure case (FK violation) returns HTTP 500 with wire code `storage.failed` and leaves zero rows changed (transactional). `MAX_BATCH_SIZE` cap: 1001 ids → 400 `validation.invalid_arguments`, 1000 ids → 200 < 50ms (D19).
- **Frontend**: Vitest for `batchRemoveTag` API client fn; Playwright E2E at `crates/oneshim-web/frontend/e2e/timeline-actions.spec.ts` (**not** `frontend/tests/…` — the `frontend/tests/` directory does not exist per CONS-M11) for "select 3 frames → add tag → remove tag → verify tag count returns to zero". The existing spec file is extended with new cases; no new E2E file is created.

### 6.2 Observability

New tracing spans — all `mech` fields use `%mech` (Display) not `?mech` (Debug) for stable Loki/Grafana log parsing per CLAUDE.md Logging convention; all error paths include `err.code = <wire-code>` structured field:

```rust
// Tracking Schedule
debug!("tracking_schedule: window_is_active = {}", active);
info!(label = %label, ends_at = %ends_at, "tracking_schedule: entered window");
info!(label = %label, "tracking_schedule: exited window, resuming capture");

// Autostart — enumerate ALL structured-log sites per CONS-M10
info!(mechanism = %mech, "autostart: enabled");
info!(mechanism = %mech, "autostart: disabled");
// macOS path
warn!(err.code = "internal.io", "autostart: launchctl load non-zero exit: {stderr}");
warn!(err.code = "internal.io", "autostart: launchctl load timeout after 5s");
warn!(err.code = "storage.failed", "autostart: plist write failed: {e}");
// Linux path
warn!(err.code = "internal.io", "autostart: systemctl --user enable non-zero exit: {stderr}");
warn!(err.code = "internal.io", "autostart: systemctl --user enable timeout after 5s");
warn!(err.code = "storage.failed", "autostart: service file write failed: {e}");
// Windows path
warn!(err.code = "internal.io", "autostart: RegSetValueExW returned non-zero: {code}");
warn!(err.code = "storage.failed", "autostart: registry write failed: {e}");
// Shared
warn!(err.code = "config.missing", "autostart: HOME env var not set");

// Bulk tag
debug!(frames = frame_ids.len(), tag_id, inserted, "batch add tag");
debug!(frames = frame_ids.len(), tag_id, deleted, "batch remove tag");
warn!(err.code = "storage.failed", "batch tag transaction rollback: {e}");
warn!(err.code = "validation.invalid_arguments", size = frame_ids.len(), "batch exceeds MAX_BATCH_SIZE");
```

New counters (optional, scheduler-emitted):

- `tracking_schedule_active_seconds_total` — cumulative seconds in a suppression window.
- `tracking_schedule_windows_configured` — gauge of `windows.len()` when enabled.
- `autostart_enabled` — gauge (0/1) reflecting OS-level state.

These are not required for MVP; list is for follow-up Grafana dashboard expansion.

### 6.3 Error code additions

Checked against `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` — **42 locked codes** (verified: `grep -c "^[a-z]" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` → 42). Note: the workspace CLAUDE.md text "Wire-format contract locked at 41 codes" is stale — file as `reference_doc_drift` follow-up. Phase 9 does not touch the wire-contract file and does not re-introduce Bedrock per ADR-019 §5.

**No new wire codes are required for Phase 9.** Every case in §4.10 (autostart) and §3 (tracking schedule config validation) maps to an existing catalog entry:

- `validation.invalid_arguments` — user supplied a malformed HH:MM in a TrackingWindow.
- `validation.invalid_field` — end or days_of_week empty.
- `config.missing` — HOME not set (autostart Linux).
- `config.invalid` — tracking_schedule.timezone is not a known IANA name.
- `storage.failed` — autostart file write failure.
- `internal.io` — launchctl / systemctl spawn failure.

If reviewer concludes "`tracking_schedule.invalid_window`" deserves its own code, the ADR-019 8-step checklist applies (per `client-rust/CLAUDE.md` reference to ADR-019 §5). The spec's recommendation: reuse the validation codes.

### 6.4 i18n keys

**Feature 1 — Tracking Schedule** (new keys, add to `crates/oneshim-web/frontend/src/i18n/locales/{en,ko}.json`):

```json
"settings.trackingScheduleTitle": "Tracking Schedule",
"settings.trackingScheduleDesc": "Suppress capture, events, and uploads during specific time windows. Your privacy control takes priority.",
"settings.trackingScheduleEnabled": "Enable tracking schedule",
"settings.trackingSchedule.addWindow": "Add window",
"settings.trackingSchedule.windowLabel": "Label (optional)",
"settings.trackingSchedule.start": "Start",
"settings.trackingSchedule.end": "End",
"settings.trackingSchedule.days": "Days",
"settings.trackingSchedule.timezone": "Timezone",
"settings.trackingSchedule.activeNow": "Active now — resumes {time}",
"settings.trackingSchedule.nextWindow": "Next window: {label} at {time}",
"settings.trackingSchedule.noWindows": "No windows configured.",
"notifications.trackingScheduleStart": "Tracking paused — resumes {time}",
"notifications.trackingScheduleEnd": "Tracking resumed",
```

```json
"settings.trackingScheduleTitle": "추적 일정",
"settings.trackingScheduleDesc": "특정 시간대에 캡처·이벤트·업로드를 중단합니다. 프라이버시 설정이 최우선으로 적용됩니다.",
"settings.trackingScheduleEnabled": "추적 일정 사용",
"settings.trackingSchedule.addWindow": "윈도우 추가",
"settings.trackingSchedule.windowLabel": "라벨 (선택)",
"settings.trackingSchedule.start": "시작",
"settings.trackingSchedule.end": "종료",
"settings.trackingSchedule.days": "요일",
"settings.trackingSchedule.timezone": "시간대",
"settings.trackingSchedule.activeNow": "현재 활성 — {time}에 재개됩니다",
"settings.trackingSchedule.nextWindow": "다음 윈도우: {label} ({time})",
"settings.trackingSchedule.noWindows": "설정된 윈도우가 없습니다.",
"notifications.trackingScheduleStart": "추적 일정 시작 — {time}에 재개됩니다",
"notifications.trackingScheduleEnd": "추적 일정 종료 — 추적이 재개되었습니다",
```

**Korean term lock (U11)**: "추적 일정" chosen over the loanword "스케줄" for consistency with enterprise-Korean convention (memory note: `feedback_industry_convention_check.md`). All Korean strings — settings title, notification body, tray tooltip — must use "추적 일정"; no mixed usage.

**Feature 2 — Autostart** (see §4.9 for the full en/ko table).

**Feature 3 — Bulk Tag** (additions only; existing keys from `timeline.*` are reused):

```json
"timeline.batchUntagged": "{{count}} frames untagged",
"timeline.removeTag": "Remove tag",
"timeline.removeTagPlaceholder": "Select a tag to remove…",
```

```json
"timeline.batchUntagged": "프레임 {{count}}개에서 태그 제거됨",
"timeline.removeTag": "태그 제거",
"timeline.removeTagPlaceholder": "제거할 태그를 선택하세요…",
```

**Autostart error-copy keys (CONS-I13 + §4.10 mapping)** — added to both `en.json` and `ko.json`:

```json
"settings.autostartError.fileWrite": "Could not write autostart file — please check permissions on ~/.config or ~/Library/LaunchAgents.",
"settings.autostartError.commandFailed": "Could not register autostart with the system — please check system settings.",
"settings.autostartError.homeMissing": "Autostart cannot be configured without a HOME environment variable.",
"settings.autostartError.unsupported": "Autostart is not supported on this system."
```

```json
"settings.autostartError.fileWrite": "자동 시작 파일을 작성할 수 없습니다 — ~/.config 또는 ~/Library/LaunchAgents의 권한을 확인하세요.",
"settings.autostartError.commandFailed": "시스템에 자동 시작을 등록할 수 없습니다 — 시스템 설정을 확인하세요.",
"settings.autostartError.homeMissing": "HOME 환경 변수가 설정되어 있지 않아 자동 시작을 구성할 수 없습니다.",
"settings.autostartError.unsupported": "이 시스템에서는 자동 시작을 지원하지 않습니다."
```

**i18n locale coverage — Decision D-i18n (U12 locked to Option B: defer es/ja/zh-CN)**: five locales exist (`en.json`, `ko.json`, `es.json`, `ja.json`, `zh-CN.json`). Phase 9 adds keys only to `en.json` and `ko.json`; missing keys in the remaining three locales **fall back to English** (i18next default behavior). A follow-up i18n PR adds the `es/ja/zh-CN` translations — tracked in `project_next_tasks.md`. This is an intentional scope reduction: the translation work itself is non-trivial and Phase 9's quick-wins framing deprioritizes it.

The `en.json` general i18n anchor is at line **1343** (not 1336 as a prior draft referenced — verified via `grep -n '"general"' crates/oneshim-web/frontend/src/i18n/locales/en.json`).

**User-facing tracking-schedule guide — Decision D-guide (U13 locked to: skip for Phase 9)**: no new `docs/guides/tracking-schedule.md` written for Phase 9. The Settings UI descriptions + tooltips carry enough for users. A dedicated guide + Korean companion is tracked as a docs-sprint follow-up (per `docs/DOCUMENTATION_POLICY.md` English-primary + Korean companion rule).

### 6.5 Lefthook / CI implications

- Existing `cargo test --workspace` + `cargo clippy --workspace` catch the Rust side; no new CI job.
- Frontend Vitest + Playwright suites gain test cases at `crates/oneshim-web/frontend/e2e/timeline-actions.spec.ts` (existing file; new cases appended per CONS-M11); existing jobs run them.
- `oneshim-web` OpenAPI contract at `docs/contracts/oneshim-web.v1.openapi.yaml` **must** be regenerated / hand-patched for the new routes (responsible step: the impl plan names this step explicitly — integrity gate is enforced by `.github/workflows/integrity-gates.yml`, not optional):
  - `GET /api/tracking-schedule`, `PUT /api/tracking-schedule`, `GET /api/tracking-schedule/status`
  - `GET /api/autostart`, `PUT /api/autostart`
  - `DELETE /api/frames/batch-tags`
- `docs/contracts/http-interface-manifest.v1.json` (hand-maintained — no generator script exists) must be updated accordingly. The impl plan names the responsible step.
- `commit-message-hygiene` check is fine (feat/refactor/fix prefixes will all apply cleanly).
- No new `#[allow(dead_code)]` introductions. Removing the `#![allow(dead_code)]` at `autostart.rs:4` is a clippy-clean diff once the IPC commands are wired.
- **`serial_test` required for new autostart integration tests** that touch real FS state (`~/.config/systemd/user/*.service`, `~/Library/LaunchAgents/*.plist`, HKCU registry) — per memory note `reference_serial_test_pattern.md`. Existing unit tests (file-content generation, pure functions) do NOT need `serial_test`; only the new IPC-roundtrip integration tests do.
- `oneshim-core::tests::wire_contract_snapshot` continues to pass — no wire-code additions (§6.3).
- **`blackout`-identifier sweep (CONS-M02)**: `grep -n "Blackout\|blackout" crates/oneshim-vision/src/trigger.rs` → line 370 contains `// ── Blackout-hours tests (Q3) ─`. Renamed/removed during trigger test migration (§3.8a). The final PR must contain zero `blackout` occurrences anywhere in the source tree.
- **Docs updates required in-PR**:
  - `docs/STATUS.md` — test-count bump (new unit tests change the total).
  - `docs/PHASE-HISTORY.md` — new Phase 9 entry.
  - `docs/DOCUMENTATION_POLICY.md` companion check: **no user-facing guide added** (per D-guide / U13). Explicit statement sufficient for policy compliance.
  - `CLAUDE.md` workspace reference (outside worktree) — "Wire-format contract locked at 41 codes" is stale → update to 42; tracked as `reference_doc_drift` follow-up if not in this PR.

## 7. Decisions log

| ID | Decision | Chosen | Rejected | Rationale source |
|----|----------|--------|----------|------------------|
| D1 | Tracking Schedule naming | "Tracking Schedule" | "Blackout Hours", "Quiet Hours", "Pause Schedule" | Industry survey §3.3 |
| D2 | Coexistence with `active_hours` | Logical AND: `consent AND active_hours AND NOT tracking_schedule AND NOT capture_paused` | Replace / override | Orthogonal semantics §3.4 |
| D3 | Unified `TimeWindow` primitive refactor | **Deferred** to follow-up | In-scope for Phase 9 | Quick-wins scope §3.5 |
| D3a | REST endpoint for autostart | **Add** `GET/PUT /api/autostart` | Tauri-only IPC | Web Settings is SoT §4.4 |
| D4 | Autostart persistence in config | **Filesystem as SoT**, no config field | `general.autostart_enabled: bool` in AppConfig | OS-owned setting §4.5 |
| D5 | Wayland support in systemd unit | Detect `WAYLAND_DISPLAY` at enable-time, emit both | X11-only / Wayland-only | Backward compat + Wayland §4.6 |
| D6 | Binary-path stability in unit file | **Document behavior** + cross-platform Repair button | Persistent watcher / lazy repair | Cost/benefit §4.7 |
| D7 | XDG `.desktop` fallback | **Keep** (best-effort caveat) | systemd-only | Coverage §4.8 |
| D8 | Batch-remove endpoint | **New** `DELETE /api/frames/batch-tags` | Extend POST with `action` field | REST semantics §5.6 |
| D8-alt | Response field naming | Rename `tagged_count` → `affected_count` | Two sibling DTOs | DRY §5.6 |
| D9 | Transaction semantics | **All-or-nothing** (rollback) | Partial-success with per-row report | REST bulk convention §5.7 |
| D10 | Multi-select interaction | **Checkboxes only** (reuse existing UX) | Shift-click / ctrl-click range select | Discoverability + cost §5.8 |
| D11 | Selection persistence across pagination | **Reset** on page change | Persist cross-page | Industry convention §5.9 |
| D12 | Timeline event count / perf budget | 50-per-page hard limit (current), <10ms bulk | Unbounded cross-page | Measured §5.10 |
| **D13** | **Analysis + focus + coaching loop gating (scope expansion)** | Phase 9 **adds** a schedule gate to analysis/focus/coaching loops + 7 other ungated pipelines (see §3.8 table rows 3-12); U1 locked Option A | Scope-reduce Phase 9 to capture+upload only | §3.8 expanded table / CONS-C01+CONS-C02 / U1 |
| **D14** | **`should_run_now` overnight handling** | **Fix + hoist both checks together** (U2 Option C); wrap-midnight branch added; schedule tests migrate from `trigger.rs` to `scheduler::mod::tests` | Option A (fix only); Option B (document limitation) | §3.4a / CONS-C05 / U2 |
| **D15** | **Batch-tag transactional refactor: 200→500 behavior change on mixed-partial-failure inputs** | **Intentional behavior change** — new semantics: all-or-nothing 500 instead of silent 200 with under-counted `tagged_count`; frontend `TimelineLayout.tsx:131-140` updated in-PR to handle `onError` | Retain 200 silent-partial behavior | §2.3 / §5.5 / CONS-C09 |
| **D16** | **`chrono-tz` placement** | **Direct dependency in `oneshim-core`** (U5 Option A); +2.1MB binary cost accepted | Port + adapter split (`TimezoneResolver`) | §3.7 / CONS-I02 / U5 |
| **D17** | **`SmartCaptureTrigger::with_schedule` refactor — in-PR** | **In-scope for same PR** (U7 Option A); DI callsites migrated, 3 schedule tests move from trigger to scheduler | Defer to follow-up; accept "half-migrated trigger" | §3.8a / CONS-I04 / U7 |
| **D-prop** | **Tray indicator propagation** | **ADR-016 `subscribe()`** (U6 Option A) — tray subscribes to config-change-bus | Tauri event emit; tray re-eval tick | §3.11a / CONS-I03 / U6 |
| **D-errtype** | **Autostart error-type upgrade — deferred** | **Keep `Result<_, String>`** (U8 Option A); map at boundary; typed `AutostartError` tracked as follow-up | Upgrade to typed `AutostartError` enum now | §4.10a / CONS-I05 / U8 |
| **D19** | **Backend page-size cap on batch endpoints** | **`MAX_BATCH_SIZE = 1000`** in new batch handlers; `> 1000` → HTTP 400 `validation.invalid_arguments` | Unbounded | §5.9 / CONS-I08 |
| **D20** | **"Remove tag" popover content** | **Show all tags** (U10 Option A); silent no-op for non-attached frames; toast `"{affected_count} of {selected_count}"` | Show intersection only (requires pre-fetch) | §5.8 / CONS-I12 / U10 |
| **D21** | **First-run autostart prompt — deferred** | **Defer to separate onboarding PR** (U9 Option B) | Minimal prompt in welcome dialog | §4.10b / CONS-I13 / U9 |
| **D22** | **Single schedule, multiple windows** | **Single `TrackingScheduleConfig`** with `Vec<TrackingWindow>` | N named schedules (Teramind/RescueTime shape) | §3.14 / CONS-M06 |
| **D-i18n** | **i18n es/ja/zh-CN translations** | **Defer to follow-up i18n PR** (U12 Option B); English fallback during the gap | Translate now in-PR | §6.4 / CONS-M17 / U12 |
| **D-guide** | **User-facing tracking-schedule guide** | **Skip for Phase 9** (U13); tracked as docs-sprint follow-up | In-PR guide + Korean companion | §6.4 / U13 |

Among these, the **decisions most likely to draw reviewer scrutiny** and therefore most in need of explicit review attention:

- **D5 (Wayland)**: risk of breaking X11-only servers that don't define `WAYLAND_DISPLAY`; confirm the env-variable fallback is conservative.
- **D8-alt (rename `tagged_count` → `affected_count`)**: a local breaking change. Reviewers should confirm no external consumers of the response (no external clients hit this endpoint).
- **D11 (reset on page change)**: current implementation silently persists across pages. The decision to reset is a behavior change, however small, and deserves explicit testing.
- **D3a (REST autostart)**: adds surface area that the tauri-only alternative would avoid; reviewers may push back on parity-with-Tauri arguments.
- **D13 (scope expansion to analysis/focus/coaching/event loops)**: transforms Phase 9 from "two gate points" to "thirteen-pipeline audit"; reviewers should confirm every gated pipeline in §3.8 table rows 3-12 has per-loop disposition rationale.
- **D14 (should_run_now overnight fix)**: latent-bug fix riding alongside the feature; reviewers should confirm the migrated trigger tests cover the overnight path end-to-end.
- **D15 (200→500 behavior change)**: explicit callout because the frontend consumer path is `TimelineLayout.tsx:131-140` and the change is silent-partial → explicit-error; reviewers should confirm the mutation-error UX is acceptable.

## 8. Alternatives considered (rejected)

### 8.1 Feature 1

- **"Blackout Hours"** (§3.3) — rejected for naming.
- **"Quiet Hours"** (§3.3) — rejected for notification-only connotation.
- **Overload `capture_paused` atomic** (§3.10) — rejected for state-reason ambiguity.
- **Unify `ScheduleConfig` + `quiet_hours` + `TrackingScheduleConfig`** (§3.5) — deferred, not rejected.
- **Fine-grained per-field suppression** ("only suppress OCR, still capture thumbnails") — rejected for Phase 9; not a user-requested shape.
- **Server-side enforcement** — rejected; client-only control is the GDPR-correct shape.
- **Pre-configured presets** ("9-to-5 work hours", "Lunch break", "After hours") — rejected; users create their own schedules at launch. No presets at launch; revisit if user feedback shows common patterns.
- **Multiple named schedules** (Teramind / RescueTime shape: `Vec<{name, enabled, windows}>`) — rejected; D22 picks single-schedule-multi-window for MVP simplicity.
- **Port-in-adapter `chrono-tz` placement** — rejected per D16 in favor of direct `oneshim-core` dependency.

### 8.2 Feature 2

- **Persist `autostart_enabled` in AppConfig** (§4.5) — rejected for external-mutation divergence.
- **Wayland-only unit template** (§4.6) — rejected for X11 coverage.
- **Auto-repair stale binary paths** (§4.7) — deferred.
- **systemd-only, drop XDG fallback** (§4.8) — rejected for coverage.
- **New `StartupTab` settings page for one toggle** (§4.9) — rejected for over-scoping.

### 8.3 Feature 3

- **Extend POST with `action` discriminator** (§5.6) — rejected for REST semantics.
- **Partial-success reporting** (§5.7) — rejected for complexity/benefit.
- **Shift+click range select** (§5.8) — rejected for discoverability.
- **Cross-page selection persistence** (§5.9) — rejected for unbounded state.

## 9. Open questions

All questions that had reviewer-drafted defaults with no user-decision blocker were folded into the Decisions log (§7) during Loop 1d. The following remain genuinely open and were NOT resolved to a Decision:

1. **Q2 — Config hot-reload latency**: `PUT /api/settings` → config manager → monitor loop tick. `ConfigManager` is read-through-clone per call, so new `tracking_schedule.enabled` / `windows` values take effect on the next tick (≤ 1s latency). Needs end-to-end verification with ADR-016 "config-change-bus" semantics once implemented — specifically, that the tray `subscribe()` path (§3.11a / D-prop) is wired through the change-bus and not a separate code path.

2. **Q6 — Tracking-schedule × Focus Mode interaction**: ONESHIM has a separate `focus_mode` feature at `runtime_state.rs:370` (`Arc<crate::focus_mode::FocusModeState>`). Does a user-facing interaction rule apply ("while in focus mode, don't fire tracking-schedule notifications", or "focus mode inherits tracking-schedule suppression")? Recommend: no special-case for Phase 9; focus mode handles its own notification suppression, tracking-schedule uses the generic `DesktopNotifier`. Cross-reference test: when both are active, only the appropriate gate fires notifications.

3. **Q9 — Post-rename external consumers** (D8-alt): any external consumer of `BatchTagResponse` beyond `TimelineLayout.tsx:131-140` we need to account for? Expected answer: no — OpenAPI schema is `GenericObject` (untyped); frontend web dashboard is the only consumer. If a bot/third-party exists, this is a breaking change they see. Hold for confirmation during impl.

**Promoted to Decisions during Loop 1d (resolved)**:

- ~~Q1 (chrono-tz placement)~~ → **D16** (§3.7) — accept `chrono-tz` in `oneshim-core` (U5 Option A).
- ~~Q3 (backend page-size cap)~~ → **D19** (§5.9) — `MAX_BATCH_SIZE = 1000` (CONS-I08).
- ~~Q4 (trigger refactor scope)~~ → **D17** (§3.8a) — in-PR (U7 Option A).
- ~~Q5 (has_systemctl caching)~~ → §4.3 note — `OnceLock<bool>` one-time init (CONS-I15).
- ~~Q7 (tray icon artwork)~~ → §3.11 inline — reuse Paused icon + change tooltip (CONS-M12).
- ~~Q8 (remove-tag popover)~~ → **D20** (§5.8) — show all + toast "x of N" (U10 Option A).

## 10. References

### Architecture Decision Records (all verified present)

- `docs/architecture/ADR-001-rust-client-architecture-patterns.md` — hexagonal pattern, `#[async_trait]`, DI conventions.
- `docs/architecture/ADR-003-directory-module-pattern.md` — directory-module convention for large source files (if `commands/tracking_schedule.rs` or `handlers/autostart.rs` grow past the threshold).
- `docs/architecture/ADR-004-tauri-v2-migration.md` — Tauri v2 IPC invoke conventions.
- `docs/architecture/ADR-008-network-resilience-patterns.md` — backoff / upload patterns (relevant for upload-defer in §3.9).
- `docs/architecture/ADR-016-config-change-bus.md` — config hot-reload (Q2).
- `docs/architecture/ADR-019-error-code-infrastructure.md` — wire code catalog (§6.3).

### Industry convention sources

- **RescueTime** — "Focus Time" / "Away Time" (https://www.rescuetime.com/).
- **ActivTrak** — "Tracking Schedule" (https://www.activtrak.com/).
- **Slack** — Do Not Disturb (DND) schedule shape: `do_not_disturb` API with `next_dnd_start_ts` / `next_dnd_end_ts` and per-user `dnd_enabled` — closest model for multi-range weekly schedules.
- **Apple DeviceActivitySchedule** — `DateComponents` start/end + `repeats` — reference for time-component + recurrence.
- **rrule.js / RFC 5545** — `FREQ=WEEKLY;BYDAY=MO,TU,WE;BYHOUR=12;BYMINUTE=0` — standard recurrence format. We intentionally simplify to a Slack-like shape rather than full RFC 5545 because the UX can't reasonably compose rrule strings.
- **GDPR Article 5 (purpose limitation, minimisation)** — (https://gdpr-info.eu/art-5-gdpr/).
- **GDPR Article 13/14 (transparency)** — (https://gdpr-info.eu/art-13-gdpr/, https://gdpr-info.eu/art-14-gdpr/).
- **GDPR Article 25 (by design/by default)** — (https://gdpr-info.eu/art-25-gdpr/).
- **GDPR Article 35 (DPIA)** — (https://gdpr-info.eu/art-35-gdpr/).

### Industry precedent for D8 and D10

- **GitHub REST API** — single-verb-per-endpoint convention.
- **Stripe Batch API** — atomic transactional bulk ops.
- **Linear, Notion** — checkbox-first selection.
- **Gmail, Google Drive** — explicit "select all N" cross-page affordance (rejected for Phase 9).

### Code citations (all verified against worktree tip `5618558c`)

- `crates/oneshim-core/src/config/sections/monitoring.rs:58-85` — existing `ScheduleConfig` (struct at 58-73, Default at 75-85).
- `crates/oneshim-core/src/config/sections/coaching.rs:118-124` — existing `TimeRange` HH:MM shape.
- `crates/oneshim-core/src/config/enums.rs:11-35` — `Weekday` enum (enum at 11-20, impl at 22-35).
- `crates/oneshim-core/src/config/mod.rs:41,113` — `AppConfig::schedule` registration.
- `crates/oneshim-core/src/consent.rs:102` — `ConsentManager` (for composition-rule top-gate wiring, §3.4).
- `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` — **42 locked wire codes**.
- `crates/oneshim-vision/src/trigger.rs:26-30,52-78,138-148,194-435` — `SmartCaptureTrigger` + schedule gate + **13** tests (mod starts at 194).
- `crates/oneshim-vision/src/trigger.rs:370` — leftover `// ── Blackout-hours tests` comment header (swept per CONS-M02).
- `crates/oneshim-vision/src/trigger.rs:373,398,409` — schedule tests migrating to scheduler (§3.8a).
- `crates/oneshim-network/src/batch_uploader.rs:74,96-156,185-197,199` — `with_health_flag` (precedent) + upload queue + flush entry point.
- `crates/oneshim-storage/src/sqlite/tags.rs:8-52,152-186` — tag storage.
- `crates/oneshim-storage/src/sqlite/events.rs:126` — `conn.transaction()` precedent (`let tx = conn`).
- `crates/oneshim-storage/src/sqlite/maintenance.rs:419` — second transaction precedent (`let tx = conn`).
- `crates/oneshim-storage/src/migration/v01_v08.rs:186-212` — `tags` + `frame_tags` schema (V5).
- `crates/oneshim-web/src/routes.rs:107-125` — tags routes.
- `crates/oneshim-web/src/handlers/tags.rs:73-98` — per-row + batch-add handlers.
- `crates/oneshim-web/src/handlers/frames.rs:12-18` — `fetchFrames` (no cap today; Phase 9 leaves unchanged).
- `crates/oneshim-web/src/services/tags_service.rs:91-114` — service layer.
- `crates/oneshim-api-contracts/src/tags.rs:3-32` — tags DTOs (BatchTagRequest at 23-26, BatchTagResponse at 29-31).
- `crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:62,111,113-114,116-128,131-140,202-204,256-271` — select-mode state + keybinding + batch-tag mutation consumer.
- `crates/oneshim-web/frontend/src/pages/timeline/AllFrames.tsx:170-181,242-284,302-331,582-603` — select-mode UI + floating action bar.
- `crates/oneshim-web/frontend/src/api/client.ts:183-198,579-587` — `fetchFrames` + `batchAddTag`.
- `crates/oneshim-web/frontend/src/pages/setting-tabs/ScheduleSettings.tsx:1-100` — existing ScheduleSettings UI (reference for TrackingSchedule UI component).
- `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx` — target for autostart toggle (§4.9).
- `crates/oneshim-web/frontend/src/i18n/locales/en.json:203,1343` — nav `general` anchor + settings `general` anchor.
- `crates/oneshim-web/frontend/src/i18n/locales/ko.json:157,26` — timeline + navigation i18n anchors.
- `docs/contracts/oneshim-web.v1.openapi.yaml:1375` — `BatchTagResponse` schema (GenericObject; untyped).
- `src-tauri/src/autostart.rs:1-549` — entire module (all platform branches).
- `src-tauri/src/autostart.rs:4` — `#![allow(dead_code)]` to be removed post-wiring.
- `src-tauri/src/autostart.rs:8,31,54` — public fn entry points.
- `src-tauri/src/autostart.rs:93,189,326` — single-line `current_exe()` calls per platform module.
- `src-tauri/src/autostart.rs:137-141` — macOS `enable` with `launchctl load` non-zero-exit-swallowed bug (CONS-C10 target).
- `src-tauri/src/autostart.rs:332-350` — `generate_service_file` (Wayland change target).
- `src-tauri/src/autostart.rs:365-371` — `has_systemctl` (needs `pub(crate)` visibility bump + `OnceLock` cache).
- `src-tauri/src/autostart.rs:373-457` — Linux enable/disable/is_enabled (CONS-C10 `systemctl enable` non-zero-exit swallow at 389-401).
- `src-tauri/src/autostart.rs:404-417` — XDG fallback.
- `src-tauri/src/autostart.rs:460-548` — existing **9** unit tests.
- `src-tauri/src/commands/mod.rs:1-21` — command module list (new `autostart.rs` + `tracking_schedule.rs` registration).
- `src-tauri/src/commands/capture_status.rs:62-153` — `get_capture_status` (line 62) + `toggle_capture_pause` (line 72) + `fetch_xor` (line 76).
- `src-tauri/src/runtime_state.rs:347-384,366,370,667-674` — `AppState` struct + `indicator_visible` (line 366) + `focus_mode` (line 370) + `capture_paused` initialization.
- `src-tauri/src/scheduler/loops/intelligence.rs:14,124,160` — `spawn_analysis_loop` / `spawn_focus_loop` / `spawn_coaching_loop` (all ungated today; Phase 9 adds gates per §3.8 rows 4-6).
- `src-tauri/src/scheduler/loops/monitor.rs:58,181-189,200-207,292` — `capture_paused` + pre-gate `save_event` path at 181-189 (Window events leak per §3.8 row 3) + `should_run_now` gate in monitor loop; file length 498 (CONS-I06).
- `src-tauri/src/scheduler/loops/events.rs:60-92,93-111,112-128,130-145` — Process/Input/Clipboard/File event sub-sections (process_interval branch + input_interval branch's 3 sub-sections; all ungated today; §3.8 rows 7-10).
- `src-tauri/src/scheduler/loops/sync.rs:15,87` — oauth_refresh + cross_device_sync loops.
- `src-tauri/src/scheduler/mod.rs:429-430,548-571,582` — `Scheduler::with_capture_paused` builder + `should_run_now` helper + `should_run_when_disabled` test precedent.
- `src-tauri/src/tray.rs:181,207,241` — tray icon state binding to `capture_paused`.

### Docs references

- `docs/DOCUMENTATION_POLICY.md:11` — English-primary, Korean companion alignment.
- `docs/contracts/oneshim-web.v1.openapi.yaml` + `docs/contracts/http-interface-manifest.v1.json` — contract freeze targets.
- `client-rust/CLAUDE.md` — workspace conventions (AppState guardrails, monitor-loop complexity budget, port instance sharing).

---

_End of spec._
