# Phase 9 Spec Review 2 — Product/UX + Privacy + Industry

**Reviewer scope**: Product/UX, GDPR/CCPA/state-law, industry convention (RescueTime/ActivTrak/Slack/Apple), edge cases, multi-select UX, autostart UX, suppression scope completeness
**Out of scope (owned by R1 / R3)**: architectural drift, ADR adherence, Hexagonal port placement, cross-platform probes, test-harness mechanics, Rust compile correctness
**Date**: 2026-04-23
**Spec reviewed**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` @ 1247 lines
**Worktree tip**: `5618558c` (branch `feature/phase9-quick-wins`)

---

## Summary

- Critical: **3**
- Important: **6**
- Minor: **7**

Gate verdict: **NOT READY** — zero-Critical / zero-Important gate fails. Three Critical findings directly undermine the GDPR purpose-limitation story the spec leans on in §2.1 / §3.9; six Important findings touch regulatory framing, scope enumeration, and wire-contract compatibility.

---

## Critical findings

### C1. Scope-suppression enumeration is materially incomplete — window titles, input events, process snapshots, clipboard, file-access leak during tracking-schedule windows

**Where**: §3.1 ("capture + events + upload + analysis are suppressed") and §3.8 (table of four gate points).

**Observed**: §3.8 names only four gate boundaries:
1. `trigger.rs:138-148` (capture decision)
2. `scheduler/loops/monitor.rs:200-207,292` (monitor-loop capture guard)
3. `batch_uploader.rs:199` (upload flush)
4. `scheduler/loops/intelligence.rs` (analysis loop)

Cross-checking against the actual scheduler surface:

| Pipeline emitting PII | File:line | Gated by `should_run_now`? | Covered by spec §3.8? |
|---|---|---|---|
| Window-switch events (`Event::Window`, app_name + window_title) | `src-tauri/src/scheduler/loops/monitor.rs:181-189` | **No — saved + enqueued BEFORE `within_active_hours` gate at line 207** | **No** |
| Input activity events (`Event::Input`, keystroke/mouse/scroll counts) | `src-tauri/src/scheduler/loops/events.rs:94-110` | **No** (whole loop is un-gated) | **No** |
| Process snapshot events (`Event::Process`, every running process + metadata) | `src-tauri/src/scheduler/loops/events.rs:63-92` | **No** (un-gated) | **No** |
| Clipboard change events (`Event::Clipboard`, content fingerprint) | `src-tauri/src/scheduler/loops/events.rs:117-124` | **No** (un-gated) | **No** |
| File-access events (directory-watcher) | `src-tauri/src/scheduler/loops/events.rs:128-…` | **No** (un-gated) | **No** |
| Focus analyzer loop (reads events store) | `src-tauri/src/scheduler/loops/intelligence.rs:124-152` | **No** (un-gated) | **No** |
| Coaching loop (evaluate implicit feedback) | `src-tauri/src/scheduler/loops/intelligence.rs:160-220` | **No** (un-gated) | **No** |
| Cross-device sync loop | `src-tauri/src/scheduler/loops/sync.rs:87-…` | **No** | **No** |
| Audio capture / STT (user-initiated via `commands::audio`) | `src-tauri/src/main.rs:340-348` | N/A (user command) | **No** — spec does not note that `start_audio_capture` should refuse during tracking window |

**Why this is Critical**: §3.1 promises suppression of "capture + events + upload + analysis"; §2.1 cites GDPR Art. 5 purpose limitation as the regulatory anchor. Window titles and clipboard content are classic PII (file names, document titles, URLs). Shipping Tracking Schedule that visibly stops the red-border capture indicator while silently continuing to store + enqueue window-title, keystroke, and clipboard telemetry would violate both the stated goal and the GDPR framing the spec uses as motivation. A user who sets a "therapy session 14:00–15:00" window and sees capture stop but whose therapist's window titles still hit the server is actively misled.

**Required spec change**:
1. Extend §3.8 table to enumerate ALL pipelines that must be gated. Recommend a single generic approach: every `save_event` + `sink.enqueue` call site in the scheduler loops must read `tracking_schedule_active(&cfg)` and short-circuit, OR the gate must be hoisted to `save_event`/`enqueue` themselves. (Architecture placement belongs to R1; scope enumeration is non-negotiable here.)
2. Explicitly state that `commands::audio::start_audio_capture` must return an error (or no-op) when called during a tracking-schedule window, with wire code `validation.invalid_arguments` or a new `tracking_schedule.active`.
3. Confirm focus loop, coaching loop, and cross-device sync loop are suppressed.
4. Add an integration test that simulates a 60-second tracking window and asserts **zero** new rows in `events` table for that period across EVERY event variant (Window/Input/Process/Clipboard/File/Context).

### C2. Upload-defer contradicts itself — pre-window-enqueued events containing in-window PII will ship on window exit

**Where**: §3.9 "In-flight events already in the `BatchUploader` SegQueue are NOT flushed … When the window exits: the queue naturally drains on the next flush tick."

**Observed**: The spec correctly identifies (per §3.9) that mid-window flush violates purpose limitation. But combined with C1's un-gated event loops, the actual runtime behavior is:

1. At T=0 (before window), queue contains pre-window events. (Fine.)
2. At T=t₁ window begins. Capture gate stops capture. Events loop CONTINUES enqueuing window/input/process/clipboard events (per C1). These events have timestamps INSIDE the window.
3. At T=t₂ window ends. Queue now contains mixed pre-window + in-window events.
4. Flush drains entire queue including in-window PII.

Even if C1 is resolved (event loops also gated), the spec still has a gap:

- What about events already in the queue at T=t₁? Those are legitimately pre-window, so the current "keep them until exit" rule is correct — but the spec doesn't say so explicitly. A reader could reasonably interpret §3.9 as "drop the queue at window entry" (stricter reading) or "drain mid-window" (looser reading). Ambiguity on a privacy primitive is Critical.

- Conversely, what if a user sets a very long window (24h)? The existing `drop_oldest()` at `batch_uploader.rs:135-156` will start dropping pre-window events, which is data loss they may not want.

**Required spec change**:
1. Explicitly state the **intent** for pre-window events: "drain on window exit, in FIFO order." Document that users who want a stricter guarantee can use `DELETE /data` to purge (GDPR Art. 17 path).
2. Prove C1 is resolved (events gated upstream), then state "no in-window-timestamped event can reach the uploader queue; therefore the exit-flush cannot ship in-window PII."
3. Add a worked example in §3.9 showing timestamps: pre-window event at T=11:30, window 12:00–13:00, uploader flushes at T=13:01 — the 11:30 event ships. An attempted capture-event at T=12:30 must not exist in the queue (C1). Test-asserted.
4. Address drop_oldest during long windows: recommend a cap check "if queue > 10000 items at window entry, prioritize pre-window flush just before window entry, not after."

### C3. DST fall-back semantics stated incorrectly — documented as "fires exactly once" but actually fires twice

**Where**: §3.7 "On fall-back (duplicate hour), the window is checked once per actual wall-clock minute — a window ending at `02:30` fires exactly once across the duplicated hour."

**Observed**: The evaluator at §3.7 uses wall-clock `"HH:MM"` string comparison via `now.format("%H:%M")`. When DST fall-back repeats the 01:00–02:00 hour (US/Europe), wall-clock 02:30 occurs TWICE — once at DST time, once at standard time. The predicate `hhmm_now < "02:30"` fires both times. So a window ending at 02:30 is ACTIVE across both occurrences, not one.

For a **suppression** window (over-suppress = safer), this is arguably benign. But the spec states the wrong behavior, which means:
- Tests written to the spec's stated semantics will assert "fires once" and will fail on actual DST systems, or worse, will pass because they don't exercise the ambiguous hour.
- Review and audit artifacts (GDPR DPIA) will document incorrect behavior.
- A spring-forward window entirely within the "lost hour" (e.g. 02:00–03:00 on US DST Sunday) is claimed to "simply be skipped and no user-visible anomaly occurs" — but if a user configured this window as a suppression block, they will get ZERO suppression that day. That IS a user-visible anomaly.

**Required spec change**:
1. Correct the fall-back semantics: "during a repeated wall-clock hour, the window is evaluated on every tick, so it fires twice. Because this is a suppression (over-suppress-safe) primitive, this is acceptable, but an integration test must assert the behavior."
2. Correct the spring-forward semantics: "if a window's time range falls entirely within the skipped hour (e.g. 02:30–02:59 on US DST Sunday), the window does NOT fire that day. The UI should warn when a user configures a window that overlaps the skipped hour in their configured timezone for the current year."
3. Add explicit integration test for US/Eastern DST transitions.

---

## Important findings

### I1. Terminology creep — "추적 스케줄" vs "추적 일정" vs "Tracking Schedule" — Korean translation is inconsistent across i18n keys and notification strings

**Where**: §6.4 i18n.

**Observed**:
- English: "Tracking Schedule" (settings title) + "Tracking paused — resumes {time}" (notification).
- Korean: "추적 스케줄" (settings title, §6.4 line 1053) BUT "추적 일시 중단" / "추적이 재개되었습니다" (notifications, lines 1065-1066) — the notification strings drop the "스케줄" term entirely and just use "추적" (tracking).

This is a mild inconsistency between the two surfaces. A non-Korean speaker may miss it; a KR speaker will wonder if "추적" alone (just "tracking") means the same thing as "추적 스케줄" in the setting title. More importantly, Memory item `feedback_industry_convention_check.md` applies: industry convention in Korean locale is "추적 일정" (time schedule) or "추적 시간대" (time band) — "스케줄" is a loanword that enterprise-Korean users find jarring relative to "일정". Consider: `추적 일정` for the setting title and `추적 일정` in the notification body, or pick one term and stick with it.

**Recommended spec change**: audit all Korean strings; pick ONE consistent term. If "스케줄" is chosen, notification body should also reference "스케줄" (e.g. "추적 스케줄이 활성화되었습니다"). If "일정" is chosen, settings title follows.

### I2. Regulatory framing incomplete — missing CCPA/CPRA, US state electronic monitoring acts, GDPR Art. 13/14

**Where**: §2.1 "Regulatory grounding".

**Observed**: The spec cites GDPR Arts. 5, 25, 35. Missing:

- **GDPR Article 13 / 14** (transparency at collection) — §3.11 implicitly addresses this by keeping notifications ON, but the motivation section doesn't cite Art. 13/14 as the reason. A legal reviewer may want the explicit linkage.
- **CCPA / CPRA** (California) — no mention. Tracking Schedule is a standard "right-to-know-when-data-is-collected" control. Omission is fine for MVP but the spec should flag "CCPA/CPRA notice-at-collection obligations are not addressed by Tracking Schedule; a separate privacy-notice UI is required."
- **NY Civil Rights Law §52-c, DE §19, CT §31-48d** (electronic workplace monitoring notice acts) — these require WRITTEN NOTICE to employees, independent of whether a suppression feature exists. The spec should note that Tracking Schedule does NOT substitute for the notice obligation; it reduces scope only.
- **GDPR Art. 17** (right to erasure) — §3.2 mentions `DELETE /data` as existing. Fine.

**Why Important, not Critical**: The GDPR 5/25/35 trio is the strongest regulatory anchor and IS cited. Missing CCPA / state-law framing doesn't block implementation but WILL come up in privacy review before production.

**Recommended spec change**: add a "Regulatory scope" sub-section in §2.1 explicitly enumerating:
```
- GDPR Art. 5, 13/14, 25, 35 — directly supported by this feature.
- CCPA/CPRA notice-at-collection — not addressed; separate control needed.
- US state monitoring acts (NY §52-c, DE §19, CT §31-48d) — written notice
  obligation is orthogonal; Tracking Schedule reduces scope only.
