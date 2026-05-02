[English](./2026-04-30-maekon-client-public-oss-strategy.md) | [한국어](./2026-04-30-maekon-client-public-oss-strategy.ko.md)

# Maekon Client Public OSS Strategy

**Date**: 2026-04-30
**Status**: Draft
**Scope**: Maekon Client public strategy, parent repo integration strategy, public export operations
**Related**: `docs/guides/public-repo-launch-playbook.md`, `scripts/export-public-repo.sh`, `scripts/public-repo-exclude.txt`

## Purpose

Maekon Client will be published as open source to build trust. Users should be able to install a public release client and optionally connect it to ONESHIM Platform.

This document defines how to operate Maekon Client like a real public open-source project while keeping parent-project server code, SaaS operations, private tests, roadmap drafts, infrastructure, and internal operating context private.

## Background

`client-rust` is currently managed as a separate repository/submodule. Long term, it should be integrated into the parent project, but a full root-level monorepo IA reset is not in scope for the current phase.

The intended operating model is partial disclosure, not pure OSS.

- Maekon Client is the public local client.
- ONESHIM Platform is the optional managed platform.
- The public repository provides trust and transparency.
- Paid/private value lives in team operations, sync, policy, audit, managed infrastructure, and enterprise support.

## Decisions

### 1. Keep the canonical source inside the parent repo

After parent integration, Maekon Client should target this canonical source path:

```text
clients/
  maekon-client/
```

Do not use `apps/maekon-client` in the current phase. Adding a root-level `apps/` directory would imply a broader decision about whether server, backoffice, and docs-site should also move into the same IA. That is beyond the intended scope.

`clients/maekon-client` expresses a client-centric SSOT without forcing the whole parent repo to reorganize.

### 2. Treat the public repo as an export target, not a second source

`pseudotop/maekon-client` is public, but it is not the development source of truth. Treat it as a curated export target generated from vetted internal source.

```text
private parent repo
  clients/maekon-client/          # SSOT

public GitHub repo
  pseudotop/maekon-client         # generated/exported public surface
```

Issues or external PRs discovered in the public repo should be applied to the parent SSOT first, then exported back to the public repo. Do not accumulate direct feature development in the public repo.

Public-only metadata, GitHub repository settings, and issue templates may be adjusted directly in the public repo. Repeated artifacts should move back into export overlays where practical.

### 3. Manage open-source operations as export policy, not a duplicate source tree

Do not create a tracked `opensource/maekon-client` copy. Duplicating the same source inside parent makes ownership ambiguous and increases stale-copy risk.

Instead, separate public operations like this:

```text
clients/
  maekon-client/                  # SSOT

tools/
  public-export/
    maekon-client/
      export.sh
      include.txt
      exclude.txt
      required-paths.txt
      forbidden-patterns.txt
      overlays/
        README.md
        SECURITY.md
        CONTRIBUTING.md
        .github/

.public-worktrees/
  maekon-client/                  # gitignored generated checkout
```

`tools/public-export/maekon-client` is the internal operations layer that defines what is public and what is excluded.

`.public-worktrees/maekon-client` is the local checkout or generated export tree used for verification. It must not be committed to the parent repo.

### 4. State the Maekon/ONESHIM relationship clearly

Use this relationship statement consistently:

> Maekon Client is the transparent, open-source local client. ONESHIM Platform is the optional managed platform for sync, teams, automation governance, and enterprise operations.

Korean companion wording:

> Maekon Client는 투명하게 공개되는 오픈소스 로컬 클라이언트입니다. ONESHIM Platform은 동기화, 팀 운영, 자동화 거버넌스, 엔터프라이즈 운영을 위한 선택적 관리형 플랫폼입니다.

The app, README, install docs, OAuth screens, login copy, update docs, and security docs should describe this relationship consistently.

### 5. Separate public and private scope

| Area | Public? | Rationale |
| --- | --- | --- |
| Maekon Client source | Public | Core of local-client trust |
| Install/release/security docs | Public | Users must verify binaries and update paths |
| Local API contract | Public | Needed to explain client integration boundaries |
| Parent server | Private | ONESHIM Platform internal implementation |
| SaaS operations/infra | Private | Deployment, operations, and security boundaries |
| Private tests | Private | May include internal scenarios or environments |
| Roadmap/spec drafts | Private | Avoid treating internal plans as public promises |
| docs/plan, docs/specs, docs/reviews | Private | Internal decision and review records |

`scripts/public-repo-exclude.txt` currently excludes `docs/plan`, `docs/specs`, `docs/reviews`, `docs/research`, `docs/roadmap`, `docs/migration`, and `tests/private`. This strategy document is under `docs/plan`, so it is not part of the public export.

### 6. Monetize operational value, not basic individual utility

The reason to open-source Maekon Client is trust. The public client must be useful after installation.

Public/free scope:

- Local capture and local analysis
- Local dashboard
- Basic settings and privacy controls
- Safe default flow for local automation
- Public install/build/security docs

Paid/private or managed-platform scope:

