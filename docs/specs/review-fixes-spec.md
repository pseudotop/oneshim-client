# Review Fixes Spec — CRITICAL + IMPORTANT Issues

**Date**: 2026-04-05

## Fixes

### C1. `verify_update_integrity` misleading return
- **Fix**: Rename to `preview_update_availability`. Change doc comment. Return `verified: false` fields to indicate no actual verification was performed.
- **File**: `src-tauri/src/updater/mod.rs`, `src-tauri/src/commands/system.rs`

### C2. Policies i18n keys missing (5 locales)
- **Fix**: Add `"policies"` section to all 5 locale JSONs (en, ko, ja, zh-CN, es) with ~15 keys
- **Files**: `i18n/locales/*.json`

### C3. Policies ActivityBar nav entry missing
- **Fix**: Add nav item to `ActivityBar.tsx` with Shield icon, route `/policies`
- **File**: `components/shell/ActivityBar.tsx`

### I4. HLC clock drift cap
- **Fix**: In `Hlc::merge()`, reject remote `wall_ms` exceeding `now + MAX_DRIFT` (1 hour). Log warning, use local time instead.
- **File**: `crates/oneshim-core/src/sync/hlc.rs` (or wherever Hlc is defined)

### I5. Policy Auto + requires_sudo guard
- **Fix**: In `create_execution_policy` and `update_execution_policy` handlers, reject policies where `confirmation == Auto && requires_sudo == true`
- **File**: `crates/oneshim-web/src/handlers/automation/execution.rs`

### I6. Policy input validation
- **Fix**: Add validation: `policy_id` max 256 chars alphanumeric+dash+underscore, `process_name` max 256 chars, `max_execution_time_ms` range 100..3_600_000
- **File**: `crates/oneshim-web/src/handlers/automation/execution.rs`

### I7. feedback_retries 7-day orphan cleanup
- **Fix**: Add `DELETE FROM feedback_retries WHERE created_at < datetime('now', '-7 days')` in maintenance loop, once per cycle
- **File**: `src-tauri/src/scheduler/loops/suggestions.rs`

### I8. SuggestionStats overlay i18n
- **Fix**: Add `useTranslation()` import, replace all hardcoded strings with `t()` calls. Add keys to 5 locales.
- **File**: `overlay/components/SuggestionStats.tsx`, `i18n/locales/*.json`

### I9. SuggestionsPanel source filter labels i18n
- **Fix**: Add `t()` for "Server", "Local", "Rules" labels. Add keys to 5 locales.
- **File**: `overlay/components/SuggestionsPanel.tsx`, `i18n/locales/*.json`

### I10. Policies checkbox → Checkbox component
- **Fix**: Replace raw `<input type="checkbox">` with project's `Checkbox` component
- **File**: `pages/policies/index.tsx`
