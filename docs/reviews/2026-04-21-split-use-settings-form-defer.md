# Split 2: `useSettingsForm.ts` — Deferred to Dedicated Frontend Session

**Date**: 2026-04-21
**Parent**: [`2026-04-21-p2-large-files-triage.md`](2026-04-21-p2-large-files-triage.md) #2 (must-split)
**Status**: **Deferred** — full plan captured for future session

## Why deferred

Triage doc #465 estimated ~1 day for full 10-concern split. After analysis, confirmed the estimate is correct — this is a substantial frontend refactor that belongs in a dedicated TS/React session rather than the current Rust-focused P2 cleanup stream.

The file (984 LOC) has tight coupling across:
- React state (4 `useState` calls for model catalog alone)
- `formDataRef` (written from many handlers)
- Cross-cutting callbacks (`showToast`, `t`, `queryClient`, `handleExternalApiChange`)
- Mutations (save / update / notification) that read `formData` synchronously
- i18n strings passed into ~20 handler bodies

A lightweight 1-hook extraction (e.g., just `useModelCatalog`) would shave ~30 LOC but wouldn't address the real SOLID violation. Doing it half-way would leave the file in a worse state than leaving it whole.

## Full extraction plan (for future frontend session)

Target structure:

```
crates/oneshim-web/frontend/src/pages/hooks/
  useSettingsForm.ts              # composition root (~200 LOC)
  useSettingsFormState.ts         # formData, hasUnsavedChanges, revert, formDataRef
  useSettingsMutations.ts         # save / update / notification mutations
  useSettingsExport.ts            # export state + handler
  useModelDiscovery.ts            # modelCatalog* state + discoverModels / canDiscover
  useSettingsHandlers.ts          # root, notification, telemetry, monitor, privacy, schedule, update, automation, sandbox handlers
  useAiProviderProfiles.ts        # profile CRUD + handleSelectAiProviderProfile
```

### Extraction order (lowest risk first)

1. **`useSettingsExport`** — self-contained: state + 1 handler. ~30 LOC extracted. Dependencies: `showToast`, `t`, `exportData`, `downloadBlob`.
2. **`useSettingsMutations`** — self-contained: `useMutation` wrappers. ~80 LOC. Dependencies: `queryClient`, `showToast`, `t`, `formData` (read-only).
3. **`useModelDiscovery`** — medium coupling: 4 state vars + 3 handlers. ~120 LOC. Dependencies: `formData`, `ocrSurface` (derived), `handleExternalApiChange` (callback — must be provided by composition root before handlers extracted), `showToast`, `t`.
4. **`useAiProviderProfiles`** — 3 handlers over `saved_profiles` array. ~100 LOC. Dependencies: `formData`, `setFormData`, `showToast`, `t`.
5. **`useSettingsHandlers`** — the big one: 11+ `handleXChange` functions. ~300 LOC. Dependencies: `setFormData`, `formDataRef`, and handlers from step 3 + 4.
6. **`useSettingsFormState`** — final extraction: `formData`, revert logic, `hasUnsavedChanges`, `saveDisabled`. ~100 LOC. Everything else now depends on this.

### Composition root shape

```typescript
export function useSettingsForm(data: SettingsDataResult): SettingsFormResult {
  const state = useSettingsFormState(data);
  const mutations = useSettingsMutations(state.formData, state.setFormData);
  const exp = useSettingsExport();
  const discovery = useModelDiscovery(state.formData, state.formDataRef, /*handleExternalApiChange*/);
  const profiles = useAiProviderProfiles(state.formData, state.setFormData);
  const handlers = useSettingsHandlers(state, profiles, discovery);

  return { ...state, ...mutations, ...exp, ...discovery, ...profiles, ...handlers, ...composite };
}
```

### Risks (for future session)

1. **Circular dependency**: `useSettingsHandlers` needs `discovery.discoverModels`, but `useModelDiscovery` also calls `handleExternalApiChange` which is inside `useSettingsHandlers`. Must break the cycle by passing `onModelDiscovered` callback from composition root.
2. **`formDataRef` currency**: many handlers read `formDataRef.current` to avoid stale closures. All extracted hooks must share the same ref. Pass ref from state to each hook.
3. **Biome lint churn**: `useExhaustiveDependencies` will flag every `useCallback` without a full dep array. Each extracted hook needs audit.
4. **Test coverage**: no existing unit tests for `useSettingsForm.ts` — adding tests as part of extraction would add 1+ day to scope. Alternative: keep tests at the `SettingsPage` integration level.

### Acceptance (when executed)

- `useSettingsForm.ts` ≤ 300 LOC
- Each extracted hook ≤ 350 LOC (per large-files triage policy)
- `pnpm lint` + `pnpm typecheck` + `pnpm test` pass
- `SettingsPage` e2e behavior unchanged (verify via Playwright)

## Recommendation

Execute this split in a dedicated **frontend-focused session** where:
1. Playwright e2e suite is available for behavior verification
2. Vitest + react-testing-library are primary test tools
3. Session has ~1 day uninterrupted for careful ref/callback threading

For now, the file stays at 984 LOC. The triage doc's "must-split" classification remains valid — this is a watch-item, not blocked-on-anything.

## Follow-up trigger

If `useSettingsForm.ts` grows beyond 1200 LOC before the split lands, escalate: the growth rate suggests concerns are accumulating and the SOLID violation is worsening.

## Related

- [PR #465](https://github.com/pseudotop/oneshim-client/pull/465): large-files triage doc that scoped this work
- [PR #471](https://github.com/pseudotop/oneshim-client/pull/471): Split 1 (`app_runtime_launch.rs`) — the Rust companion must-split, which landed.
