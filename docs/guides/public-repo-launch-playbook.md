[English](./public-repo-launch-playbook.md) | [한국어](./public-repo-launch-playbook.ko.md)

# Public Repo Launch Playbook

This playbook defines a safe process for publishing Maekon as an open-source project without rewriting private/internal history.

**Internal-only**: this launch playbook is for maintainers preparing the public
repository. It is excluded from the public-minimal export. Public users should
see release, install, security, and contribution docs instead.

For the broader SSOT/export/managed-platform strategy, see
`docs/plan/2026-04-30-maekon-client-public-oss-strategy.md`.

## Strategy

Use a **separate public history** generated from a curated snapshot.

- Keep the internal/private repository history unchanged.
- Export a public-ready tree snapshot from a vetted source ref.
- Start a new one-commit history in a separate directory/repository.
- Push that result to the public remote.
- Treat the public repository as an export target, not a second development
  source of truth.

The export profile is intentionally **public-minimal**: source code, build
metadata, install/release docs, security docs, architecture ADRs, API contracts,
crate references, and public guides are exported. Session plans, sprint review
artifacts, exploratory research, roadmap/spec drafts, private test bundles, and
environment files are excluded.

One runtime data exception is required: `specs/providers/provider-surface-catalog.json`
must stay in the public tree because `oneshim-core` embeds it at compile time.

## Suggested Hook Copy

Use one consistent positioning line across README + repository description.

- **Hook line**: `From raw desktop activity to daily focus wins.`
- **Repository description candidate**: `Open-source desktop intelligence client that turns local work signals into a real-time focus timeline and actionable suggestions.`

## Preconditions (Go/No-Go)

1. CI is green (Rust + frontend build + E2E).
2. Public release artifacts are validated for the current shipped platform
   scope. Linux downloads stay out of the public release surface while the
   documented `glib 0.18.x` runtime advisory exception remains active.
3. Known P0 issues are zero.
4. Latest QA run evidence and workflow pages are up to date.

## Export Procedure

```bash
# From the private/internal repository root
./scripts/export-public-repo.sh /tmp/maekon-client-public <source-ref>

# Example
./scripts/export-public-repo.sh /tmp/maekon-client-public codex/release-web-gates-qa-connected-hardening

# Smoke the current working tree before committing
./scripts/export-public-repo.sh --dry-run --worktree
```

The script:

1. archives `<source-ref>`;
2. removes paths listed in `scripts/public-repo-exclude.txt`;
3. validates required public paths and forbidden internal paths;
4. runs a high-confidence internal-reference scan;
5. initializes a fresh Git history with one initial commit.

Use `--no-commit` when a downstream tool wants the exported tree without a fresh
Git history.

## Export Gate Coverage

The built-in gate blocks the edge cases that are most likely to leak private
context or break public builds:

- forbidden internal planning, review, research, roadmap, migration, and private
  validation directories;
- parent-monorepo directories such as `server/`, `backoffice/`, and `terraform/`;
- local environment and agent-tooling files;
- accidental removal of required public/runtime files, including the provider
  surface catalog;
- accidental removal of the public Dependabot config, plus accidental inclusion
  of internal Dependabot auto-merge automation or generated SBOM artifacts;
- high-confidence internal text references such as local absolute paths,
  generated assistant-review markers, and private test bundle references.

The gate is not a substitute for release review. Before pushing, still inspect
the exported diff, run tests in the exported tree, and review public docs for
broken links caused by excluded internal planning artifacts.

## Dependency Update Policy

Keep public Dependabot enabled. Public dependency PRs are part of the open
source trust surface: they make dependency drift visible, let contributors
review the same signals, and give maintainers a public audit trail.

Handle those PRs with a path-aware triage rule:

- mirrored source and dependency paths, such as `Cargo.toml`, `Cargo.lock`, Rust
  source, and exported workflows, are replayed in the parent/client SSOT tree
  first; after private/full CI and the export gate pass, regenerate the public
  tree and close or supersede the public PR with a link to the upstream change;
- public-only paths, such as public repository metadata, public issue templates,
  and explicitly public overlay files, may be adjusted on the public side and
  then folded back into the export overlay/tooling when they should persist;
- security-critical fixes may use a maintainer exception for speed, but the
  SSOT replay should follow immediately so the public repo does not become a
  permanent second source of truth.

For public repo settings, keep Dependabot security alerts and version-update
PRs enabled, but do not enable public auto-merge for mirrored dependency paths.
The public-minimal export keeps `.github/dependabot.yml` available for
visibility while excluding the internal Dependabot auto-merge workflow.

## Publish Procedure

```bash
cd /tmp/maekon-client-public
git remote add origin <public-repo-url>
git push -u origin main
```

## Update Cycle

For subsequent public updates:

1. prepare a new internal source ref;
2. rerun export into a fresh temp directory;
3. verify that public Dependabot config is present and internal auto-merge
   automation is not present in the export;
4. verify diff and CI on the public repo;
5. push with a clear release note.
