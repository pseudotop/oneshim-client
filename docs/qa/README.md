# QA Result Storage Policy

This directory is the canonical location for manual/reviewed QA execution results.

## What goes where

- QA criteria/checklists (stable definitions):
  - `docs/guides/uiux-qa-sheet.md`
  - `docs/guides/replay-uiux-qa-sheet.md`
- Actual QA run outputs (mutable run evidence):
  - `docs/qa/runs/*.md`
- CI machine artifacts (screenshots, Playwright HTML, logs):
  - CI artifact store (GitHub Actions artifacts), linked from run files.
- Project-level mutable quality summary:
  - `docs/STATUS.md` (single source of truth summary), with links to latest QA run.

## Execution Policy

- Interactive UI/UX QA must be executed with Playwright CLI, not Playwright MCP.
- Preferred interactive commands:
  - `cd crates/oneshim-web/frontend && pnpm qa:pwcli:open`
  - `cd crates/oneshim-web/frontend && pnpm qa:pwcli:snapshot`
  - `cd crates/oneshim-web/frontend && pnpm qa:pwcli:show`
- Attach run evidence (CLI output summary + artifact/report link) to `docs/qa/runs/*.md`.

## Naming convention for run files

Use date-first filenames for chronological sorting:

- `YYYY-MM-DD-uiux-qa-rcN.md`
- `YYYY-MM-DD-replay-qa-rcN.md`

Examples:

- `2026-02-23-uiux-qa-rc1.md`
- `2026-02-23-replay-qa-rc1.md`

## Minimum fields per QA run

- Build/commit under test
- Scope (pages/features)
- Checklist version used
- P0/P1/P2 findings
- Pass/Partial/Fail table with evidence links
- Decision (go/no-go) and owner
- Follow-up tickets/commits