- GDPR Art. 17 right to erasure — existing `DELETE /data` primitive.
```

### I3. Consent interaction not specified — schedule vs revoked consent conflict unspecified

**Where**: §3.4 (composition rule), §3.10 (atomic interaction), §3.12 (IPC), §3.13 (migration). No section on `ConsentManager`.

**Observed**: `crates/oneshim-core/src/consent.rs:102` (`ConsentManager`) tracks `screen_capture`, `ocr_processing`, `input_activity`, `window_title_collection`, etc. Revoked consent means "we must not capture this tier regardless of other config." The spec's composition rule in §3.4:

```
capture_allowed(now) = active_hours_gate(now) AND NOT tracking_schedule_active(now) AND NOT capture_paused
```

… does NOT mention consent. So:
- If a user revokes `screen_capture` consent, capture stops via the consent gate — but the spec's composition rule would say "capture_allowed = true" if only schedule/active_hours/paused are checked. This is a documentation gap; the actual code presumably checks consent independently, but the spec's rule is stated incompletely.
- Cross-dependency: if `consent.screen_capture = false` AND tracking_schedule_active = false, capture stops (consent wins). Fine.
- Cross-dependency: if `consent.screen_capture = true` AND tracking_schedule_active = true, capture stops (schedule wins). Fine.
- **Concerning case**: during an active tracking window, if the user calls `DELETE /data` to purge in-window events (a Art. 17 right to erasure path), does that also purge the uploader queue? Spec §3.9 doesn't connect.

**Why Important**: users expect revoked consent to be the highest-authority gate. The spec's composition rule should say so to close any reader-induced ambiguity.

**Recommended spec change**: in §3.4, change the rule to include consent as the top gate:
```
capture_allowed(now, tier) = consent_granted(tier)
                          AND active_hours_gate(now)
                          AND NOT tracking_schedule_active(now)
                          AND NOT capture_paused
