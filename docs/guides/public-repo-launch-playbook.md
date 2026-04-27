[English](./public-repo-launch-playbook.md) | [한국어](./public-repo-launch-playbook.ko.md)

# Public Repo Launch Playbook

This playbook defines a safe process for publishing Maekon as an open-source project without rewriting private/internal history.

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
2. Release artifacts are validated for all target platforms.
3. Known P0 issues are zero.
4. Latest QA run evidence and workflow pages are up to date.

## Open Operational TODOs

These items are outside the Rust client codebase and can be completed without a
source change. They are tracked here so they do not block the current client
cleanup, and this playbook is excluded from the public export.

- **Landing deployment, deferred**: no landing implementation exists yet, so do
  not connect `maekon.dev` to a placeholder web host. Keep DNS ready but leave
  the apex web target unset until the landing surface is selected. When it is
  ready, connect `maekon.dev` to the public landing host, redirect
  `www.maekon.dev` to the apex host, and move SSL/TLS to Full (strict) once the
  origin presents a valid certificate.
- **Human contact email, complete enough for launch prep**: keep
  `support@maekon.dev` and `security@maekon.dev` on Cloudflare Email Routing.
  Keep catch-all disabled. This is sufficient for README/SECURITY/GitHub
  security contact readiness before transactional product email exists.
- **Transactional email, deferred**: do not configure Resend outbound now.
  Reason: at the time of this decision, Resend's public pricing lists the Free
  plan with 1 domain and Pro with 10 domains at $20/mo; the current Free team
  already uses its one custom domain for `thengd.com`; creating a new team is a
  paid feature in the dashboard; and creating/using another account only to
  bypass domain quotas conflicts with Resend's acceptable-use language on quota
  circumvention. Also, Maekon does not yet require automated outbound mail for
  public repository bootstrap. Setting up outbound now would add provider
  credentials, DNS state, and deliverability maintenance before there is a
  product workflow that uses them. Revisit when automated email is genuinely
  needed. At that point choose one of: upgrade the existing Resend team and add
  `mail.maekon.dev`, replace `thengd.com` if it no longer needs Resend, or
  select a different outbound provider. If Resend is selected, reserve
  `noreply@mail.maekon.dev` and `releases@mail.maekon.dev`, add SPF/DKIM/DMARC
  records in Cloudflare, and issue a Sending-access API key restricted to
  `mail.maekon.dev`.
  References: [Resend pricing](https://resend.com/pricing),
  [Resend acceptable use](https://resend.com/legal/acceptable-use), and
  [Resend API key permissions](https://resend.com/docs/dashboard/api-keys/introduction).
- **Inbound automation, later**: add `reply.maekon.dev` for Resend inbound
  webhooks only if the product needs email replies to become app events.

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
- high-confidence internal text references such as local absolute paths,
  generated assistant-review markers, and private test bundle references.

The gate is not a substitute for release review. Before pushing, still inspect
the exported diff, run tests in the exported tree, and review public docs for
broken links caused by excluded internal planning artifacts.

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
3. verify diff and CI on the public repo;
4. push with a clear release note.