- ONESHIM account-based sync
- Team and organization policy
- Central audit logs and retention policy
- SSO/SCIM, RBAC, compliance export
- Managed LLM/OCR routing
- Enterprise support, SLA, managed updates

The initial strategy assumes an OSI-approved open-source license. Source-available or anti-free-riding licenses are not the default for this phase. Final license selection should be handled separately across Apache-2.0, MIT, and AGPL candidates.

## Landing/docs Operations

After Maekon is public, landing/docs should not be mixed with parent ONESHIM docs.

Recommended public surface:

```text
maekon.dev
  - Maekon Client introduction
  - Downloads
  - GitHub link
  - ONESHIM Platform connection explanation

docs.maekon.dev
  - Installation
  - Local run
  - Privacy/security
  - ONESHIM integration
  - Development/build
  - Release integrity
```

Initially, the public repo README and GitHub Releases are the priority. The landing/docs deploy unit should be decided later, after the parent work stabilizes.

## Public Contribution Flow

If the public repo is an export target, external PR handling must be explicit.

1. Receive suggestions through public repo issues/PRs.
2. A maintainer applies the intended change to the parent SSOT.
3. Tests and review run in parent.
4. Regenerate the public export.
5. Push the exported result to the public repo and reference the original public issue/PR.

This flow can look slower to public contributors. The public README/CONTRIBUTING should explain that "the public repo is a curated export, and accepted changes are applied through the internal source tree before export."

## Release Trust Baseline

Public releases should let users answer these questions:

- Where do I download the binary?
- Which source version does it correspond to?
- What are the signing and notarization states?
- Where can I verify checksums?
- Where do I report vulnerabilities?
- How does the auto-update server relate to GitHub Releases?

Minimum baseline:

- Provide platform artifacts on GitHub Releases.
- Provide checksums.
- Maintain signed/notarized/stapled macOS releases.
- List `security@maekon.dev` in `SECURITY.md`.
- List `support@maekon.dev` in support docs.
- Repeat the Maekon Client / ONESHIM Platform relationship in release notes.

## `oneshim-client` Archive Boundary

`pseudotop/oneshim-client` should not keep receiving broad product work after
the Maekon public repo and parent SSOT path are ready. Treat it as a transition
repository that needs a clean archival boundary, not as a second long-lived
public channel.

The final update window for `oneshim-client` should include only:

- the latest already-merged client state that still needs to be preserved;
- public export gate and launch-playbook fixes needed to safely bootstrap
  `pseudotop/maekon-client`;
- documentation that explains the Maekon Client / ONESHIM Platform relationship;
- release/install/security notes needed by users who still arrive through
  existing `oneshim-client` links;
- an archive/deprecation notice once the replacement public repo is ready.

Do not use `oneshim-client` for:

- new feature development after parent SSOT migration starts;
- long-lived Maekon-specific landing/docs work;
- duplicate public source maintenance once `pseudotop/maekon-client` exists;
- parent migration scripts that are only meaningful inside the private parent
  repo.

Recommended archive sequence:

1. Land the strategy/export cleanup in `oneshim-client`.
2. Bootstrap or update `pseudotop/maekon-client` from a vetted export.
3. Verify install, release, and security links in the Maekon public repo.
4. Add a final archive notice to `oneshim-client` README and repository
   description that points to `pseudotop/maekon-client`.
5. Freeze `oneshim-client` to security/redirect-only maintenance.
6. Archive the GitHub repository only after no active install/update flow still
   depends on it.

Archiving the GitHub repository too early can break user expectations around
install URLs, Releases, and issue reporting. The archive action should be the
last step, not the first signal.

## Pre-integration Review Gate

Do not promote `client-rust` into the parent-internal SSOT until these checks pass:

1. Parent work is stable and the latest `origin/main` structure has been reviewed again.
2. Existing parent `client-rust` submodule workflow docs have been identified.
3. `clients/maekon-client` does not unnecessarily disturb parent server/backoffice/docs structure.
4. Export tooling can accept a parent source path.
5. Public export gates block parent-only path leakage.
6. The Maekon/ONESHIM relationship copy can be applied to README, install docs, login/OAuth, and update docs.
7. Public contribution handling copy is ready for the public repo.

## Follow-up Work

1. Re-scan current submodule usage after the parent repo stabilizes.
2. Write a separate migration plan for moving to `clients/maekon-client`.
3. Decide whether to evolve `scripts/export-public-repo.sh` into a parent-aware export tool or replace it with `tools/public-export/maekon-client`.
4. Prepare public repo templates, `SECURITY.md`, `CONTRIBUTING.md`, and release notes skeleton for `pseudotop/maekon-client`.
5. Decide whether Maekon landing/docs live inside the public repo or in separate repo/deploy units.
6. Prepare the final `oneshim-client` archive notice after the Maekon public repo is verified.

## Current Conclusion

The current recommendation is:

```text
clients/maekon-client              # parent-internal SSOT
tools/public-export/maekon-client  # public scope and export operations
.public-worktrees/maekon-client    # gitignored public checkout
```

This structure supports Maekon Client as a trust-focused public client without immediately forcing the whole parent repo into a complete monorepo IA reset.