```
And add a new §3.4.a documenting conflict resolution.

### I4. Multi-select UX — no upper bound, "Select all filtered" can exceed page size and the spec's perf bound (§5.10)

**Where**: §5.8 batch action bar, §5.10 perf bound.

**Observed**:
- §5.10 states "maximum selection size within a single page is 50" and derives <10ms latency. But §5.8 quotes "Select all" button behavior from `AllFrames.tsx:582-603`: the existing implementation calls `selectAllFiltered`, and the spec §5.9 says "Select all currently means 'all **filtered** frames on this page'".
- **BUT** `TimelineLayout.tsx:116-128` (per the spec's own citation in §5.3) defines `selectAllFiltered` — the name implies FILTERED not PAGED. If the filter returns 1000 frames across 20 pages and the user clicks "Select all", does the Set contain 50 (page-scoped) or 1000 (filter-scoped)?
- Spec D11 decides "reset on page change" but does NOT clarify "Select all" scope. If "Select all" means "all filtered across all pages", then D11's reset-on-page-change is a trap (user selects 1000, clicks Next Page, selection vanishes).
- Downstream: if "Select all" selects > 50, the batch op request body exceeds the implicit perf budget. No hard cap is specified in `handlers/tags.rs` or the new storage ops. Q3 in the spec acknowledges this but defers to impl.

**Why Important, not Critical**: the existing UX already has this edge case; Phase 9 isn't introducing a new bug. But shipping batch-remove without clarifying "Select all" scope compounds the existing ambiguity. Users will ship a support ticket saying "I selected 1000, clicked remove, only 50 were affected" or similar.

**Recommended spec change**:
1. Clarify "Select all" scope: explicitly state "Select all selects all frames currently loaded in the active page viewport (≤ pageSize = 50)." OR state "Select all selects all filtered frames across all pages (unbounded); clients must cap requests at `max_batch_size = 1000`."
2. Specify `max_batch_size = 1000` in the new batch handlers and return `validation.invalid_arguments` (400) when exceeded. The spec Q3 tentatively recommends this; promote to a Decision D12-a (normative).
3. Add test: `batch_remove_tag` with 1001 ids → 400; with 1000 ids → 200 (asserts under 50ms).

### I5. "Remove tag" popover content decision deferred to impl time (Q8), should be a Decision D-something

**Where**: §5.8 "Remove tag" popover, Q8.

**Observed**: §5.8 says "selecting a tag that isn't attached to some frames is a no-op for those frames" and the spec defers the "show all tags vs show intersection" decision to Q8 open question. But that's a UX decision with real impact:
- "Show all tags" is simpler; user selects tag X, frames that don't have X are silently untouched. Feedback: "affected_count: 3 of 10 selected".
- "Show intersection" is more discoverable; user only sees tags that appear on ALL selected frames — prevents accidental no-op selection — but computing the intersection requires a read round-trip that isn't currently wired.

**Why Important**: the spec needs this as a normative decision before impl, otherwise Loop 2 will land either version and the reviewer will re-open it.

**Recommended spec change**: promote Q8 to Decision D13: "Show all tags in the remove popover; display `{affected_count} of {selected_count}` in the confirmation toast." Remove from Open Questions.

### I6. Autostart UX gap — no first-run prompt decision, and user can lose their autostart after a binary move with no repair UI

**Where**: §4.7 "Re-enable is idempotent", §4.9 UI toggle location.

**Observed**:
- §4.7 mentions "Surface this as a 'Repair Autostart' button in the Settings UI only when …" and defers to follow-up.
- No first-run prompt decision — users will not know autostart exists until they navigate to Settings → General. Most peer products prompt explicitly during onboarding (Slack, Todoist, 1Password). RescueTime prompts at first capture-grant moment.
- The toast feedback in §4.9 "display toast on success or error" is under-specified — if `PUT /api/autostart` returns 500, what does the toast say to the user? Wire codes `internal.io` and `storage.failed` need user-facing translations.

**Why Important**: a toggle without first-run prompt and without repair UI is a discoverability regression versus the peer-product baseline.

**Recommended spec change**:
1. Add D14: "First-run onboarding includes an autostart toggle with explicit consent-style language." Position the decision; resolution can be "yes, minimal prompt in welcome dialog" or "no, defer to separate onboarding PR." Either is fine; the current spec leaves it implicit.
2. Add the Repair-Autostart UI to the in-scope set with a one-sentence behavior spec (§4.7 currently says "deferred to follow-up").
3. Specify user-facing error copy for toast: "Could not enable autostart — please check [system permissions]." Map wire codes to translation keys in §6.4.

---

## Minor findings

### M1. Leftover "Blackout" string in `trigger.rs:370` not reconciled by spec §6.5 CI checks

`crates/oneshim-vision/src/trigger.rs:370` contains `// ── Blackout-hours tests (Q3) ─` as a comment header. Spec §3.3 explicitly rejects "Blackout" naming. The rename impact section (§6.5 Lefthook/CI) doesn't enumerate this stale identifier. Spec should say: "audit the codebase for leftover `blackout` identifiers (currently `trigger.rs:370`) and rename to `tracking_schedule` or remove when the referenced tests migrate."

