[English](./public-repo-launch-playbook.md) | [한국어](./public-repo-launch-playbook.ko.md)

# Public Repo Launch Playbook

This playbook defines a safe process for publishing ONESHIM as an open-source project without rewriting private/internal history.

## Strategy

Use a **separate public history** generated from a curated snapshot.

- Keep the internal/private repository history unchanged.
- Export a public-ready tree snapshot from a vetted source ref.
- Start a new one-commit history in a separate directory/repository.
- Push that result to the public remote.

## Suggested Hook Copy

Use one consistent positioning line across README + repository description.

- **Hook line**: `From raw desktop activity to daily focus wins.`
- **Repository description candidate**: `Open-source desktop intelligence client that turns local work signals into a real-time focus timeline and actionable suggestions.`

## Preconditions (Go/No-Go)

1. CI is green (Rust + frontend build + E2E).
2. Release artifacts are validated for all target platforms.
3. Known P0 issues are zero.
4. `docs/STATUS.md` and latest QA run evidence are up to date.

## Export Procedure

```bash
# From the private/internal repository root
./scripts/export-public-repo.sh /tmp/oneshim-client-public <source-ref>

# Example
./scripts/export-public-repo.sh /tmp/oneshim-client-public codex/release-web-gates-qa-connected-hardening
```

The script:

1. archives `<source-ref>`;
2. removes paths listed in `scripts/public-repo-exclude.txt`;
3. initializes a fresh Git history with one initial commit.

## Publish Procedure

```bash
cd /tmp/oneshim-client-public
git remote add origin <public-repo-url>
git push -u origin main
```

## Update Cycle

For subsequent public updates:

1. prepare a new internal source ref;
2. rerun export into a fresh temp directory;
3. verify diff and CI on the public repo;
4. push with a clear release note.
