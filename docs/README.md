[English](./README.md) | [한국어](./README.ko.md)

# Documentation Index

This directory is organized by document intent.

## Top-level docs

- [DOCUMENTATION_POLICY.md](./DOCUMENTATION_POLICY.md): documentation conventions and maintenance rules
- [STATUS.md](./STATUS.md): mutable quality metrics single source of truth
- [install.md](./install.md): installation guide

## Directories

- `architecture/`: ADR-only architectural decisions
- `research/`: exploratory and investigational notes
- `guides/`: operator/developer playbooks, runbooks, and how-to guides
- `plan/`: dated implementation plans and execution tracking docs
- `contracts/`: versioned API/payload contracts and generated OpenAPI snapshots
- `crates/`: crate-level implementation references
- `migration/`: migration history and phased plans (with active/legacy classification in `migration/README.md`)
- `security/`: security baseline and integrity operations docs
- `qa/`: QA templates, execution run logs, and artifacts metadata

## Naming and placement quick rules

1. Use `ADR-XXX-*` naming only under `docs/architecture/`.
2. Put non-binding explorations under `docs/research/`.
3. Put procedural playbooks/runbooks under `docs/guides/` unless they are security-specific (`docs/security/`).
4. Put implementation plans under `docs/plan/` using `YYYY-MM-DD-*.md` (+ `.ko.md` companion for key plans).
5. Keep English-primary docs and maintain Korean companion docs for key public docs.