### M2. Spec §4.9 fabricates context — "Start minimized" toggle does not exist

§4.9 states "GeneralTab already hosts app-lifecycle toggles ('Start minimized', 'Check for updates at startup')". Verification against `GeneralTab.tsx` shows only language, web port/allowExternal, and update-related toggles. "Start minimized" is not present. This is a minor documentation accuracy issue — the placement argument ("belongs to the same mental category") is still valid, but the specific citation is wrong.

### M3. Accessibility (ARIA) coverage not addressed for multi-select

§5.8 states the interaction "checkboxes (the existing mechanism) only" and claims "accessible to both mouse and keyboard users". Verification of `AllFrames.tsx` shows only `aria-hidden="true"` on decorative SVGs — no `role="checkbox"`, `aria-checked`, `aria-selected`, or named group region on the selection UI. Spec should either (a) note the pre-existing a11y gap as out-of-scope for Phase 9, or (b) add ARIA attributes as a Phase 9 hardening item. Preferred: (a) with a TODO sprint item.

### M4. Tracking-schedule notification i18n opt-out key name conflicts with naming convention

§3.11 introduces `notification.tracking_schedule_notifications: bool` — the field name contains the word "notifications" twice. Existing convention in `NotificationConfig` (`crates/oneshim-core/src/config/sections/notification.rs`) uses shorter names like `idle_enabled`, `long_session_enabled`. Recommend `tracking_schedule_enabled` (field lives on NotificationConfig so the section name provides context). Minor.

