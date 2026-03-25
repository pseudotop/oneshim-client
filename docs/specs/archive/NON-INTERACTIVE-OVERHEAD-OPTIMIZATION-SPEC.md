# Non-Interactive Agent Overhead Optimization — Technical Specification

> **Version**: 1.1 (verified 2026-03-25)
> **Scope**: lefthook pre-commit hook optimization for subagent environments

---

## 1. Problem

`cargo-clippy` pre-commit hook runs ~90 seconds on every commit, including
subagent commits where clippy was already verified in the parent session.

---

## 2. Actionable Fix: lefthook Environment Detection

### Current State

`lefthook.yml` lines 99-102:
```yaml
cargo-clippy:
  glob: "*.rs"
  run: cargo clippy --workspace --quiet -- -D warnings
```

No environment-based skip condition exists.

### Fix

Add `ONESHIM_AGENT` environment variable check to skip clippy in subagent mode:

```yaml
cargo-clippy:
  glob: "*.rs"
  run: |
    if [ "$ONESHIM_AGENT" = "subagent" ]; then
      echo "clippy: skipped (subagent mode)"
      exit 0
    fi
    cargo clippy --workspace --quiet -- -D warnings
```

Subagents set `ONESHIM_AGENT=subagent` before committing. The parent session
runs clippy in CI or manually before merge.

### Files

| File | Change |
|------|--------|
| `lefthook.yml` | Add `ONESHIM_AGENT` check to `cargo-clippy` command |

### Verification

1. Without env var: `cargo clippy` still runs normally
2. With `ONESHIM_AGENT=subagent git commit`: clippy skipped, commit proceeds

---

## 3. Deferred Items (Claude Code Feature Dependent)

These require Claude Code/Superpowers features that don't exist yet:

| Item | Blocker |
|------|---------|
| `.claude/profiles/` system | No `--profile` flag or `CLAUDE_PROFILE` env var in Claude Code |
| MCP conditional loading | MCP config managed externally by Claude Code |
| Skills selective loading | Skills managed by Superpowers plugin |
| Deferred tools minimization | Controlled by Claude Code runtime |

These items should be revisited when Claude Code adds profile support.
