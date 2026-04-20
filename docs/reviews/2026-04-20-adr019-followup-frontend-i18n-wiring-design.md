# ADR-019 Follow-up #3 — Frontend i18n Wiring Design

**Date:** 2026-04-20
**Status:** ✅ SHIPPED (iter-205 core; iter-210/212 UI demos; iter-214 lefthook guard wired)
**Scope:** `crates/oneshim-web/frontend/src/i18n/` (or equivalent i18n module), error-facing UI components, translation resource files
**Origin:** ADR-019 §Known follow-ups #3 — "Frontend i18n wiring"
**Parent ADR:** [ADR-019](../architecture/ADR-019-error-code-infrastructure.md)
**Dependency:** Follow-up #1 (IpcError DTO) — frontend must see `{code, message}` not free string
**Target version:** Frontend-side release (no Rust change)

> **Shipped beyond the design:** the snapshot-driven coverage-parity test (Vitest reads `wire_contract_snapshot.expected.txt` directly) was an addition to the original design's build-time check — it gives the same fail-fast guarantee via the existing `pnpm test` run without needing a separate CI stage. Also added: `lefthook.yml` pre-commit hook (iter-214) that runs `scripts/check-wire-error-i18n-coverage.sh` (~3ms) on any staged change to the 3 source-of-truth files, catching drift before CI.

## Context

ADR-019 established a stable wire-code registry (41 codes — `config.invalid`, `network.timeout`, `provider.bedrock.unsupported`, etc.). These codes are globally unique, immutable, and machine-readable — exactly the shape a translation system wants as keys.

Follow-up #1 (IpcError DTO) makes the code available to the frontend at the IPC boundary. **This follow-up uses that code as an i18n translation key** so user-visible errors can be localized (ko/en) instead of showing the raw English Display string.

## Goal

Localize user-facing error messages by keying off `err.code`. Frontend renders:

```
// Instead of
"Configuration error [config.invalid]: bad value"

// Show
"설정 값이 올바르지 않습니다: bad value"  (ko)
"Invalid configuration: bad value"      (en)
```

## Decision

### 1. Translation resource structure

Create (or extend) `crates/oneshim-web/frontend/src/i18n/resources/errors.{en,ko}.json`:

```json
// errors.en.json
{
  "config.invalid": "Invalid configuration: {message}",
  "config.missing": "Missing configuration: {message}",
  "config.out_of_range": "Configuration value out of range: {message}",
  "network.timeout": "Request timed out",
  "network.rate_limit": "Too many requests — please wait and try again",
  "provider.bedrock.unsupported": "AWS Bedrock is not supported in this build",
  "internal.generic": "An unexpected error occurred: {message}",
  "internal.io": "Storage or network I/O failed: {message}",
  ...
}
```

```json
// errors.ko.json
{
  "config.invalid": "설정 값이 올바르지 않습니다: {message}",
  "config.missing": "필수 설정이 누락되었습니다: {message}",
  ...
}
```

**Coverage**: all 41 wire codes from `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`. A build-time script can diff the snapshot against the JSON keys to prevent drift.

### 2. Translation API

```typescript
// src/i18n/translateError.ts
import { type IpcError, isIpcError } from "@/api/desktop";
import enErrors from "./resources/errors.en.json";
import koErrors from "./resources/errors.ko.json";

const resources = { en: enErrors, ko: koErrors } as const;
type Locale = keyof typeof resources;

export function translateError(err: unknown, locale: Locale = "en"): string {
  if (!isIpcError(err)) {
    // Fallback for non-IpcError exceptions (network layer failures, etc.)
    return err instanceof Error ? err.message : String(err);
  }
  const template = resources[locale]?.[err.code]
    ?? resources.en[err.code]  // graceful fallback to English
    ?? err.message;            // ultimate fallback: raw message
  return template.replace("{message}", err.message);
}
```

### 3. Component integration

Replace existing `error.message` usages at user-visible surfaces:

```typescript
// Before
.catch((err) => setError(err instanceof Error ? err.message : String(err)))

// After
.catch((err) => setError(translateError(err, currentLocale)))
```

Components to update (survey):