### M5. Multiple-schedules-per-user question left implicit

Peer products (Teramind, RescueTime, ActivTrak) allow users to define **multiple named schedules** (e.g. "Weekdays", "Weekends", "On-call") and enable/disable them independently. The spec's shape is ONE `TrackingScheduleConfig` with a `windows: Vec<TrackingWindow>` — effectively one schedule. Users cannot name a group of windows and enable/disable the group without editing individual windows. This may be intentional (simpler UX) but the spec doesn't state why. Add a §3.14 "Single schedule, multiple windows — Decision D-new" documenting the trade-off explicitly.

### M6. Midnight-crossing worked example in §3.4 worked correctly, but semantics of "*" (any day) glossed over

§3.4 table row 7: `{22:00–06:00 *}` with `now = 23:00 Wed`. The value `*` is not defined in the config shape (§3.6) — `days_of_week: Vec<Weekday>` is a concrete enum list. The `*` must mean "all 7 Weekday values enumerated." Minor notation inconsistency; either add `*` as shorthand in the example, or spell out `[Mon, Tue, Wed, Thu, Fri, Sat, Sun]`.

### M7. No rejected option documented for "pre-configured presets"

Peer products often ship pre-configured presets ("9-to-5 work hours", "Lunch break", "After hours"). The spec's §8.1 rejected-options list does not include "ship presets." Since the spec rejects unification (D3) and preset libraries, a brief "no presets at launch; users create their own schedules" note in §8.1 would close the gap. Minor; only matters for consistency of the "alternatives considered" audit trail.

