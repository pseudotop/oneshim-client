# v0.5.0 UX Polish: Coaching Settings Completion

## Status: REVIEWED (R1)

**Branch**: `feature/v050-ux-completion`

---

## Context

Deep code analysis revealed that most previously-identified "gaps" are **already fully implemented**:
- Onboarding overlay: ✅ Complete (4-step page, IPC, E2E tests)
- Coaching explanation UI: ✅ Complete (expandable `<details>` in CoachingPopup.tsx)
- Goal setting: ✅ Complete (CoachingGoalsTab + CRUD + progress bars)
- Blackout hours enforcement: ✅ Backend complete (quiet_hours guard in evaluate())

**Actual remaining gap**: The "Coaching" settings tab (`CoachingGoalsTab.tsx`) only manages regime goals. Three other user-facing coaching config options lack UI:

| Config Field | Type | Default | Current UI |
|-------------|------|---------|------------|
| `quiet_hours: Vec<TimeRange>` | Time ranges (HH:MM → HH:MM) | Empty (no quiet hours) | **None** |
| `tone: CoachingTone` | Enum: Direct / Gentle / DataDriven | Gentle | **None** |
| `profiles: HashMap<String, ProfileConfig>` | Per-profile enable + interval | All enabled, 300s interval | **None** |

---

## Design

### Expand CoachingGoalsTab → CoachingSettingsTab

Rename and expand the existing `CoachingGoalsTab.tsx` (121 lines) to include three new sections below the existing goals section:

#### Section 1: Quiet Hours

Time range inputs for coaching suppression:
- List of existing quiet hour ranges (e.g., "22:00 – 06:00")
- Add button: two time inputs (start HH:MM, end HH:MM) + "Add" button
- Delete button per range
- Supports overnight ranges (e.g., 22:00 → 06:00)
- Saved via existing PATCH `/api/settings` endpoint (coaching.quiet_hours field)

#### Section 2: Coaching Tone

Radio group or select dropdown:
- Direct — "Short, actionable statements"
- Gentle — "Softer, encouraging language" (default)
- DataDriven — "Statistics-focused with numbers"
- Saved via PATCH `/api/settings` (coaching.tone field)

#### Section 3: Profile Toggle

Toggle rows for each coaching profile:
- FocusGuard (enabled/disabled)
- TimeAware (enabled/disabled)
- DeepWorkCoach (enabled/disabled)
- ContextRestore (enabled/disabled)
- GoalTracker (enabled/disabled)
- Each row: profile name + description + toggle switch
- Saved via PATCH `/api/settings` (coaching.profiles field)

### Backend Changes

**One change required:** The `PATCH /api/settings` handler in `src-tauri/src/commands/settings.rs` uses an `ALLOWED_KEYS` whitelist. Currently `"coaching"` is **NOT** in this list. Add it:

In `src-tauri/src/commands/settings.rs` (~line 30-43), add `"coaching"` to the `ALLOWED_KEYS` array.

No new endpoints or ports needed — the existing settings PATCH flow handles deep-merge automatically once the key is whitelisted.

### API Contract

No new endpoints. The existing settings flow:
1. `GET /api/settings` → returns full AppConfig (including coaching section)
2. `PATCH /api/settings` with `{ "coaching": { "tone": "Direct" } }` → deep-merges into config

### Tab Registration

In `Settings.tsx`, rename the tab:
```typescript
{ id: 'coaching', label: t('settings.tabs.coaching', 'Coaching') }
```

The tab ID stays `'coaching'` — no routing change.

### i18n Keys

Add to all 5 locale files:

```json
"coaching": {
    "quietHours": "Quiet Hours",
    "quietHoursDesc": "Coaching messages are suppressed during these time ranges.",
    "addQuietHour": "Add Time Range",
    "start": "Start",
    "end": "End",
    "tone": "Coaching Tone",
    "toneDesc": "Choose how coaching messages are worded.",
    "toneDirect": "Direct",
    "toneDirectDesc": "Short, actionable statements",
    "toneGentle": "Gentle",
    "toneGentleDesc": "Softer, encouraging language",
    "toneDataDriven": "Data-Driven",
    "toneDataDrivenDesc": "Statistics-focused with numbers",
    "profiles": "Coaching Profiles",
    "profilesDesc": "Enable or disable individual coaching profiles.",
    "profileFocusGuard": "Focus Guard",
    "profileFocusGuardDesc": "Alerts on context switching and focus loss",
    "profileTimeAware": "Time Aware",
    "profileTimeAwareDesc": "Reminders about work duration and breaks",
    "profileDeepWorkCoach": "Deep Work Coach",
    "profileDeepWorkCoachDesc": "Encourages sustained focus blocks",
    "profileContextRestore": "Context Restore",
    "profileContextRestoreDesc": "Helps resume work after interruptions",
    "profileGoalTracker": "Goal Tracker",
    "profileGoalTrackerDesc": "Tracks progress toward regime time goals"
}
```

---

## Affected Files

| File | Changes |
|------|---------|
| `crates/oneshim-web/frontend/src/pages/setting-tabs/CoachingGoalsTab.tsx` | Rename to `CoachingSettingsTab.tsx`, expand with 3 new sections |
| `crates/oneshim-web/frontend/src/pages/Settings.tsx` | Update lazy import + tab label |
| `src-tauri/src/commands/settings.rs` | Add `"coaching"` to ALLOWED_KEYS whitelist |
| `crates/oneshim-web/frontend/src/api/contracts.ts` | Add CoachingConfig TypeScript type (if missing) |
| 5 locale files (en/ko/ja/es/zh-CN) | Add coaching settings i18n keys |

## Estimated Tests: ~2-3 (frontend component tests)

## Estimated Impact
- **New files**: 0 (rename + expand existing)
- **Modified files**: ~8 (1 component + settings + 5 locales + contracts)
- **Lines added**: ~200-250
- **Backend changes**: 0

---

## Review History

### R1 (2026-04-06)

Previous gap analysis over-reported missing features based on roadmap TODOs without verifying code. Code-level verification confirmed:
- Onboarding, coaching explanation, goal setting, blackout enforcement are all complete
- Only the coaching settings UI for quiet_hours/tone/profiles was missing
- No backend changes needed — settings PATCH endpoint already supports all fields