```bash
rg -n 'error\.message|String\(err' crates/oneshim-web/frontend/src/components/ \
  crates/oneshim-web/frontend/src/pages/
```

Estimated 30-50 user-visible error display sites.

### 4. Missing-key detection

Build-time check (Vite plugin or tsx script):

```typescript
// scripts/check-error-i18n-coverage.ts
import fs from "fs";
import path from "path";
const snapshot = fs.readFileSync(
  "../../crates/oneshim-core/tests/wire_contract_snapshot.expected.txt",
  "utf-8"
).trim().split("\n");

for (const locale of ["en", "ko"]) {
  const resource = JSON.parse(
    fs.readFileSync(`src/i18n/resources/errors.${locale}.json`, "utf-8")
  );
  const missing = snapshot.filter(code => !(code in resource));
  if (missing.length > 0) {
    console.error(`Missing ${locale} translations for: ${missing.join(", ")}`);
    process.exit(1);
  }
}
```

Wire into `pnpm build` so CI catches regressions.

### 5. Test strategy

**Resource test** (`errors.test.ts`):
```typescript
test.each(["en", "ko"])("every wire code has a %s translation", (locale) => {
  const snapshot = /* read wire_contract_snapshot.expected.txt */;
  const resource = /* read errors.{locale}.json */;
  for (const code of snapshot) {
    expect(resource[code]).toBeDefined();
    expect(resource[code]).toContain("{message}");  // except for pure-state codes
  }
});
```

**Translation test**:
```typescript
test("translateError formats known code", () => {
  const err: IpcError = { code: "config.invalid", message: "x" };
  expect(translateError(err, "en")).toBe("Invalid configuration: x");
  expect(translateError(err, "ko")).toBe("설정 값이 올바르지 않습니다: x");
});

test("translateError falls back gracefully for unknown code", () => {
  const err: IpcError = { code: "novel.code", message: "raw" };
  expect(translateError(err, "ko")).toBe("raw");
});
```

## Consequences

### Positive
- User-visible errors are now localized instead of showing English Display strings to Korean users.
- Message quality improves (non-technical phrasing for UI vs technical phrasing for logs).
- Adding a new error code requires adding translations — regression prevented by build-time coverage check.

### Negative
- Adding any new wire code now requires updating `errors.en.json` + `errors.ko.json` in addition to the Rust code.
- Translation drift possible if ko messages aren't updated alongside en (build check mitigates).

### Neutral
- Depends on Follow-up #1 — if the frontend still receives free-form strings, this pattern has nowhere to key off.

## Implementation Plan

1. **PR1** (infrastructure): `translateError` + resource JSON skeleton + build-time coverage check. ~2 hours.
2. **PR2** (translations): populate all 41 en/ko translation pairs. ~2 hours (copywriting is the bottleneck).
3. **PR3-5** (component migration): 30-50 user-visible sites, ~1 hour per 10 sites. ~4-5 hours.

**Total effort estimate:** ~1 day.

> **Post-execution reality:** Items 1+2 shipped together in iter-205 (single commit, combined the infra + all 41×2-locale translations since the coverage-parity test gates them together). Item 3 is partially shipped as demonstrations — 2 UI sites integrated (iter-210 `GeneralTab::SupportToolsCard` translateError-as-primary, iter-212 `BugReportWizard::handleExport` translateError-as-detail-suffix) rather than the estimated 30-50. The remaining 28-48 sites are *piecemeal follow-up-free*: the snapshot-driven parity tests prevent Rust-↔-TS drift regardless of how many UI catch-sites migrate, so future `.catch((err) => err.message)` call-sites can adopt the pattern on demand without needing a coordinated sweep. iter-214 added a lefthook pre-commit hook wiring the coverage script — not part of the original design but caught en/ko drift faster than the Vitest parity test alone.

**Hard dependency**: Follow-up #1 must land first. Without `IpcError`, the frontend has no `code` field to key on.

## Out of Scope

- Server-side error localization (handled by server's own i18n layer).
- Locale detection (assumed to already exist in the i18n provider).
- Accessibility announcement of errors (ARIA live regions) — separate UX concern.