---

## Dimensions checklist

- [x] A. **Industry convention** — mostly well-aligned. "Tracking Schedule" chosen correctly (D1). Multiple-range-per-day supported from day one (§3.7). Upload-defer correct strategy (§3.9, modulo C2). Notifications ON correctly per §3.11. **Gap**: single-schedule-vs-multiple-schedule question (M5). Indicator coverage: tray + badge + toast specified (good).
- [x] B. **GDPR + regulatory** — Arts. 5, 25, 35 cited (§2.1). Art. 13/14 implicit in §3.11 but not cited. **Gap I2**: CCPA/CPRA + US state acts not framed.
- [x] C. **Naming consistency (en + ko)** — EN uniform. **Gap I1**: KR inconsistent between settings title and notification strings. **Minor M1**: leftover "Blackout" in trigger.rs:370.
- [x] D. **Feature completeness vs peers** — mostly matches baseline. **Gap M5**: no multiple-named-schedules. **Gap I6**: no first-run autostart prompt (peer-product standard). Tag batch: spec documents "add tag" vs "remove tag" symmetry well; but no "replace tag" in scope (acceptable for Phase 9).
- [x] E. **Edge cases** — DST fall-back stated incorrectly (**C3**). DST spring-forward skip case documented but user-visible anomaly claim is wrong. Midnight-crossing shown in §3.4 (minor notation M6). Timezone change mid-window — **not addressed** in spec; user shifts from Seoul→Tokyo mid-window, does the rule use creation TZ (Local) or new system TZ? Should be a short clarification.
- [x] F. **Scope suppression completeness** — **Critical C1**: window title, input, process, clipboard, file-access, audio capture, focus analyzer, coaching, cross-device sync all un-gated per §3.8 as drafted.
- [x] G. **Consent interaction** — **Gap I3**: not documented. Composition rule incomplete.
- [x] H. **Upload defer mechanics** — semantic **Critical C2**: rule is correct in isolation but inconsistent with C1's un-gated event loops. Also minor: no explicit FIFO-on-exit statement; no pre-window cap handling.
- [x] I. **Multi-select UX** — existing UX correctly characterized (§5.3 footnote about TimelineView.tsx:1-150 being unrelated is good). **Gap I4**: "Select all" scope unclear. **Gap I5**: remove-tag popover content deferred. **Minor M3**: a11y not addressed.
- [x] J. **Autostart UX** — reasonable. **Gap I6**: first-run prompt + repair UI not firmly scoped.
- [x] K. **Rejected options coverage** — "Blackout" ✔, "Quiet Hours" ✔, "RRULE" ✔ (via simplified Slack-like shape §3.6), "Cron" not explicitly rejected but the Slack-shape rationale implies it, "Silent pause" ✔ (§3.11 keeps notifications ON), "Pre-window buffer flush" ✔ (§3.9). **Missing M7**: preset schedules.
- [x] L. **Accessibility** — **Minor M3**: ARIA coverage gap on multi-select.

