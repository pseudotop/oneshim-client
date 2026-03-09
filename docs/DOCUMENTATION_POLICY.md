[English](./DOCUMENTATION_POLICY.md) | [한국어](./DOCUMENTATION_POLICY.ko.md)

# Documentation Policy

## Language Policy

- Public-facing documentation in this repository is **English-primary**.
- Multilingual companion docs are maintained for key public guides.
- **Supported languages**: English (primary), 한국어 (ko), 日本語 (ja), 简体中文 (zh-CN), Español (es)
- This includes `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, and docs entry pages.
- Companion docs should be kept semantically aligned with English primary docs.
- Naming convention: `{filename}.{lang-code}.md` (e.g., `README.ja.md`, `README.zh-CN.md`)

## Metrics Consistency Policy

- Mutable quality metrics (test counts, lint/build pass state) must be maintained only in [STATUS.md](./STATUS.md).
- Other documents must reference `docs/STATUS.md` instead of duplicating live numbers.

## Directory Structure Policy

- `docs/architecture/` is ADR-only.
  - File naming: `ADR-XXX-*.md` and `ADR-XXX-*.ko.md`
  - Non-ADR notes (research, playbooks, runbooks) must not be stored in this directory.
- `docs/research/` stores exploratory and investigational documents.
- `docs/guides/` stores operator/developer playbooks and how-to/runbook style documents.
- `docs/plan/` stores dated implementation plans and execution tracking notes.
  - File naming: `YYYY-MM-DD-*.md` and `YYYY-MM-DD-*.ko.md` for key plans.
- `docs/contracts/` stores versioned payload/API contracts and generated OpenAPI snapshots.
- `docs/crates/` stores per-crate implementation references.
- `docs/migration/` stores migration history and archive-oriented phase documents. Keep `README` files current, and classify sub-docs as active vs legacy in the migration index.
- `docs/security/` stores security baseline and integrity operations docs.
- `docs/qa/` stores QA templates and run artifacts metadata.

See [docs/README.md](./README.md) for the current document map.

## Why

- Prevent contradictory numbers across docs.
- Reduce maintenance overhead.
- Keep onboarding and release communication consistent.

## Companion Docs

- Korean companion: [DOCUMENTATION_POLICY.ko.md](./DOCUMENTATION_POLICY.ko.md)
- Status companion: [STATUS.ko.md](./STATUS.ko.md)
