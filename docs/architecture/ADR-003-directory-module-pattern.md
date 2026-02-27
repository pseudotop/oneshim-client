[English](./ADR-003-directory-module-pattern.md) | [한국어](./ADR-003-directory-module-pattern.ko.md)

# ADR-003: Directory Module Pattern for Large Source Files

**Status**: Accepted
**Date**: 2026-02-27
**Scope**: All crates in the workspace

---

## Context

Several source files in the workspace exceeded 500 lines, making them difficult to navigate, review, and maintain. The server-side codebase had already adopted a similar pattern (server ADR-013: Domain Service Folder Pattern) for Python modules exceeding 500 lines with positive results across 5 domains.

Files identified at the time of this decision:

| File | Lines | Crate |
|------|-------|-------|
| `handlers/automation.rs` | 1,558 | oneshim-web |
| `controller.rs` | 1,465 | oneshim-automation |
| `updater.rs` | 1,418 | oneshim-app |
| `config.rs` | 1,382 | oneshim-core |
| `app.rs` | 1,227 | oneshim-ui |
| `scheduler.rs` | 1,067 | oneshim-app |
| `focus_analyzer.rs` | 859 | oneshim-app |
| `policy.rs` | 815 | oneshim-automation |
| `gui_interaction.rs` | 750 | oneshim-automation |

`main.rs` (726 lines) was excluded — it is a binary entry point with sequential DI wiring where splitting would scatter the composition logic with no clear responsibility boundary.

---

## Decision

### 1. Convert large files to directory modules

When a Rust source file exceeds **500 lines**, convert it from a single file (`foo.rs`) to a directory module (`foo/mod.rs` + focused sub-files).

### 2. Preserve external API via `pub use` re-exports

`mod.rs` must re-export all public symbols so that **every existing import path continues to compile without changes**. No downstream consumer should need modification after a split.

```rust
// foo/mod.rs
mod helpers;
mod types;

pub use helpers::*;
pub use types::*;
```

### 3. Use `pub(super)` for internal items

Items shared across sub-files within the directory but not intended for external use must use `pub(super)` visibility.

```rust
// foo/helpers.rs
pub(super) fn require_config_manager(state: &AppState) -> Result<&ConfigManager, ApiError> {
    // ...
}
```

### 4. Keep tests in `mod.rs`

All `#[cfg(test)] mod tests` blocks remain in `mod.rs`. Tests naturally exercise the module's public interface and serve as documentation of expected behavior at the module boundary.

### 5. Split by responsibility, not by size

Sub-files are organized by functional responsibility, not arbitrary line counts:

- **types/models**: Data structures, enums, DTOs
- **helpers**: Private utility functions
- **feature groups**: Logically cohesive handler/method groups (e.g., `scene.rs`, `execution.rs`, `intent.rs`, `preset.rs`)

### 6. Threshold and exclusions

- **Threshold**: 500 lines (soft guideline, not a hard rule)
- **Excluded**: `main.rs` and similar binary entry points where sequential composition logic is the primary concern
- **Not retroactive**: Files under 500 lines should not be split preemptively

---

## Applied Splits

| Original File | Target Structure | Crate |
|---------------|-----------------|-------|
| `gui_interaction.rs` | `gui_interaction/{mod, types, crypto, helpers, service}.rs` | oneshim-automation |
| `policy.rs` | `policy/{mod, models, token}.rs` | oneshim-automation |
| `controller.rs` | `controller/{mod, types, intent, preset}.rs` | oneshim-automation |
| `focus_analyzer.rs` | `focus_analyzer/{mod, models, suggestions}.rs` | oneshim-app |
| `scheduler.rs` | `scheduler/{mod, config, loops}.rs` | oneshim-app |
| `updater.rs` | `updater/{mod, github, install, state}.rs` | oneshim-app |
| `config.rs` | `config/{mod, enums, sections}.rs` | oneshim-core |
| `handlers/automation.rs` | `handlers/automation/{mod, helpers, scene, execution}.rs` | oneshim-web |
| `app.rs` | `app/{mod, message, update, view}.rs` | oneshim-ui |

---

## Consequences

### Positive

- Each sub-file is under 300 lines, improving navigation and code review
- `cargo test/clippy/fmt` continue to pass without any logic changes
- External API paths are fully preserved — zero downstream breakage
- Consistent with the server-side ADR-013 folder pattern, reducing cognitive overhead across the monorepo

### Tradeoffs

- Minor increase in file count (9 files become ~35 files)
- Developers must understand `pub(super)` and re-export patterns
- `mod.rs` files carry re-export boilerplate

### Risks

- `pub use *` re-exports may unintentionally expose items added later. Mitigated by code review and `pub(super)` discipline on internal items.

---

## Related Docs

- Server ADR-013: `server/docs/architecture/ADR-013-domain-service-folder-pattern.md`
- `docs/architecture/ADR-001-rust-client-architecture-patterns.md`
- `CLAUDE.md` — Crate Summary section documents each directory module structure