---

## Notes / observations (not findings)

- The spec's self-assessment of "decisions most likely to draw reviewer scrutiny" (§7 last paragraph) correctly flags D5 Wayland and D8-alt rename as the subtle ones but MISSES the scope-enumeration weakness (C1). Author should treat C1 as the headline change for Loop 2.
- `BatchTagResponse.tagged_count` rename (D8-alt) is flagged as "local break" — verified via `docs/contracts/oneshim-web.v1.openapi.yaml:1375` where the response schema is `GenericObject`, so the OpenAPI contract is untyped and the rename is formally non-breaking at the contract level. The ONLY consumer is `crates/oneshim-web/frontend/src/api/client.ts:579-588`. The rename is safe. D8-alt **approve**.
- `chrono-tz` addition (Q1) — adding +~2.1MB to installer is non-trivial for enterprise MDM-managed Windows installs; but the alternative (UTC-offset only) is a UX regression and breaks DST semantics the spec needs. Weight toward **approve Q1 = yes**. Consider feature-gating (`tracking-schedule-iana-tz`) if the binary-size pressure is real — but a Phase 9 feature-flag is more surface than it's worth; recommend unconditional add.
- Peer-product indicator patterns cross-check: Slack shows a moon icon in tray during DND; RescueTime shows a pause banner; Apple Screen Time shows a "Downtime" pill in Control Center. Spec §3.11 tray-tooltip strategy is weaker than Slack/RescueTime. If art-asset churn is the blocker, a text-based tray menu entry ("Tracking scheduled — resumes 13:00") is the industry-minimum.
- The spec correctly avoids the server-side enforcement trap (§3.2 non-goal + §8.1). Client-only gating is GDPR-correct. Worth preserving as Loop-2 guidance.
- Q7 (art asset for tracking-scheduled tray icon) can likely be resolved with a text tooltip on the existing paused icon per §3.11 recommendation — no new asset needed for MVP.

---

## Summary of required spec changes for Loop 2

1. **C1**: Rewrite §3.8 gate-boundary table with ALL un-gated event loops enumerated. Add failure-mode integration test. Address audio/focus/coaching/cross-device loops.
2. **C2**: Explicit FIFO-exit semantics in §3.9. Prove no in-window events can enter the queue (depends on C1). Add long-window cap handling.
3. **C3**: Correct DST fall-back + spring-forward text in §3.7. Add integration tests. Optionally add UI warning for user configuring a window overlapping the skipped hour.
4. **I1**: Korean i18n uniformity sweep.
5. **I2**: Extend regulatory scope in §2.1.
6. **I3**: Document consent × schedule composition in §3.4 or new §3.4.a.
7. **I4**: Clarify "Select all" scope and cap size, new Decision D12-a.
8. **I5**: Promote Q8 to Decision D13.
9. **I6**: First-run autostart prompt decision + repair UI + error copy.
10. **M1–M7**: as listed above.

---

_End of Review 2._
