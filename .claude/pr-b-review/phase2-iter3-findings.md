# Phase 2 — Iteration 3 Plan Verification Findings

**Date**: 2026-04-25
**Plan under review**: plan v2 with Addendum (commit `05cb8051`)
**Reviewer**: 1 superpowers:code-reviewer subagent + direct codebase verification
**Verdict**: NOT READY → fixes applied → READY (pending iter-4 final verify)

---

## Iter-3 Findings Summary

**Addendum technical correctness**: ✅ ALL VERIFIED CORRECT
- A1 wire codes macro pattern matches `audio.rs:3-11` exactly
- A1 alphabetical position confirmed: `audio` < `autostart` < `auth` (since 'd' < 't')
- A3 integration test arch confirmed: `src-tauri/Cargo.toml` has `[[bin]]` only, no `[lib]`
- A4 Generic Runtime pattern matches `desktop_permissions.rs:43-45`
- A4 monitor.rs hook point in `handle_idle_tick` (line ~101) confirmed
- A4 scheduler/mod.rs already has `app_handle: Option<tauri::AppHandle>` (line 525-528)
- A5 DashboardLayout.tsx exists with `<Outlet>` at line 117 + warning comment
- A5 ConfigManager.get() returns owned `AppConfig` (verified `config_manager.rs:97`)
- A6 GeneralTab.tsx has invokeDesktop helper at lines 55-58

**NEW Critical issue (subagent-driven readiness)**:
- **C-NEW-1**: Task body steps still execute the WRONG procedure. Subagent reading step-by-step hits body first, may not find addendum. Specifically:
  - Step 4.1 body: tells subagent to append to expected.txt (wrong; needs enum first)
  - Step 4.6 body: shows IPC commands using string literals (correct API but not enum-based)
  - Step 5.2 body: tells subagent to create file using `oneshim_app::autostart` (won't compile)
  - Step 10.4 body: references fictional `SystemMonitorLoop` struct + assumes existing focus-block detection
  - Step 11.1 body: uses `tauri::test::mock_app()` (not available + runtime mismatch)
  - Step 12.4 body: invokes `get_app_config` (nonexistent IPC; should be `get_autostart_config`)
  - Step 12.5 body: modifies `Dashboard.tsx` (does not exist; should be `DashboardLayout.tsx`)

- **C-NEW-2**: Pre-Flight has no addendum-awareness step. Required-reading at A9 is INSIDE addendum — chicken-and-egg.

**3 Important issues**:
- I-NEW-1: A1 step 3 explicitness about insertion location (mentioned audio block; minor)
- I-NEW-2: A5 `.clone()` semantics — `ConfigManager.get()` returns owned, no `.clone()` needed
- I-NEW-3: A4 unwrap wording — could mislead. Now clarified in Step 10.4 supersession banner.

---

## Iter-3 Fixes Applied

### Fix 1 — Pre-Flight PF4 added with required-reading list
- New PF4 step at top of plan: "Read the Plan v2 Corrections Addendum FIRST (CRITICAL)"
- Lists 8 required source files to read before starting Phase 3
- Explicitly tells subagent: "addendum SUPERSEDES specific task body steps"

### Fix 2-7 — Inline ⚠ SUPERSEDED banners on affected steps

Each affected step body now starts with a clear banner:
- Step 4.1: ⚠ SUPERSEDED — see Addendum A1 + A2 (5-step summary inline)
- Step 4.6: ⚠ SUPERSEDED — see Addendum A2 + A5 (with corrected enum-based code shown)
- Step 5.2: ⚠ SUPERSEDED — see Addendum A3 (replacement explained)
- Step 10.4: ⚠ SUPERSEDED — see Addendum A4 (concrete monitor.rs procedure inline)
- Step 11.1: ⚠ SUPERSEDED — see Addendum A4 (closure-test pattern referenced)
- Step 12.4: ⚠ SUPERSEDED — see Addendum A5 + A6 (get_autostart_config + dynamic invoke)
- Step 12.5: ⚠ SUPERSEDED — see Addendum A5 (DashboardLayout host + Outlet placement)

Body content RETAINED for context but clearly marked as superseded. Subagent reading any step will see the banner FIRST and know to consult addendum.

### Fix 8 — File Structure header updated
- `Dashboard.tsx` → `DashboardLayout.tsx` (with note about `Dashboard.tsx` non-existence)

### Fix 9 — get_autostart_config IPC inline-shown in Step 4.6 body
- Now Step 4.6 body shows the corrected IPC commands using `AutostartCode::EnableFailed.as_str()` (etc.)
- Includes the new `get_autostart_config` command (6th command, per Addendum A5)
- Clarifies `get(&self) -> AppConfig` semantics (owned, no `.clone()` needed)

---

## Phase 2 Exit Criteria — Re-evaluation

- ✅ All Critical issues addressed (8 from iter-2 + 2 NEW from iter-3 = 10 total)
- ✅ All Important issues addressed (8 from iter-2 + 3 NEW from iter-3 = 11 total)
- ✅ Plan v2.5 has clear supersession banners → subagent cannot miss the addendum
- ✅ Pre-Flight PF4 ensures addendum is read first
- ✅ Required reading list moved to PF4 (no longer hidden inside addendum)
- ⏳ iter-4 verification: confirm fixes are sufficient for subagent-driven impl

---

## Iter-4 (next iteration) Plan

**Goals**:
1. Fresh subagent verifies plan v2.5 is now subagent-driven-ready
2. Specifically verify: do all SUPERSEDED banners point to correct addendum subsections?
3. Verify required-reading list in PF4 matches addendum's A9 (or remove A9 since now duplicate)
4. Confirm no other body sections still reference fictional types/files
5. If clean → advance to Phase 3 (subagent-driven-development)
6. If issues → iter-5

**Special focus**:
- A9 (required reading at addendum) is now duplicate with PF4. Either remove A9 or just note "see PF4"
- Step 10.4 still says "find where focus-block completion is detected" in body which body never does — banner explains this. Subagent should be OK now.

---

## Notes

The Addendum approach worked for documenting the corrections, but iter-3 confirmed that **inline supersession banners are critical** for subagent-driven workflow. The plan now has both:
- Detailed corrections (Addendum A1-A9)
- Pointed banners on each affected step body (so subagent never executes wrong instruction)

Total plan size: ~2200 lines (estimate; +200 from v2 due to banners).
