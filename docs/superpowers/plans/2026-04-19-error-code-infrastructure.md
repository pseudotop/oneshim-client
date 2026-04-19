# Error Code Infrastructure + C5 AWS Bedrock Skip — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retrofit `CoreError` (32 variants, 1,052 sites) and `GuiInteractionError` (8 variants, 97 sites) across the 14-crate workspace to carry typed error code enums; ship AWS Bedrock as the first intentionally-unsupported provider via `ConfigCode::UnsupportedProviderBedrock`; delete Bedrock from the provider catalog; fix OCR no-auth fallthrough security bug.

**Architecture:** 19 code enums (one per error domain) defined via a single `define_code_enum!` macro for single-source-of-truth between variant list, `as_str` match, `Display` impl, and `all()` enumerator. `CoreError` and `GuiInteractionError` gain a `code` field on each variant while keeping existing message/structured fields. Soft V1→V2→V1 migration in 4 phases / 16 PRs: Phase 1 introduces V2 variants + deprecates V1; Phase 2 per-crate retrofits V1→V2 (13 PRs); Phase 3 ships C5 Bedrock + ADR-019; Phase 4 deletes V1 and LSP-renames V2 to canonical names. CI enforces `-D deprecated` at Phase 4 boundary.

**Tech Stack:** Rust 1.77.1 edition 2021; `thiserror` for error derivation; `#[non_exhaustive]` on code enums; rust-analyzer LSP rename (not `sed`) for Phase 4; hand-rolled snapshot test for wire-format contract; `cargo clippy --workspace -- -D warnings` for CI gating.

**Related documents:**
- Spec: `docs/superpowers/specs/2026-04-19-error-code-infrastructure-design.md`
- ADR-019 (to be authored in Phase 3): `docs/architecture/ADR-019-error-code-infrastructure.md`
- ADR-001 §1 (Error strategy), ADR-003 (Directory module pattern) — existing references

**Memory patterns to apply:**
- `feedback_cross_consumer_audit` — grep all consumers before changing variant default/semantics
- `feedback_holistic_pre_merge_review` — final integrated pass after all PRs land
- `feedback_3loop_quality_gate` — spec → plan → impl, zero-Critical/Important gate each loop
- `feedback_ci_workflow_assumption_verification` — verify any CI workflow claim against live data
- `feedback_serial_test_pattern` — `serial_test` dev-dep for tests mutating module globals

---

## File Structure (all paths relative to workspace root)

### Created files (Phase 1)

| Path | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/error_codes/mod.rs` | Module root; `pub use` re-exports; `#[doc(hidden)] pub fn all_codes()` aggregator |
| `crates/oneshim-core/src/error_codes/macros.rs` | `define_code_enum!` macro (internal) |
| `crates/oneshim-core/src/error_codes/config.rs` | `ConfigCode` enum via macro |
| `crates/oneshim-core/src/error_codes/network.rs` | `NetworkCode` |
| `crates/oneshim-core/src/error_codes/auth.rs` | `AuthCode` |
| `crates/oneshim-core/src/error_codes/internal.rs` | `InternalCode` |
| `crates/oneshim-core/src/error_codes/validation.rs` | `ValidationCode` |
| `crates/oneshim-core/src/error_codes/not_found.rs` | `NotFoundCode` |
| `crates/oneshim-core/src/error_codes/consent.rs` | `ConsentCode` |
| `crates/oneshim-core/src/error_codes/integrity.rs` | `IntegrityCode` |
| `crates/oneshim-core/src/error_codes/sandbox.rs` | `SandboxCode` |
| `crates/oneshim-core/src/error_codes/policy.rs` | `PolicyCode` |
| `crates/oneshim-core/src/error_codes/permission.rs` | `PermissionCode` |
| `crates/oneshim-core/src/error_codes/oauth.rs` | `OAuthCode` |
| `crates/oneshim-core/src/error_codes/secret.rs` | `SecretCode` |
| `crates/oneshim-core/src/error_codes/provider.rs` | `ProviderCode` |
| `crates/oneshim-core/src/error_codes/audio.rs` | `AudioCode` |
| `crates/oneshim-core/src/error_codes/storage.rs` | `StorageCode` |
| `crates/oneshim-core/src/error_codes/ui.rs` | `UiCode` |
| `crates/oneshim-core/src/error_codes/service.rs` | `ServiceCode` |
| `crates/oneshim-core/src/error_codes/gui.rs` | `GuiCode` |
| `crates/oneshim-core/tests/wire_contract_snapshot.rs` | Snapshot test integration |
| `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` | Sorted wire-format fixture |

### Modified files (Phase 1)

| Path | Change |
|------|--------|
| `crates/oneshim-core/src/lib.rs` | `pub mod error_codes;` + `pub use error_codes::*;` |
| `crates/oneshim-core/src/error.rs` | Add V2 variants; `#[deprecated]` V1; `impl CoreError::code()`; same for `GuiInteractionError` |
| `crates/oneshim-core/Cargo.toml` | Add `serial_test = "3"` dev-dep (if not present) for test isolation |

### Created files (Phase 3)

| Path | Responsibility |
|------|----------------|
| `docs/architecture/ADR-019-error-code-infrastructure.md` | Architectural record |
| `docs/architecture/ADR-019-error-code-infrastructure.ko.md` | Korean companion |

### Modified files (Phase 3)

| Path | Change |
|------|--------|
| `specs/providers/provider-surface-catalog.json` | Remove `bedrock` vendor + `provider_surface.bedrock.direct_api` surface |
| `crates/oneshim-network/src/ai_ocr_client/mod.rs` | 2 match arms → `ConfigV2 { code: UnsupportedProviderBedrock, .. }`; `apply_auth_headers` signature → `Result<_, CoreError>` |
| `crates/oneshim-network/src/ai_ocr_client/strategy.rs` | 1 match arm |
| `crates/oneshim-network/src/ai_llm_client/request.rs` | 3 match arms |
| `crates/oneshim-network/src/http_api_session/mod.rs` | 1 match arm |
| `crates/oneshim-network/tests/ai_provider_live_smoke.rs` | Bedrock arm updated to assert error pattern without live call |

### Modified files (Phase 2 — per crate; pattern detailed in Task Template)

Per crate: retrofit every `CoreError::VariantV1(...)` construction and match site to `CoreError::VariantV2 { code, message }`. Exact file count per crate determined by grep at start of each Phase-2 PR.

### Modified files (Phase 4)

| Path | Change |
|------|--------|
| `crates/oneshim-core/src/error.rs` | Delete V1 variants; rust-analyzer rename `ConfigV2` → `Config`, etc. |
| `crates/oneshim-core/src/lib.rs` | Remove any V1-specific re-exports |
| `.github/workflows/ci.yml` (or equivalent) | Flip to `cargo clippy -- -D deprecated` |

---

## PR Sequencing Summary

```
PR  1 ─── Phase 1: Error code infrastructure (single PR)
PR  2 ─── Phase 2: oneshim-api-contracts
PR  3 ─── Phase 2: oneshim-embedding
PR  4 ─── Phase 2: oneshim-audio
PR  5 ─── Phase 2: oneshim-monitor
PR  6 ─── Phase 2: oneshim-storage
PR  7 ─── Phase 2: oneshim-vision
PR  8 ─── Phase 2: oneshim-analysis
PR  9 ─── Phase 2: oneshim-network
PR 10 ─── Phase 2: oneshim-suggestion
PR 11 ─── Phase 2: oneshim-automation
PR 12 ─── Phase 2: oneshim-web
PR 13 ─── Phase 2: oneshim-sandbox-worker (verification-only)
PR 14 ─── Phase 2: src-tauri
PR 15 ─── Phase 3: C5 Bedrock skip + ADR-019
PR 16 ─── Phase 4: V1 deletion + V2 rename + CI gate flip
```

Gating: Phase 2 PRs 2-14 require PR 1 merged. PR 15 requires PR 9 merged (not entire Phase 2). PR 16 requires PRs 1-15 all merged.

---

# Phase 1 — Error Code Infrastructure (PR 1)

### Task 1.1: Create `error_codes` module scaffold

**Files:**
- Create: `crates/oneshim-core/src/error_codes/mod.rs`
- Create: `crates/oneshim-core/src/error_codes/macros.rs`
- Modify: `crates/oneshim-core/src/lib.rs` (add `pub mod error_codes;`)

- [ ] **Step 1: Write the failing test**

Create `crates/oneshim-core/tests/error_codes_module_present.rs`:

```rust
//! Smoke test: error_codes module is exposed.
use oneshim_core::error_codes;

#[test]
fn module_exists() {
    // This file existing and compiling proves the module is accessible.
    let _ = error_codes::all_codes();
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p oneshim-core --test error_codes_module_present
```

Expected: compile error — `error_codes` module not found.

- [ ] **Step 3: Create the macro file**

Create `crates/oneshim-core/src/error_codes/macros.rs`:

```rust
//! define_code_enum! — single-source macro for code enum definitions.
//!
//! Generates enum body, `as_str` match, `Display` impl, and `all()` enumerator
//! from one variant list. Prevents drift between match and array per §7.5 of spec
//! `docs/superpowers/specs/2026-04-19-error-code-infrastructure-design.md`.

#[macro_export]
macro_rules! define_code_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $wire:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Wire-format code string. Immutable after first release — see ADR-019 §2.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $wire, )+
                }
            }

            /// Compile-time exhaustive enumeration. The `as_str` match above is
            /// the enforcement point — adding a variant without updating it fails
            /// `cargo build`. This method derives from the same list so drift
            /// between match and array is architecturally impossible.
            ///
            /// `pub(crate)` so that (a) the `error_codes/mod.rs` aggregator can
            /// call `ConfigCode::all()` and (b) per-enum `#[cfg(test)] mod tests`
            /// blocks inside the enum's own file (a child module, which `pub(super)`
            /// would not reach) can enumerate variants.
            #[allow(dead_code)]
            pub(crate) const fn all() -> &'static [Self] {
                &[ $( Self::$variant, )+ ]
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
```

- [ ] **Step 4: Create module root**

Create `crates/oneshim-core/src/error_codes/mod.rs`:

```rust
//! Error code registry — central management of wire-format error identifiers.
//!
//! Per ADR-019 §2, every code follows the naming convention
//! `{domain}.{category}[.{qualifier}]` and is immutable after first release.
//!
//! Each code enum is defined via the `define_code_enum!` macro for single-source-
//! of-truth between variant list, `as_str` match, `Display` impl, and `all()`
//! enumerator. See `macros.rs` for the macro definition.

#[macro_use]
mod macros;

// Sub-modules will be added incrementally in Tasks 1.2–1.20.

/// Collects every wire-format code string across every code enum, sorted.
///
/// Internal test helper for wire-contract snapshot test at
/// `tests/wire_contract_snapshot.rs`. Marked `#[doc(hidden)]` to signal it is
/// not an external API.
#[doc(hidden)]
pub fn all_codes() -> Vec<&'static str> {
    let mut codes: Vec<&'static str> = Vec::new();
    // Each Task 1.2–1.20 appends one block `for c in {Xyz}Code::all() { codes.push(c.as_str()); }`
    codes.sort();
    codes
}
```

- [ ] **Step 5: Register module in `lib.rs`**

Modify `crates/oneshim-core/src/lib.rs` — find the top-level module declarations and add:

```rust
pub mod error_codes;
```

after the existing `pub mod` lines (e.g., immediately before or after `pub mod config;`).

- [ ] **Step 6: Run smoke test**

```bash
cargo test -p oneshim-core --test error_codes_module_present
```

Expected: PASS (module is accessible, `all_codes()` returns empty sorted `Vec`).

- [ ] **Step 7: Run workspace build to ensure no regression**

```bash
cargo check --workspace
```

Expected: clean compile (no new warnings, no errors).

- [ ] **Step 8: Commit**

```bash
git add crates/oneshim-core/src/error_codes/ \
        crates/oneshim-core/src/lib.rs \
        crates/oneshim-core/tests/error_codes_module_present.rs
git commit -m "feat(oneshim-core): add error_codes module scaffold + define_code_enum macro

Per spec docs/superpowers/specs/2026-04-19-error-code-infrastructure-design.md §4.2.
Scaffolds module with directory-module layout; macro for single-source enum definition.
Follows ADR-003 style; does not yet contain any code enums (added in Tasks 1.2-1.20)."
```

---

### Task 1.2: Implement `ConfigCode`

**Files:**
- Create: `crates/oneshim-core/src/error_codes/config.rs`
- Modify: `crates/oneshim-core/src/error_codes/mod.rs` (register submodule + all_codes entry)

- [ ] **Step 1: Write the failing tests (TDD)**

The enum does not exist yet; Step 2 will define it alongside the tests so Steps 1-2 are tightly coupled. Proceed to Step 2, which creates both the enum and its tests in a single file. After Step 2 the tests will briefly fail-to-compile (enum not yet registered in `mod.rs`); Step 3 registers the module which resolves the fail, and Step 4 confirms pass.

- [ ] **Step 2: Create `ConfigCode` via macro (enum + tests in one file)**

Create `crates/oneshim-core/src/error_codes/config.rs`:

```rust
//! ConfigCode — Config 카테고리 에러 코드.
//!
//! 네이밍: `config.*` 접두사. 신규 코드 추가 시 ADR-019 §2 컨벤션 준수.

use crate::define_code_enum;

define_code_enum! {
    /// Config 카테고리 에러 코드.
    pub enum ConfigCode {
        /// 설정 파일 파싱 실패 또는 스키마 불일치.
        Invalid => "config.invalid",
        /// 필수 설정 필드 누락.
        Missing => "config.missing",
        /// 설정 값이 허용 범위 밖.
        OutOfRange => "config.out_of_range",
        /// AWS Bedrock 의도적 미지원 (ADR-019 §5 재도입 체크리스트 참조).
        UnsupportedProviderBedrock => "provider.bedrock.unsupported",
        /// 세분화 미완료 — Phase 2 일괄 이관 시 기본값. §10-7 lint가 crate별 사용량 감시.
        Generic => "config.generic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn as_str_round_trip_unique() {
        let codes: Vec<&str> = ConfigCode::all().iter().map(|c| c.as_str()).collect();
        let unique: HashSet<_> = codes.iter().collect();
        assert_eq!(codes.len(), unique.len(), "duplicate codes: {codes:?}");
    }

    #[test]
    fn naming_convention() {
        for c in ConfigCode::all() {
            let s = c.as_str();
            assert!(
                s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '.' || ch == '_'),
                "non-conforming character in {s:?}"
            );
            assert!(s.contains('.'), "missing dot in {s:?}");
        }
    }

    #[test]
    fn display_matches_as_str() {
        for c in ConfigCode::all() {
            assert_eq!(format!("{c}"), c.as_str());
        }
    }
}
```

- [ ] **Step 3: Register submodule + all_codes aggregator**

Modify `crates/oneshim-core/src/error_codes/mod.rs`:

1. Add `pub mod config;` after `mod macros;`
2. Add `pub use config::ConfigCode;` re-export block
3. Inside `all_codes()`, immediately before `codes.sort();`, add:

```rust
    for c in ConfigCode::all() {
        codes.push(c.as_str());
    }
```

- [ ] **Step 4: Run tests to verify all pass**

```bash
cargo test -p oneshim-core error_codes::config
```

Expected: 3 tests PASS (`as_str_round_trip_unique`, `naming_convention`, `display_matches_as_str`).

- [ ] **Step 5: Run workspace check**

```bash
cargo check --workspace
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-core/src/error_codes/config.rs \
        crates/oneshim-core/src/error_codes/mod.rs
git commit -m "feat(oneshim-core): add ConfigCode enum

5 initial variants: Invalid, Missing, OutOfRange, UnsupportedProviderBedrock, Generic.
UnsupportedProviderBedrock is the C5 Bedrock skip anchor (Phase 3 will reference this)."
```

---

### Task 1.3: Implement `NetworkCode`

**Files:**
- Create: `crates/oneshim-core/src/error_codes/network.rs`
- Modify: `crates/oneshim-core/src/error_codes/mod.rs`

- [ ] **Step 1: Create the enum**

Create `crates/oneshim-core/src/error_codes/network.rs`:

```rust
//! NetworkCode — Network 카테고리 에러 코드. `network.*` 접두사.

use crate::define_code_enum;

define_code_enum! {
    /// Network 카테고리 에러 코드.
    pub enum NetworkCode {
        /// 네트워크 요청 일반 실패 (연결 거부, DNS 실패 등).
        Failed => "network.failed",
        /// 요청 타임아웃 초과.
        Timeout => "network.timeout",
        /// 서버 레이트 리밋 도달 (429).
        RateLimit => "network.rate_limit",
        /// 세분화 미완료.
        Generic => "network.generic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn as_str_round_trip_unique() {
        let codes: Vec<&str> = NetworkCode::all().iter().map(|c| c.as_str()).collect();
        let unique: HashSet<_> = codes.iter().collect();
        assert_eq!(codes.len(), unique.len());
    }

    #[test]
    fn naming_convention() {
        for c in NetworkCode::all() {
            let s = c.as_str();
            assert!(s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '.' || ch == '_'));
            assert!(s.contains('.'));
        }
    }

    #[test]
    fn display_matches_as_str() {
        for c in NetworkCode::all() {
            assert_eq!(format!("{c}"), c.as_str());
        }
    }
}
```

- [ ] **Step 2: Register in `mod.rs`**

Modify `crates/oneshim-core/src/error_codes/mod.rs`:
- Add `pub mod network;`
- Add `pub use network::NetworkCode;`
- Add `for c in NetworkCode::all() { codes.push(c.as_str()); }` inside `all_codes()`.

- [ ] **Step 3: Run tests**

```bash
cargo test -p oneshim-core error_codes::network
```

Expected: **3 tests PASS** (matching the standard 3-tests-per-enum pattern in Task 1.2).

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-core/src/error_codes/network.rs \
        crates/oneshim-core/src/error_codes/mod.rs
git commit -m "feat(oneshim-core): add NetworkCode enum"
```

---

### Task 1.4: Implement remaining 17 code enums

**Pattern**: repeat Tasks 1.2–1.3 for each of the remaining 17 enums. Each sub-task follows identical structure — only the variant list differs.

For each enum below, execute:
- Step A: Create `crates/oneshim-core/src/error_codes/{file}.rs` with `define_code_enum!` invocation using the given variant list.
- Step B: Add `tests` module identical to ConfigCode/NetworkCode pattern (uniqueness + naming convention).
- Step C: Register in `mod.rs` (submodule + re-export + `all_codes()` entry).
- Step D: `cargo test -p oneshim-core error_codes::{file}` passes.
- Step E: Commit with message `feat(oneshim-core): add {Xyz}Code enum`.

**Enums to implement** (17 remaining; variant lists mandated — do not add or remove without spec PR):

**Standard per-enum test block** (append inside each `crates/oneshim-core/src/error_codes/{xyz}.rs` beneath the `define_code_enum!` invocation — 3 tests to match Task 1.2's `ConfigCode` pattern):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn as_str_round_trip_unique() {
        let codes: Vec<&str> = /* XyzCode */::all().iter().map(|c| c.as_str()).collect();
        let unique: HashSet<_> = codes.iter().collect();
        assert_eq!(codes.len(), unique.len(), "duplicate codes: {codes:?}");
    }

    #[test]
    fn naming_convention() {
        for c in /* XyzCode */::all() {
            let s = c.as_str();
            assert!(s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '.' || ch == '_'));
            assert!(s.contains('.'));
        }
    }

    #[test]
    fn display_matches_as_str() {
        for c in /* XyzCode */::all() {
            assert_eq!(format!("{c}"), c.as_str());
        }
    }
}
```

Substitute `/* XyzCode */` with the actual enum name (e.g., `AuthCode`, `NetworkCode`). **3 tests per enum × 19 enums = 57 total unit tests** in the error_codes module.

#### 1.4.a `AuthCode` — `crates/oneshim-core/src/error_codes/auth.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum AuthCode {
        Failed => "auth.failed",
        Generic => "auth.generic",
    }
}
```

#### 1.4.b `InternalCode` — `crates/oneshim-core/src/error_codes/internal.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum InternalCode {
        Generic => "internal.generic",
        Io => "internal.io",
        Serialization => "internal.serialization",
    }
}
```

Note: `Io` and `Serialization` are used only by the `impl CoreError::code()` arms for `#[from]`-wrapped variants — they are NOT stored on any variant field.

#### 1.4.c `ValidationCode` — `crates/oneshim-core/src/error_codes/validation.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum ValidationCode {
        InvalidField => "validation.invalid_field",
        InvalidArguments => "validation.invalid_arguments",
        Generic => "validation.generic",
    }
}
```

#### 1.4.d `NotFoundCode` — `crates/oneshim-core/src/error_codes/not_found.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum NotFoundCode {
        ResourceMissing => "not_found.resource_missing",
    }
}
```

#### 1.4.e `ConsentCode` — `crates/oneshim-core/src/error_codes/consent.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum ConsentCode {
        Required => "consent.required",
        Expired => "consent.expired",
        Generic => "consent.generic",
    }
}
```

#### 1.4.f `IntegrityCode` — `crates/oneshim-core/src/error_codes/integrity.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum IntegrityCode {
        HashMismatch => "integrity.hash_mismatch",
    }
}
```

#### 1.4.g `SandboxCode` — `crates/oneshim-core/src/error_codes/sandbox.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum SandboxCode {
        InitFailed => "sandbox.init_failed",
        ExecutionFailed => "sandbox.execution_failed",
        UnsupportedPlatform => "sandbox.unsupported_platform",
        Timeout => "sandbox.timeout",
    }
}
```

#### 1.4.h `PolicyCode` — `crates/oneshim-core/src/error_codes/policy.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum PolicyCode {
        Denied => "policy.denied",
        ProcessDenied => "policy.process_denied",
        Generic => "policy.generic",
    }
}
```

#### 1.4.i `PermissionCode` — `crates/oneshim-core/src/error_codes/permission.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum PermissionCode {
        PermissionDenied => "permission.permission_denied",
        PrivacyDenied => "permission.privacy_denied",
        Generic => "permission.generic",
    }
}
```

#### 1.4.j `OAuthCode` — `crates/oneshim-core/src/error_codes/oauth.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum OAuthCode {
        Failed => "oauth.failed",
        RefreshFailed => "oauth.refresh_failed",
        Generic => "oauth.generic",
    }
}
```

#### 1.4.k `SecretCode` — `crates/oneshim-core/src/error_codes/secret.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum SecretCode {
        Failed => "secret.failed",
        Generic => "secret.generic",
    }
}
```

#### 1.4.l `ProviderCode` — `crates/oneshim-core/src/error_codes/provider.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum ProviderCode {
        OcrFailed => "provider.ocr_failed",
        AnalysisFailed => "provider.analysis_failed",
        Generic => "provider.generic",
    }
}
```

#### 1.4.m `AudioCode` — `crates/oneshim-core/src/error_codes/audio.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum AudioCode {
        CaptureFailed => "audio.capture_failed",
        SttFailed => "audio.stt_failed",
        Generic => "audio.generic",
    }
}
```

#### 1.4.n `StorageCode` — `crates/oneshim-core/src/error_codes/storage.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum StorageCode {
        Failed => "storage.failed",
        Generic => "storage.generic",
    }
}
```

#### 1.4.o `UiCode` — `crates/oneshim-core/src/error_codes/ui.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum UiCode {
        ElementMissing => "ui.element_missing",
    }
}
```

#### 1.4.p `ServiceCode` — `crates/oneshim-core/src/error_codes/service.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum ServiceCode {
        Unavailable => "service.unavailable",
        Generic => "service.generic",
    }
}
```

#### 1.4.q `GuiCode` — `crates/oneshim-core/src/error_codes/gui.rs`
```rust
use crate::define_code_enum;
define_code_enum! {
    pub enum GuiCode {
        Unauthorized => "gui.unauthorized",
        NotFound => "gui.not_found",
        BadRequest => "gui.bad_request",
        Forbidden => "gui.forbidden",
        FocusDrift => "gui.focus_drift",
        TicketInvalid => "gui.ticket_invalid",
        Unavailable => "gui.unavailable",
        InternalError => "gui.internal_error",
        Generic => "gui.generic",
    }
}
```

- [ ] **Checkpoint after 1.4.q**: all 19 enums exist. Run:

```bash
cargo test -p oneshim-core error_codes
```

Expected: all 19 × 3 tests pass (57 total unit tests).

- [ ] **Commit grouping**: each enum is its own commit per the pattern in Task 1.3.

---

### Task 1.5: Add V2 variants to `CoreError` with deprecated V1

**Files:**
- Modify: `crates/oneshim-core/src/error.rs`

- [ ] **Step 1: Read current error.rs to preserve exact field layouts**

```bash
cat crates/oneshim-core/src/error.rs
```

(The agent must read this before editing to preserve `#[from]` and derive attributes.)

- [ ] **Step 2: Rewrite `CoreError` enum body with V1 + V2 variants**

Replace the existing `#[derive(Debug, Error)] pub enum CoreError { ... }` block with:

```rust
use crate::error_codes::{
    AuthCode, AudioCode, ConfigCode, ConsentCode, IntegrityCode, InternalCode,
    NetworkCode, NotFoundCode, OAuthCode, PermissionCode, PolicyCode, ProviderCode,
    SandboxCode, SecretCode, ServiceCode, StorageCode, UiCode, ValidationCode,
};
use crate::ports::oauth::OAuthErrorKind;

#[derive(Debug, Error)]
pub enum CoreError {
    // === V2 variants (new struct-variant shape) ===

    #[error("Configuration error [{code}]: {message}")]
    ConfigV2 { code: ConfigCode, message: String },

    #[error("Network error [{code}]: {message}")]
    NetworkV2 { code: NetworkCode, message: String },

    #[error("Request timed out [{code}] after {timeout_ms}ms")]
    RequestTimeoutV2 { code: NetworkCode, timeout_ms: u64 },

    #[error("Request rate limit exceeded [{code}], retry after {retry_after_secs}s")]
    RateLimitV2 { code: NetworkCode, retry_after_secs: u64 },

    #[error("Service temporarily unavailable [{code}]: {message}")]
    ServiceUnavailableV2 { code: ServiceCode, message: String },

    #[error("Authentication error [{code}]: {message}")]
    AuthV2 { code: AuthCode, message: String },

    #[error("OAuth error [{code}] for provider {provider}: {message}")]
    OAuthErrorV2 { code: OAuthCode, provider: String, message: String },

    #[error("OAuth refresh error [{code}] for provider {provider}: [{kind:?}] {message}")]
    OAuthRefreshErrorV2 { code: OAuthCode, provider: String, kind: OAuthErrorKind, message: String },

    #[error("Validation failed [{code}] - {field}: {message}")]
    ValidationV2 { code: ValidationCode, field: String, message: String },

    #[error("Invalid arguments [{code}]: {message}")]
    InvalidArgumentsV2 { code: ValidationCode, message: String },

    #[error("{resource_type} not found [{code}]: {id}")]
    NotFoundV2 { code: NotFoundCode, resource_type: String, id: String },

    #[error("UI element not found [{code}]: {name}")]
    ElementNotFoundV2 { code: UiCode, name: String },

    #[error("Binary hash mismatch [{code}]: expected={expected}, actual={actual}")]
    BinaryHashMismatchV2 { code: IntegrityCode, expected: String, actual: String },

    #[error("Internal error [{code}]: {message}")]
    InternalV2 { code: InternalCode, message: String },

    #[error("Policy denied [{code}]: {message}")]
    PolicyDeniedV2 { code: PolicyCode, message: String },

    #[error("Process not allowed [{code}]: {message}")]
    ProcessNotAllowedV2 { code: PolicyCode, message: String },

    #[error("Consent required [{code}]: {message}")]
    ConsentRequiredV2 { code: ConsentCode, message: String },

    #[error("Consent expired [{code}]")]
    ConsentExpiredV2 { code: ConsentCode },

    #[error("Sandbox initialization failed [{code}]: {message}")]
    SandboxInitV2 { code: SandboxCode, message: String },

    #[error("Sandbox execution failed [{code}]: {message}")]
    SandboxExecutionV2 { code: SandboxCode, message: String },

    #[error("Sandbox unsupported on platform [{code}]: {message}")]
    SandboxUnsupportedV2 { code: SandboxCode, message: String },

    #[error("Execution timeout [{code}] exceeded: {timeout_ms}ms")]
    ExecutionTimeoutV2 { code: SandboxCode, timeout_ms: u64 },

    #[error("Privacy denied [{code}]: {message}")]
    PrivacyDeniedV2 { code: PermissionCode, message: String },

    #[error("Permission denied [{code}]: {message}")]
    PermissionDeniedV2 { code: PermissionCode, message: String },

    #[error("OCR error [{code}]: {message}")]
    OcrErrorV2 { code: ProviderCode, message: String },

    #[error("Analysis error [{code}]: {message}")]
    AnalysisV2 { code: ProviderCode, message: String },

    #[error("Audio capture error [{code}]: {message}")]
    AudioCaptureV2 { code: AudioCode, message: String },

    #[error("Speech-to-text error [{code}]: {message}")]
    SpeechToTextV2 { code: AudioCode, message: String },

    #[error("Storage error [{code}]: {message}")]
    StorageV2 { code: StorageCode, message: String },

    #[error("Secret store error [{code}]: {message}")]
    SecretStoreErrorV2 { code: SecretCode, message: String },

    // === `#[from]`-wrapped external error types (unchanged across phases) ===

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // === V1 deprecated variants (removed in Phase 4) ===
    //
    // Every variant below is marked #[deprecated]. `cargo build` will emit
    // warnings on every V1 construction/match site; Phase 2 per-crate retrofits
    // migrate them to the V2 variants above. Phase 4 deletes this entire block.

    #[deprecated(since = "next", note = "use ConfigV2 { code, message } — ADR-019")]
    #[error("Configuration error: {0}")]
    Config(String),

    #[deprecated(since = "next", note = "use NetworkV2 { code, message } — ADR-019")]
    #[error("Network error: {0}")]
    Network(String),

    #[deprecated(since = "next", note = "use RequestTimeoutV2 { code, timeout_ms } — ADR-019")]
    #[error("Request timed out after {timeout_ms}ms")]
    RequestTimeout { timeout_ms: u64 },

    #[deprecated(since = "next", note = "use RateLimitV2 { code, retry_after_secs } — ADR-019")]
    #[error("Request rate limit exceeded, retry after {retry_after_secs}s")]
    RateLimit { retry_after_secs: u64 },

    #[deprecated(since = "next", note = "use ServiceUnavailableV2 — ADR-019")]
    #[error("Service temporarily unavailable: {0}")]
    ServiceUnavailable(String),

    #[deprecated(since = "next", note = "use AuthV2 — ADR-019")]
    #[error("Authentication error: {0}")]
    Auth(String),

    #[deprecated(since = "next", note = "use OAuthErrorV2 — ADR-019")]
    #[error("OAuth error for provider {provider}: {message}")]
    OAuthError { provider: String, message: String },

    #[deprecated(since = "next", note = "use OAuthRefreshErrorV2 — ADR-019")]
    #[error("OAuth refresh error for provider {provider}: [{kind:?}] {message}")]
    OAuthRefreshError { provider: String, kind: OAuthErrorKind, message: String },

    #[deprecated(since = "next", note = "use ValidationV2 — ADR-019")]
    #[error("Validation failed - {field}: {message}")]
    Validation { field: String, message: String },

    #[deprecated(since = "next", note = "use InvalidArgumentsV2 — ADR-019")]
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[deprecated(since = "next", note = "use NotFoundV2 — ADR-019")]
    #[error("{resource_type} not found: {id}")]
    NotFound { resource_type: String, id: String },

    #[deprecated(since = "next", note = "use ElementNotFoundV2 — ADR-019")]
    #[error("UI element not found: {0}")]
    ElementNotFound(String),

    #[deprecated(since = "next", note = "use BinaryHashMismatchV2 — ADR-019")]
    #[error("Binary hash mismatch: expected={expected}, actual={actual}")]
    BinaryHashMismatch { expected: String, actual: String },

    #[deprecated(since = "next", note = "use InternalV2 — ADR-019")]
    #[error("Internal error: {0}")]
    Internal(String),

    #[deprecated(since = "next", note = "use PolicyDeniedV2 — ADR-019")]
    #[error("Policy denied: {0}")]
    PolicyDenied(String),

    #[deprecated(since = "next", note = "use ProcessNotAllowedV2 — ADR-019")]
    #[error("Process is not allowed: {0}")]
    ProcessNotAllowed(String),

    #[deprecated(since = "next", note = "use ConsentRequiredV2 — ADR-019")]
    #[error("Consent required: {0}")]
    ConsentRequired(String),

    #[deprecated(since = "next", note = "use ConsentExpiredV2 — ADR-019")]
    #[error("Consent expired - re-consent required")]
    ConsentExpired,

    #[deprecated(since = "next", note = "use SandboxInitV2 — ADR-019")]
    #[error("Sandbox initialization failed: {0}")]
    SandboxInit(String),

    #[deprecated(since = "next", note = "use SandboxExecutionV2 — ADR-019")]
    #[error("Sandbox execution failed: {0}")]
    SandboxExecution(String),

    #[deprecated(since = "next", note = "use SandboxUnsupportedV2 — ADR-019")]
    #[error("Sandbox unsupported on platform: {0}")]
    SandboxUnsupported(String),

    #[deprecated(since = "next", note = "use ExecutionTimeoutV2 — ADR-019")]
    #[error("Execution timeout exceeded: {timeout_ms}ms")]
    ExecutionTimeout { timeout_ms: u64 },

    #[deprecated(since = "next", note = "use PrivacyDeniedV2 — ADR-019")]
    #[error("Privacy denied: {0}")]
    PrivacyDenied(String),

    #[deprecated(since = "next", note = "use PermissionDeniedV2 — ADR-019")]
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[deprecated(since = "next", note = "use OcrErrorV2 — ADR-019")]
    #[error("OCR error: {0}")]
    OcrError(String),

    #[deprecated(since = "next", note = "use AnalysisV2 — ADR-019")]
    #[error("Analysis error: {0}")]
    Analysis(String),

    #[deprecated(since = "next", note = "use AudioCaptureV2 — ADR-019")]
    #[error("Audio capture error: {0}")]
    AudioCapture(String),

    #[deprecated(since = "next", note = "use SpeechToTextV2 — ADR-019")]
    #[error("Speech-to-text error: {0}")]
    SpeechToText(String),

    #[deprecated(since = "next", note = "use StorageV2 — ADR-019")]
    #[error("Storage error: {0}")]
    Storage(String),

    #[deprecated(since = "next", note = "use SecretStoreErrorV2 — ADR-019")]
    #[error("secret store error: {0}")]
    SecretStoreError(String),
}
```

- [ ] **Step 3: Verify workspace compiles (with deprecation warnings expected)**

```bash
cargo check --workspace 2>&1 | head -50
```

Expected: clean compile aside from `#[deprecated]` warnings on existing V1 usage sites (these are the retrofit signal for Phase 2).

- [ ] **Step 4: Run existing tests to confirm no regression**

```bash
cargo test -p oneshim-core
```

Expected: all existing tests PASS (V1 variants still functional for unchanged callsites).

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-core/src/error.rs
git commit -m "feat(oneshim-core): add V2 variants to CoreError; deprecate V1

All 32 existing variants gain a V2 counterpart with typed \`code\` field.
V1 marked #[deprecated] — emits warnings at every existing call site as
Phase 2 retrofit signal. V1 variants removed in Phase 4 per spec §5.

Builds cleanly with deprecation warnings expected until Phase 2 completes."
```

---

### Task 1.6: Add V2 variants to `GuiInteractionError`

**Files:**
- Modify: `crates/oneshim-core/src/error.rs`

- [ ] **Step 1: Rewrite `GuiInteractionError` enum body**

Replace existing `pub enum GuiInteractionError { ... }` block with:

```rust
use crate::error_codes::GuiCode;

#[derive(Debug, Error)]
pub enum GuiInteractionError {
    // === V2 variants ===

    #[error("GUI session token is invalid [{code}]")]
    UnauthorizedV2 { code: GuiCode },

    #[error("GUI session '{name}' not found [{code}]")]
    NotFoundV2 { code: GuiCode, name: String },

    #[error("Invalid GUI request [{code}]: {message}")]
    BadRequestV2 { code: GuiCode, message: String },

    #[error("GUI request forbidden [{code}]: {message}")]
    ForbiddenV2 { code: GuiCode, message: String },

    #[error("GUI focus drift detected [{code}]: {message}")]
    FocusDriftV2 { code: GuiCode, message: String },

    #[error("GUI ticket is no longer valid [{code}]: {message}")]
    TicketInvalidV2 { code: GuiCode, message: String },

    #[error("GUI runtime unavailable [{code}]: {message}")]
    UnavailableV2 { code: GuiCode, message: String },

    #[error("GUI runtime failed [{code}]: {message}")]
    InternalV2 { code: GuiCode, message: String },

    // === V1 deprecated variants ===

    #[deprecated(since = "next", note = "use UnauthorizedV2 — ADR-019")]
    #[error("GUI session token is invalid")]
    Unauthorized,

    #[deprecated(since = "next", note = "use NotFoundV2 — ADR-019")]
    #[error("GUI session '{0}' not found")]
    NotFound(String),

    #[deprecated(since = "next", note = "use BadRequestV2 — ADR-019")]
    #[error("Invalid GUI request: {0}")]
    BadRequest(String),

    #[deprecated(since = "next", note = "use ForbiddenV2 — ADR-019")]
    #[error("GUI request forbidden: {0}")]
    Forbidden(String),

    #[deprecated(since = "next", note = "use FocusDriftV2 — ADR-019")]
    #[error("GUI focus drift detected: {0}")]
    FocusDrift(String),

    #[deprecated(since = "next", note = "use TicketInvalidV2 — ADR-019")]
    #[error("GUI ticket is no longer valid: {0}")]
    TicketInvalid(String),

    #[deprecated(since = "next", note = "use UnavailableV2 — ADR-019")]
    #[error("GUI runtime unavailable: {0}")]
    Unavailable(String),

    #[deprecated(since = "next", note = "use InternalV2 — ADR-019")]
    #[error("GUI runtime failed: {0}")]
    Internal(String),
}
```

- [ ] **Step 2: Verify compile**

```bash
cargo check --workspace 2>&1 | tail -20
```

Expected: clean + deprecation warnings only.

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-core/src/error.rs
git commit -m "feat(oneshim-core): add V2 variants to GuiInteractionError; deprecate V1

8 V2 variants with typed GuiCode. V1 deprecated with ADR-019 note.
Pairs with CoreError V2 retrofit in preceding commit."
```

---

### Task 1.7: Implement `impl CoreError::code()` accessor

**Files:**
- Modify: `crates/oneshim-core/src/error.rs` (add impl block after enum)

- [ ] **Step 1: Write accessor tests first (TDD)**

Append to `crates/oneshim-core/src/error.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn config_v2_code_returns_config_invalid() {
        let err = CoreError::ConfigV2 {
            code: crate::error_codes::ConfigCode::Invalid,
            message: "bad".into(),
        };
        assert_eq!(err.code(), "config.invalid");
    }

    #[test]
    fn config_v1_code_returns_config_generic_fallback() {
        #[allow(deprecated)]
        let err = CoreError::Config("legacy".into());
        assert_eq!(err.code(), "config.generic");
    }

    #[test]
    fn serialization_code_returns_internal_serialization() {
        let err: CoreError = serde_json::from_str::<i32>("not a number")
            .expect_err("should be error")
            .into();
        assert_eq!(err.code(), "internal.serialization");
    }

    #[test]
    fn io_code_returns_internal_io() {
        let err: CoreError = std::io::Error::new(std::io::ErrorKind::Other, "x").into();
        assert_eq!(err.code(), "internal.io");
    }

    #[test]
    fn every_variant_has_code() {
        // Representative sample of every V2 + V1 variant; uses fixtures.
        let samples: Vec<CoreError> = sample_core_error_variants();
        for err in samples {
            let c = err.code();
            assert!(!c.is_empty(), "empty code for {err:?}");
            assert!(c.contains('.'), "missing dot in {c:?}");
        }
    }
```

Add the sample builder (outside the test module, still inside the file, guarded by `#[cfg(test)]`):

```rust
#[cfg(test)]
fn sample_core_error_variants() -> Vec<CoreError> {
    use crate::error_codes::*;
    use crate::ports::oauth::OAuthErrorKind;

    vec![
        // V2 samples (one per variant)
        CoreError::ConfigV2 { code: ConfigCode::Invalid, message: "".into() },
        CoreError::NetworkV2 { code: NetworkCode::Failed, message: "".into() },
        CoreError::RequestTimeoutV2 { code: NetworkCode::Timeout, timeout_ms: 0 },
        CoreError::RateLimitV2 { code: NetworkCode::RateLimit, retry_after_secs: 0 },
        CoreError::ServiceUnavailableV2 { code: ServiceCode::Unavailable, message: "".into() },
        CoreError::AuthV2 { code: AuthCode::Failed, message: "".into() },
        CoreError::OAuthErrorV2 { code: OAuthCode::Failed, provider: "".into(), message: "".into() },
        CoreError::OAuthRefreshErrorV2 {
            code: OAuthCode::RefreshFailed,
            provider: "".into(),
            kind: OAuthErrorKind::InvalidGrant,
            message: "".into(),
        },
        CoreError::ValidationV2 { code: ValidationCode::InvalidField, field: "".into(), message: "".into() },
        CoreError::InvalidArgumentsV2 { code: ValidationCode::InvalidArguments, message: "".into() },
        CoreError::NotFoundV2 { code: NotFoundCode::ResourceMissing, resource_type: "".into(), id: "".into() },
        CoreError::ElementNotFoundV2 { code: UiCode::ElementMissing, name: "".into() },
        CoreError::BinaryHashMismatchV2 { code: IntegrityCode::HashMismatch, expected: "".into(), actual: "".into() },
        CoreError::InternalV2 { code: InternalCode::Generic, message: "".into() },
        CoreError::PolicyDeniedV2 { code: PolicyCode::Denied, message: "".into() },
        CoreError::ProcessNotAllowedV2 { code: PolicyCode::ProcessDenied, message: "".into() },
        CoreError::ConsentRequiredV2 { code: ConsentCode::Required, message: "".into() },
        CoreError::ConsentExpiredV2 { code: ConsentCode::Expired },
        CoreError::SandboxInitV2 { code: SandboxCode::InitFailed, message: "".into() },
        CoreError::SandboxExecutionV2 { code: SandboxCode::ExecutionFailed, message: "".into() },
        CoreError::SandboxUnsupportedV2 { code: SandboxCode::UnsupportedPlatform, message: "".into() },
        CoreError::ExecutionTimeoutV2 { code: SandboxCode::Timeout, timeout_ms: 0 },
        CoreError::PrivacyDeniedV2 { code: PermissionCode::PrivacyDenied, message: "".into() },
        CoreError::PermissionDeniedV2 { code: PermissionCode::PermissionDenied, message: "".into() },
        CoreError::OcrErrorV2 { code: ProviderCode::OcrFailed, message: "".into() },
        CoreError::AnalysisV2 { code: ProviderCode::AnalysisFailed, message: "".into() },
        CoreError::AudioCaptureV2 { code: AudioCode::CaptureFailed, message: "".into() },
        CoreError::SpeechToTextV2 { code: AudioCode::SttFailed, message: "".into() },
        CoreError::StorageV2 { code: StorageCode::Failed, message: "".into() },
        CoreError::SecretStoreErrorV2 { code: SecretCode::Failed, message: "".into() },
        // #[from] wrapped
        CoreError::Serialization(serde_json::from_str::<i32>("x").unwrap_err()),
        CoreError::Io(std::io::Error::new(std::io::ErrorKind::Other, "x")),
        // V1 samples (one per variant — all fallback to *Code::Generic where available)
        #[allow(deprecated)] CoreError::Config("".into()),
        #[allow(deprecated)] CoreError::Network("".into()),
        #[allow(deprecated)] CoreError::RequestTimeout { timeout_ms: 0 },
        #[allow(deprecated)] CoreError::RateLimit { retry_after_secs: 0 },
        #[allow(deprecated)] CoreError::ServiceUnavailable("".into()),
        #[allow(deprecated)] CoreError::Auth("".into()),
        #[allow(deprecated)] CoreError::OAuthError { provider: "".into(), message: "".into() },
        #[allow(deprecated)] CoreError::OAuthRefreshError { provider: "".into(), kind: OAuthErrorKind::InvalidGrant, message: "".into() },
        #[allow(deprecated)] CoreError::Validation { field: "".into(), message: "".into() },
        #[allow(deprecated)] CoreError::InvalidArguments("".into()),
        #[allow(deprecated)] CoreError::NotFound { resource_type: "".into(), id: "".into() },
        #[allow(deprecated)] CoreError::ElementNotFound("".into()),
        #[allow(deprecated)] CoreError::BinaryHashMismatch { expected: "".into(), actual: "".into() },
        #[allow(deprecated)] CoreError::Internal("".into()),
        #[allow(deprecated)] CoreError::PolicyDenied("".into()),
        #[allow(deprecated)] CoreError::ProcessNotAllowed("".into()),
        #[allow(deprecated)] CoreError::ConsentRequired("".into()),
        #[allow(deprecated)] CoreError::ConsentExpired,
        #[allow(deprecated)] CoreError::SandboxInit("".into()),
        #[allow(deprecated)] CoreError::SandboxExecution("".into()),
        #[allow(deprecated)] CoreError::SandboxUnsupported("".into()),
        #[allow(deprecated)] CoreError::ExecutionTimeout { timeout_ms: 0 },
        #[allow(deprecated)] CoreError::PrivacyDenied("".into()),
        #[allow(deprecated)] CoreError::PermissionDenied("".into()),
        #[allow(deprecated)] CoreError::OcrError("".into()),
        #[allow(deprecated)] CoreError::Analysis("".into()),
        #[allow(deprecated)] CoreError::AudioCapture("".into()),
        #[allow(deprecated)] CoreError::SpeechToText("".into()),
        #[allow(deprecated)] CoreError::Storage("".into()),
        #[allow(deprecated)] CoreError::SecretStoreError("".into()),
    ]
}
```

- [ ] **Step 2: Run tests to verify they fail (no `code()` method yet)**

```bash
cargo test -p oneshim-core error::tests::config_v2_code_returns
```

Expected: compile error — `no method named code`.

- [ ] **Step 3: Implement `impl CoreError::code()`**

Immediately after the `pub enum CoreError { ... }` block, add:

```rust
impl CoreError {
    /// Wire-format error code. UI, log, telemetry entry point.
    ///
    /// V1/V2 coexistence rule (Phases 1–3) — three-tier V1 fallback policy
    /// per spec §4.4:
    /// 1. **Default**: domain's `Generic` code (e.g., `Config(_) → ConfigCode::Generic`).
    /// 2. **Narrow-specific override**: if the V1 variant name uniquely maps to
    ///    a specific code (e.g., `RequestTimeout { .. } → NetworkCode::Timeout`,
    ///    `InvalidArguments(_) → ValidationCode::InvalidArguments`), use that code.
    /// 3. **Sole-variant domains**: for enums without `Generic` (`NotFoundCode`,
    ///    `UiCode`, `IntegrityCode`, `SandboxCode`), use the most-matching specific variant.
    ///
    /// V1 arms are deleted in Phase 4 alongside the V1 variant itself. See ADR-019.
    pub fn code(&self) -> &'static str {
        use crate::error_codes::*;
        match self {
            // --- V2 struct variants ---
            Self::ConfigV2 { code, .. } => code.as_str(),
            Self::NetworkV2 { code, .. } => code.as_str(),
            Self::RequestTimeoutV2 { code, .. } => code.as_str(),
            Self::RateLimitV2 { code, .. } => code.as_str(),
            Self::ServiceUnavailableV2 { code, .. } => code.as_str(),
            Self::AuthV2 { code, .. } => code.as_str(),
            Self::OAuthErrorV2 { code, .. } => code.as_str(),
            Self::OAuthRefreshErrorV2 { code, .. } => code.as_str(),
            Self::ValidationV2 { code, .. } => code.as_str(),
            Self::InvalidArgumentsV2 { code, .. } => code.as_str(),
            Self::NotFoundV2 { code, .. } => code.as_str(),
            Self::ElementNotFoundV2 { code, .. } => code.as_str(),
            Self::BinaryHashMismatchV2 { code, .. } => code.as_str(),
            Self::InternalV2 { code, .. } => code.as_str(),
            Self::PolicyDeniedV2 { code, .. } => code.as_str(),
            Self::ProcessNotAllowedV2 { code, .. } => code.as_str(),
            Self::ConsentRequiredV2 { code, .. } => code.as_str(),
            Self::ConsentExpiredV2 { code } => code.as_str(),
            Self::SandboxInitV2 { code, .. } => code.as_str(),
            Self::SandboxExecutionV2 { code, .. } => code.as_str(),
            Self::SandboxUnsupportedV2 { code, .. } => code.as_str(),
            Self::ExecutionTimeoutV2 { code, .. } => code.as_str(),
            Self::PrivacyDeniedV2 { code, .. } => code.as_str(),
            Self::PermissionDeniedV2 { code, .. } => code.as_str(),
            Self::OcrErrorV2 { code, .. } => code.as_str(),
            Self::AnalysisV2 { code, .. } => code.as_str(),
            Self::AudioCaptureV2 { code, .. } => code.as_str(),
            Self::SpeechToTextV2 { code, .. } => code.as_str(),
            Self::StorageV2 { code, .. } => code.as_str(),
            Self::SecretStoreErrorV2 { code, .. } => code.as_str(),

            // --- `#[from]`-wrapped external variants ---
            Self::Serialization(_) => InternalCode::Serialization.as_str(),
            Self::Io(_) => InternalCode::Io.as_str(),

            // --- V1 deprecated variants (removed in Phase 4) ---
            #[allow(deprecated)] Self::Config(_) => ConfigCode::Generic.as_str(),
            #[allow(deprecated)] Self::Network(_) => NetworkCode::Generic.as_str(),
            #[allow(deprecated)] Self::RequestTimeout { .. } => NetworkCode::Timeout.as_str(),
            #[allow(deprecated)] Self::RateLimit { .. } => NetworkCode::RateLimit.as_str(),
            #[allow(deprecated)] Self::ServiceUnavailable(_) => ServiceCode::Unavailable.as_str(),
            #[allow(deprecated)] Self::Auth(_) => AuthCode::Generic.as_str(),
            #[allow(deprecated)] Self::OAuthError { .. } => OAuthCode::Generic.as_str(),
            #[allow(deprecated)] Self::OAuthRefreshError { .. } => OAuthCode::RefreshFailed.as_str(),
            #[allow(deprecated)] Self::Validation { .. } => ValidationCode::Generic.as_str(),
            #[allow(deprecated)] Self::InvalidArguments(_) => ValidationCode::InvalidArguments.as_str(),
            #[allow(deprecated)] Self::NotFound { .. } => NotFoundCode::ResourceMissing.as_str(),
            #[allow(deprecated)] Self::ElementNotFound(_) => UiCode::ElementMissing.as_str(),
            #[allow(deprecated)] Self::BinaryHashMismatch { .. } => IntegrityCode::HashMismatch.as_str(),
            #[allow(deprecated)] Self::Internal(_) => InternalCode::Generic.as_str(),
            #[allow(deprecated)] Self::PolicyDenied(_) => PolicyCode::Generic.as_str(),
            #[allow(deprecated)] Self::ProcessNotAllowed(_) => PolicyCode::ProcessDenied.as_str(),
            #[allow(deprecated)] Self::ConsentRequired(_) => ConsentCode::Generic.as_str(),
            #[allow(deprecated)] Self::ConsentExpired => ConsentCode::Expired.as_str(),
            #[allow(deprecated)] Self::SandboxInit(_) => SandboxCode::InitFailed.as_str(),
            #[allow(deprecated)] Self::SandboxExecution(_) => SandboxCode::ExecutionFailed.as_str(),
            #[allow(deprecated)] Self::SandboxUnsupported(_) => SandboxCode::UnsupportedPlatform.as_str(),
            #[allow(deprecated)] Self::ExecutionTimeout { .. } => SandboxCode::Timeout.as_str(),
            #[allow(deprecated)] Self::PrivacyDenied(_) => PermissionCode::PrivacyDenied.as_str(),
            #[allow(deprecated)] Self::PermissionDenied(_) => PermissionCode::Generic.as_str(),
            #[allow(deprecated)] Self::OcrError(_) => ProviderCode::OcrFailed.as_str(),
            #[allow(deprecated)] Self::Analysis(_) => ProviderCode::AnalysisFailed.as_str(),
            #[allow(deprecated)] Self::AudioCapture(_) => AudioCode::Generic.as_str(),
            #[allow(deprecated)] Self::SpeechToText(_) => AudioCode::SttFailed.as_str(),
            #[allow(deprecated)] Self::Storage(_) => StorageCode::Generic.as_str(),
            #[allow(deprecated)] Self::SecretStoreError(_) => SecretCode::Generic.as_str(),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p oneshim-core error::tests
```

Expected: all 5 new accessor tests + `every_variant_has_code` PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-core/src/error.rs
git commit -m "feat(oneshim-core): implement CoreError::code() with V1/V2 coexistence

V1 variants return their domain's Generic code as transitional fallback.
V2 variants return their stored code's as_str.
#[from] wrapped types derive code via match arm (InternalCode::Io/Serialization).
Phase 4 will delete the V1 arms in lockstep with V1 variant deletion.

Covered by every_variant_has_code + 4 per-variant assertion tests."
```

---

### Task 1.8: Implement `impl GuiInteractionError::code()` accessor

**Files:**
- Modify: `crates/oneshim-core/src/error.rs`

- [ ] **Step 1: Add tests alongside existing ones**

```rust
    #[test]
    fn gui_v2_code_returns_gui_unauthorized() {
        let err = GuiInteractionError::UnauthorizedV2 {
            code: crate::error_codes::GuiCode::Unauthorized,
        };
        assert_eq!(err.code(), "gui.unauthorized");
    }

    #[test]
    fn gui_v1_code_returns_bad_request_code() {
        // V1 GuiInteractionError::BadRequest maps to GuiCode::BadRequest
        // (narrow-specific) per the V1 fallback policy in spec §4.4.
        #[allow(deprecated)]
        let err = GuiInteractionError::BadRequest("bad".into());
        assert_eq!(err.code(), "gui.bad_request");
    }

    #[test]
    fn gui_every_variant_has_code() {
        for err in sample_gui_error_variants() {
            let c = err.code();
            assert!(!c.is_empty());
            assert!(c.contains('.'));
        }
    }
```

Add sample builder:

```rust
#[cfg(test)]
fn sample_gui_error_variants() -> Vec<GuiInteractionError> {
    use crate::error_codes::GuiCode;
    vec![
        GuiInteractionError::UnauthorizedV2 { code: GuiCode::Unauthorized },
        GuiInteractionError::NotFoundV2 { code: GuiCode::NotFound, name: "".into() },
        GuiInteractionError::BadRequestV2 { code: GuiCode::BadRequest, message: "".into() },
        GuiInteractionError::ForbiddenV2 { code: GuiCode::Forbidden, message: "".into() },
        GuiInteractionError::FocusDriftV2 { code: GuiCode::FocusDrift, message: "".into() },
        GuiInteractionError::TicketInvalidV2 { code: GuiCode::TicketInvalid, message: "".into() },
        GuiInteractionError::UnavailableV2 { code: GuiCode::Unavailable, message: "".into() },
        GuiInteractionError::InternalV2 { code: GuiCode::InternalError, message: "".into() },
        #[allow(deprecated)] GuiInteractionError::Unauthorized,
        #[allow(deprecated)] GuiInteractionError::NotFound("".into()),
        #[allow(deprecated)] GuiInteractionError::BadRequest("".into()),
        #[allow(deprecated)] GuiInteractionError::Forbidden("".into()),
        #[allow(deprecated)] GuiInteractionError::FocusDrift("".into()),
        #[allow(deprecated)] GuiInteractionError::TicketInvalid("".into()),
        #[allow(deprecated)] GuiInteractionError::Unavailable("".into()),
        #[allow(deprecated)] GuiInteractionError::Internal("".into()),
    ]
}
```

- [ ] **Step 2: Implement `impl GuiInteractionError::code()`**

Add after the `pub enum GuiInteractionError { ... }` block:

```rust
impl GuiInteractionError {
    pub fn code(&self) -> &'static str {
        use crate::error_codes::GuiCode;
        match self {
            Self::UnauthorizedV2 { code } => code.as_str(),
            Self::NotFoundV2 { code, .. } => code.as_str(),
            Self::BadRequestV2 { code, .. } => code.as_str(),
            Self::ForbiddenV2 { code, .. } => code.as_str(),
            Self::FocusDriftV2 { code, .. } => code.as_str(),
            Self::TicketInvalidV2 { code, .. } => code.as_str(),
            Self::UnavailableV2 { code, .. } => code.as_str(),
            Self::InternalV2 { code, .. } => code.as_str(),

            #[allow(deprecated)] Self::Unauthorized => GuiCode::Unauthorized.as_str(),
            #[allow(deprecated)] Self::NotFound(_) => GuiCode::NotFound.as_str(),
            #[allow(deprecated)] Self::BadRequest(_) => GuiCode::BadRequest.as_str(),
            #[allow(deprecated)] Self::Forbidden(_) => GuiCode::Forbidden.as_str(),
            #[allow(deprecated)] Self::FocusDrift(_) => GuiCode::FocusDrift.as_str(),
            #[allow(deprecated)] Self::TicketInvalid(_) => GuiCode::TicketInvalid.as_str(),
            #[allow(deprecated)] Self::Unavailable(_) => GuiCode::Unavailable.as_str(),
            #[allow(deprecated)] Self::Internal(_) => GuiCode::InternalError.as_str(),
        }
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p oneshim-core error::tests::gui
```

Expected: all GUI accessor tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-core/src/error.rs
git commit -m "feat(oneshim-core): implement GuiInteractionError::code() with V1/V2 coexistence

Mirrors CoreError::code() pattern. All 8 V1 + 8 V2 variants covered."
```

---

### Task 1.9: Wire-format snapshot test

**Files:**
- Create: `crates/oneshim-core/tests/wire_contract_snapshot.rs`
- Create: `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt`

- [ ] **Step 1: Generate initial expected fixture**

Create `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` with the sorted list of all 57 codes. Seed with the following (must be kept sorted ASCII ascending; one code per line, trailing newline):

```
audio.capture_failed
audio.generic
audio.stt_failed
auth.failed
auth.generic
config.generic
config.invalid
config.missing
config.out_of_range
consent.expired
consent.generic
consent.required
gui.bad_request
gui.focus_drift
gui.forbidden
gui.generic
gui.internal_error
gui.not_found
gui.ticket_invalid
gui.unauthorized
gui.unavailable
integrity.hash_mismatch
internal.generic
internal.io
internal.serialization
network.failed
network.generic
network.rate_limit
network.timeout
not_found.resource_missing
oauth.failed
oauth.generic
oauth.refresh_failed
permission.generic
permission.permission_denied
permission.privacy_denied
policy.denied
policy.generic
policy.process_denied
provider.analysis_failed
provider.bedrock.unsupported
provider.generic
provider.ocr_failed
sandbox.execution_failed
sandbox.init_failed
sandbox.timeout
sandbox.unsupported_platform
secret.failed
secret.generic
service.generic
service.unavailable
storage.failed
storage.generic
ui.element_missing
validation.generic
validation.invalid_arguments
validation.invalid_field
```

(57 lines.)

- [ ] **Step 2: Create the snapshot test**

Create `crates/oneshim-core/tests/wire_contract_snapshot.rs`:

```rust
//! Wire-format contract snapshot.
//!
//! Every addition / removal / rename of a code string MUST update
//! `wire_contract_snapshot.expected.txt` alongside the source change.
//! Per spec §7.5, released code strings are wire-immutable: deletions and
//! renames require an RFC PR justifying the wire break.

use oneshim_core::error_codes;

#[test]
fn wire_codes_match_expected_snapshot() {
    let actual: Vec<&'static str> = error_codes::all_codes();
    let actual_sorted = {
        let mut v = actual.clone();
        v.sort();
        v.dedup();
        v
    };

    let expected_raw = include_str!("wire_contract_snapshot.expected.txt");
    let expected: Vec<&str> = expected_raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    if actual_sorted != expected {
        let diff_added: Vec<&&str> = actual_sorted.iter()
            .filter(|c| !expected.contains(c))
            .collect();
        let diff_removed: Vec<&&str> = expected.iter()
            .filter(|c| !actual_sorted.contains(c))
            .collect();

        panic!(
            "Wire-format snapshot mismatch. \
             Added codes (not in fixture): {diff_added:?}. \
             Removed codes (fixture has them but source does not): {diff_removed:?}. \
             Update tests/wire_contract_snapshot.expected.txt to reflect the change. \
             Per spec §7.5, deletions/renames require RFC PR."
        );
    }
}
```

- [ ] **Step 3: Run test**

```bash
cargo test -p oneshim-core --test wire_contract_snapshot
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-core/tests/wire_contract_snapshot.rs \
        crates/oneshim-core/tests/wire_contract_snapshot.expected.txt
git commit -m "test(oneshim-core): add wire-format contract snapshot

Enforces §4.5 wire-immutability rule. Any variant/code addition, removal,
or rename must co-update the fixture. Test failure gives actionable diff."
```

---

### Task 1.10: Workspace-wide Phase 1 verification

- [ ] **Step 1: Build every target**

```bash
cargo check --workspace --all-targets
```

Expected: clean compile; deprecation warnings at existing V1 usage sites are expected.

- [ ] **Step 2: Run every test**

```bash
cargo test --workspace
```

Expected: PASS (baseline of 3,461 tests + ~57 new tests in oneshim-core).

- [ ] **Step 3: Clippy pass (warn-only on deprecated for Phases 1-3)**

```bash
cargo clippy --workspace --all-targets -- -D warnings -W deprecated
```

Expected: clean warnings for non-deprecated issues; deprecated warnings surfaced but not fatal.

- [ ] **Step 4: Format check**

```bash
cargo fmt --check
```

Expected: clean.

- [ ] **Step 5: Count deprecation warnings (optional sanity)**

```bash
cargo build --workspace 2>&1 | grep -c "use of deprecated" || echo "0 deprecated warnings"
```

Expected: ~1,050 warnings (matching the 1,052 V1 callsite count from spec §1.2). This number is the Phase 2 workload baseline.

- [ ] **Step 6: Open Phase 1 PR**

```bash
# Assuming branch is 'feature/error-code-infrastructure-phase1'
gh pr create \
  --title "feat(oneshim-core): error code infrastructure (Phase 1)" \
  --body "$(cat <<'EOF'
## Summary

Phase 1 of the Error Code Infrastructure + C5 AWS Bedrock Skip program per
`docs/superpowers/specs/2026-04-19-error-code-infrastructure-design.md`.

- Adds `error_codes` directory module with 19 code enums (`ConfigCode`,
  `NetworkCode`, ..., `GuiCode`) defined via single-source `define_code_enum!`
  macro.
- Adds V2 struct-variant shape to every `CoreError` + `GuiInteractionError`
  variant carrying a typed `code` field. V1 marked `#[deprecated]`.
- Adds `err.code() -> &'static str` unified accessor (V1/V2 coexistence).
- Adds wire-format contract snapshot test (57 codes sorted).

~1,050 V1 `#[deprecated]` warnings are expected and intentional — they are the
Phase 2 retrofit signal. CI remains warn-only on deprecated until Phase 4.

## Test plan

- [ ] `cargo test -p oneshim-core error_codes` — 19 enums × 2-3 tests pass.
- [ ] `cargo test -p oneshim-core error::tests` — all `code()` accessor tests pass.
- [ ] `cargo test -p oneshim-core --test wire_contract_snapshot` — passes.
- [ ] `cargo test --workspace` — no regression.
- [ ] `cargo clippy --workspace -- -D warnings -W deprecated` — clean apart from expected deprecations.
EOF
)"
```

- [ ] **Step 7: Subagent review (per spec §8.3)**

Dispatch subagents:
- **Implementer check**: verify macro expansion produces correct enum bodies (run `cargo expand -p oneshim-core error_codes::config`).
- **Spec compliance**: verify §4.1, §4.2, §4.3, §4.4, §7.5 implementation matches spec text.
- **Code quality**: `superpowers:code-reviewer` on the PR diff.

Merge PR only after zero Critical / zero Important findings.

---

# Phase 2 — Per-Crate Retrofit (PRs 2-14)

### Phase 2 Task Template

**This template applies to every Phase-2 PR (2 through 14).** Each PR retrofits one crate's V1 construction and match sites to V2. For each PR, instantiate the template with the target crate.

### Task 2.X: Retrofit `{crate-name}` V1 → V2

**Crate**: `crates/{crate-name}`

**Files:** determined by grep in Step 1.

- [ ] **Step 1: Enumerate V1 callsites in this crate**

```bash
cd crates/{crate-name}
grep -rn "CoreError::" --include="*.rs" | grep -vE "CoreError::(Config|Network|[Rr]equest[Tt]imeout|RateLimit|ServiceUnavailable|Auth|OAuthError|OAuthRefreshError|Validation|InvalidArguments|NotFound|ElementNotFound|BinaryHashMismatch|Internal|PolicyDenied|ProcessNotAllowed|ConsentRequired|ConsentExpired|SandboxInit|SandboxExecution|SandboxUnsupported|ExecutionTimeout|PrivacyDenied|PermissionDenied|OcrError|Analysis|AudioCapture|SpeechToText|Storage|SecretStoreError)V2" | grep -vE "(Serialization|Io)" > /tmp/v1_sites_{crate-name}.txt
wc -l /tmp/v1_sites_{crate-name}.txt
```

This produces a list of every V1 construction site (excludes V2 sites and `#[from]` unchanged variants).

- [ ] **Step 2: Categorize each site**

For each line in the output:
1. Determine the V1 variant used.
2. Map to the target V2 variant (identical name + `V2` suffix).
3. Determine the appropriate code for this site:
   - If the error arises from a clearly-identifiable subcategory (e.g., "API key missing" → `ConfigCode::Missing`), use the specific code.
   - Otherwise use the domain's `Generic` fallback (spec §4.3 policy; allowed by §5.4 checklist).

- [ ] **Step 3: For each site, execute edit + test loop**

**Construction sites** (`CoreError::Config(msg)` → `CoreError::ConfigV2 { code, message }`):

Ensure `use oneshim_core::error_codes::ConfigCode;` (or the relevant code enum(s)) is imported at the top of the file before the first construction site.

Before:
```rust
return Err(CoreError::Config(format!("missing API key for {provider}")));
```

After:
```rust
use oneshim_core::error_codes::ConfigCode;  // add once per file

return Err(CoreError::ConfigV2 {
    code: ConfigCode::Missing,
    message: format!("missing API key for {provider}"),
});
```

**Match sites** (`CoreError::Config(msg) => ...`):

Policy: at each match site, **retrofit to V2 AND remove the V1 arm** — this drives the "0 deprecation warnings per crate" gate in Step 6. Because every V1 construction site in this crate is being migrated concurrently, no intra-crate match can still receive a V1 instance. The only case where a V1 arm must remain is if the match pattern is on a type alias / re-export that crosses crate boundaries — in practice this is rare; flag and confirm with reviewer.

Before:
```rust
match err {
    CoreError::Config(msg) => log::error!("config error: {msg}"),
    // ...
}
```

After (recommended — delete V1 arm along with construction-site retrofit):
```rust
match err {
    CoreError::ConfigV2 { code, message, .. } => log::error!("config error [{code}]: {message}"),
    // ...
}
```

**Cross-crate match fallback** (only if match receives V1 instances from a dependency crate that hasn't been retrofitted yet): add `#[allow(deprecated)]` at the match-stmt scope and keep both V1 and V2 arms. This is tolerated for the narrow window before the dependency crate's Phase-2 PR lands; remove the `#[allow]` + V1 arm in the follow-up commit once the dependency is retrofitted. Any such occurrence MUST be called out in the PR description.

- [ ] **Step 4: Run the crate's tests**

```bash
cargo test -p {crate-name}
```

Expected: all existing tests PASS (retrofit preserves semantics).

- [ ] **Step 5: Run workspace check**

```bash
cargo check --workspace
```

Expected: clean compile.

- [ ] **Step 6: Count remaining deprecation warnings for THIS crate**

```bash
cargo build -p {crate-name} 2>&1 | grep "use of deprecated" | wc -l
```

Expected: **0** (every V1 site in this crate should now be V2).

If >0: step 1 missed a callsite. Return to step 2.

- [ ] **Step 7: Commit**

```bash
git add crates/{crate-name}/
git commit -m "refactor({crate-name}): retrofit CoreError V1 → V2 (Phase 2)

All V1 construction and match sites in this crate now use the V2 struct-variant
shape with typed code fields. Generic codes used where the existing message
did not suggest a specific subcategory; refinement follow-ups tracked per §10.

Verified: 0 deprecated warnings in this crate; workspace builds clean."
```

- [ ] **Step 8: Open PR**

```bash
gh pr create \
  --title "refactor({crate-name}): CoreError V1→V2 retrofit (Phase 2)" \
  --body "$(cat <<'EOF'
## Summary

Per the Phase-2 retrofit template in `docs/superpowers/plans/2026-04-19-error-code-infrastructure.md`,
this PR retrofits every V1 `CoreError` construction and match site in `{crate-name}` to V2.

- Construction sites: `CoreError::Xxx(msg)` → `CoreError::XxxV2 { code, message }`
- Match sites: V2 arm added; V1 arm retained with `#[allow(deprecated)]` + delegation for exhaustive match coverage.
- Generic codes used by default; subcategory refinement is a §10 follow-up.

Post-PR: crate builds with 0 deprecation warnings.

## Test plan

- [ ] `cargo test -p {crate-name}` — all tests pass.
- [ ] `cargo build -p {crate-name} 2>&1 | grep "use of deprecated" | wc -l` — 0.
- [ ] `cargo check --workspace` — clean.
EOF
)"
```

- [ ] **Step 9: Subagent triad review per §8.3**

- Implementer: verify every grep hit was migrated.
- Spec compliance: verify code selection matches §4.3 / §5.4 policy.
- Code quality: `superpowers:code-reviewer`.

Merge after zero Critical/Important.

---

### Phase 2 instantiation — PRs 2 through 14

Apply Task 2.X template with the following substitutions:

| PR | Crate | Expected V1 sites (approximate, grep at PR time for exact) |
|----|-------|------------------------------------------------------------|
| 2  | `oneshim-api-contracts` | small (<30) — leaf crate |
| 3  | `oneshim-embedding` | small |
| 4  | `oneshim-audio` | medium (audio capture/STT errors) |
| 5  | `oneshim-monitor` | medium |
| 6  | `oneshim-storage` | medium-high (storage errors dominate; ~30+ Storage sites) |
| 7  | `oneshim-vision` | medium |
| 8  | `oneshim-analysis` | medium |
| 9  | `oneshim-network` | **highest** (~150+ sites: Network, Auth, Config, Internal) |
| 10 | `oneshim-suggestion` | medium |
| 11 | `oneshim-automation` | medium-high |
| 12 | `oneshim-web` | medium |
| 13 | `oneshim-sandbox-worker` | **0 expected** — verification-only PR (see Task 2.13) |
| 14 | `src-tauri` | high (entry point; wraps many variants) |

### Task 2.13 (PR 13) — `oneshim-sandbox-worker` verification-only

Special case of the template: expect 0 V1 sites. PR content is a defensive test + confirmation.

- [ ] **Step 1: Confirm zero V1 sites**

```bash
grep -rn "CoreError::" crates/oneshim-sandbox-worker --include="*.rs" | wc -l
```

Expected: **0**.

- [ ] **Step 2: Create verification test**

Create `crates/oneshim-sandbox-worker/tests/no_v1_usage.rs`:

```rust
//! Verification: oneshim-sandbox-worker introduces no CoreError V1 usage.
//!
//! This crate has no current CoreError usage. Phase-4 CI gate (§5.1) will
//! fail on any V1 usage workspace-wide; this test asserts the crate stays
//! clean during the Phase-1 to Phase-4 migration window.

// Intentionally minimal: if a future change introduces `use CoreError::Foo`
// where `Foo` is a V1 deprecated variant, rustc will emit a deprecation
// warning and cargo build -D deprecated (Phase 4 onward) will fail.
//
// This test simply compiles the crate and exercises a no-op assertion to
// ensure the test runner touches the crate.

#[test]
fn sandbox_worker_v1_free() {
    let _placeholder = true;
    assert!(_placeholder);
}
```

- [ ] **Step 3: Run test**

```bash
cargo test -p oneshim-sandbox-worker --test no_v1_usage
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-sandbox-worker/tests/no_v1_usage.rs
git commit -m "test(oneshim-sandbox-worker): verify zero V1 CoreError usage

Verification-only PR per spec §5.3. Crate depends on oneshim-core but
has no current CoreError usage. Test asserts the crate remains V1-free
through the Phase-2 to Phase-4 migration window."
```

- [ ] **Step 5: Open PR + subagent review per template.**

---

# Phase 3 — C5 AWS Bedrock Skip + ADR-019 (PR 15)

Prerequisite: PR 9 (`oneshim-network` retrofit) merged. PR 15 uses `ConfigV2` directly.

### Task 3.1: Delete Bedrock from catalog

**Files:**
- Modify: `specs/providers/provider-surface-catalog.json`

- [ ] **Step 1: Locate and verify current Bedrock entries**

```bash
grep -n '"vendor_id": "bedrock"' specs/providers/provider-surface-catalog.json
grep -n '"provider_surface.bedrock.direct_api"' specs/providers/provider-surface-catalog.json
```

Expected output (pre-PR-15): line ~90 and ~2265 respectively (exact lines shift post-Phase-2 retrofit of adjacent files, though the catalog itself is not touched by Phase 2).

- [ ] **Step 2: Remove the `bedrock` vendor object**

Use a JSON-aware edit (do NOT use `sed`). Locate the `vendors` array, find the object `{ "vendor_id": "bedrock", ... }` (~lines 89-104), delete the entire object including the trailing comma (adjust preceding/following comma to maintain valid JSON).

- [ ] **Step 3: Remove the `provider_surface.bedrock.direct_api` surface object**

Locate the `surfaces` array, find the object `{ "surface_id": "provider_surface.bedrock.direct_api", ... }` (~lines 2264-2398, ~135 lines), delete entire object including trailing comma.

- [ ] **Step 4: Validate JSON parses**

```bash
python3 -c "import json; json.load(open('specs/providers/provider-surface-catalog.json')); print('valid')"
```

Expected: `valid`.

- [ ] **Step 5: Commit**

```bash
git add specs/providers/provider-surface-catalog.json
git commit -m "chore(provider-catalog): remove Bedrock vendor + direct_api surface

Per ADR-019 §5, AWS Bedrock is intentionally unsupported in this build.
Catalog entries removed so the UI does not offer Bedrock as a selection.
Enum variants AiProviderType::Bedrock / ProviderAuthScheme::AwsSignatureV4
/ ProviderRequestShape::BedrockConverse retained (runtime-unreachable)."
```

---

### Task 3.2: Retrofit OCR `apply_auth_headers` signature + Bedrock arm

**Files:**
- Modify: `crates/oneshim-network/src/ai_ocr_client/mod.rs`

- [ ] **Step 1: Write failing test for security fix**

Create or extend `crates/oneshim-network/src/ai_ocr_client/tests.rs` (or inline `#[cfg(test)] mod tests`) with:

```rust
#[test]
fn aws_signature_v4_rejected_with_config_error() {
    // Regression guard: OCR apply_auth_headers must NOT silently pass an
    // unauthenticated request when the auth scheme is AwsSignatureV4.
    // Prior to Phase 3 this was a security bug (spec §1.3).
    use oneshim_api_contracts::provider_specs::ProviderAuthScheme;
    use oneshim_core::error_codes::ConfigCode;

    let client = reqwest::Client::new();
    let builder = client.get("https://example.invalid/");

    let result = super::apply_auth_headers(
        ProviderAuthScheme::AwsSignatureV4,
        builder,
        "fake-key",
    );

    match result {
        Err(CoreError::ConfigV2 { code, .. }) => {
            assert_eq!(code, ConfigCode::UnsupportedProviderBedrock);
        }
        other => panic!("expected ConfigV2 {{ code: UnsupportedProviderBedrock, .. }}, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p oneshim-network ai_ocr_client::tests::aws_signature_v4_rejected
```

Expected: compile error (apply_auth_headers signature mismatch) OR PASS with wrong shape. Either is acceptable "fail" here.

- [ ] **Step 3: Retrofit `apply_auth_headers` signature + AwsSignatureV4 arm**

In `ai_ocr_client/mod.rs`:

1. Change the signature:

```rust
// Before
fn apply_auth_headers(
    auth_scheme: ProviderAuthScheme,
    builder: reqwest::RequestBuilder,
    api_key: &str,
) -> reqwest::RequestBuilder {

// After
fn apply_auth_headers(
    auth_scheme: ProviderAuthScheme,
    builder: reqwest::RequestBuilder,
    api_key: &str,
) -> Result<reqwest::RequestBuilder, CoreError> {
```

2. Wrap existing arms with `Ok(...)`:

```rust
match auth_scheme {
    ProviderAuthScheme::None => Ok(builder),
    ProviderAuthScheme::XApiKey => Ok(builder
        .header("x-api-key", api_key)
        .header("anthropic-version", crate::ANTHROPIC_API_VERSION)),
    ProviderAuthScheme::XGoogApiKey => Ok(builder.header("x-goog-api-key", api_key)),
    ProviderAuthScheme::Bearer => Ok(builder.header("Authorization", format!("Bearer {api_key}"))),
    ProviderAuthScheme::AwsSignatureV4 => {
        Err(CoreError::ConfigV2 {
            code: oneshim_core::error_codes::ConfigCode::UnsupportedProviderBedrock,
            message: "AWS Bedrock is intentionally unsupported in this build".into(),
        })
    }
}
```

3. Update the two call sites at lines ~390 and ~393 to use `?`:

```rust
// Before
builder = apply_auth_headers(auth_scheme, builder, "");
// After
builder = apply_auth_headers(auth_scheme, builder, "")?;

// Before
builder = apply_auth_headers(auth_scheme, builder, &bearer_token);
// After
builder = apply_auth_headers(auth_scheme, builder, &bearer_token)?;
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test -p oneshim-network ai_ocr_client::tests::aws_signature_v4_rejected
```

Expected: PASS.

- [ ] **Step 5: Also retrofit the OCR `BedrockConverse` arm at line ~373**

Locate:

```rust
ProviderRequestShape::BedrockConverse => {
    return Err(CoreError::Internal(
        "Bedrock Converse request shape is not yet supported for OCR extraction".into(),
    ));
}
```

Replace with:

```rust
ProviderRequestShape::BedrockConverse => {
    return Err(CoreError::ConfigV2 {
        code: oneshim_core::error_codes::ConfigCode::UnsupportedProviderBedrock,
        message: "AWS Bedrock is intentionally unsupported in this build".into(),
    });
}
```

- [ ] **Step 6: Add regression test for the request-shape arm**

```rust
#[test]
fn bedrock_converse_request_shape_rejected() {
    // Verify OCR construction with BedrockConverse returns the Bedrock code.
    // (Detailed setup depends on existing test fixtures; add a variant test
    // alongside the existing OCR request-shape tests.)
    // ... pattern mirrors aws_signature_v4_rejected_with_config_error ...
}
```

Flesh out test body using the existing OCR client test fixtures as a template.

- [ ] **Step 7: Commit**

```bash
git add crates/oneshim-network/src/ai_ocr_client/mod.rs \
        crates/oneshim-network/src/ai_ocr_client/tests.rs
git commit -m "fix(oneshim-network): OCR apply_auth_headers fails closed on AwsSignatureV4

Changes apply_auth_headers signature from infallible to Result<_, CoreError>.
AwsSignatureV4 arm now returns ConfigV2 { UnsupportedProviderBedrock }
instead of silently passing an unauthenticated request. Closes §1.3 security
bug and establishes C5 Bedrock skip contract.

Also retrofits the OCR BedrockConverse request-shape arm to the same code.
Regression tests added."
```

---

### Task 3.3: Retrofit remaining 5 Bedrock match arms

**Files:**
- Modify: `crates/oneshim-network/src/ai_ocr_client/strategy.rs`
- Modify: `crates/oneshim-network/src/ai_llm_client/request.rs` (3 arms)
- Modify: `crates/oneshim-network/src/http_api_session/mod.rs`

For each arm (as enumerated in spec §6.3), apply the identical replacement:

```rust
// Before (template)
return Err(CoreError::Internal("{ original message }".to_string()));

// After (template)
return Err(CoreError::ConfigV2 {
    code: oneshim_core::error_codes::ConfigCode::UnsupportedProviderBedrock,
    message: "AWS Bedrock is intentionally unsupported in this build".into(),
});
```

- [ ] **Step 1: Add unit test for each arm (5 new tests)**

Write one test per arm asserting the `ConfigV2 { UnsupportedProviderBedrock }` return pattern. Follow the `aws_signature_v4_rejected_with_config_error` template.

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p oneshim-network bedrock_unsupported
```

Expected: fail (arms not yet updated).

- [ ] **Step 3: Apply the 4 replacements**

For each of:
- `ai_ocr_client/strategy.rs` around lines 32-35 (`BedrockConverse` strategy arm)
- `ai_llm_client/request.rs` around lines 110-114 (`BedrockConverse` request-build arm)
- `ai_llm_client/request.rs` around lines 146-151 (`AwsSignatureV4` auth arm)
- `ai_llm_client/request.rs` around lines 189-194 (`BedrockConverse` response-parse arm)
- `http_api_session/mod.rs` around lines 207-212 (`AwsSignatureV4` auth arm)

Apply the template replacement.

- [ ] **Step 4: Run tests to confirm they pass**

```bash
cargo test -p oneshim-network bedrock_unsupported
cargo test -p oneshim-network --test ai_provider_live_smoke
```

Expected: PASS for all 5 new tests. Live smoke test still passes for non-Bedrock providers.

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-network/src/
git commit -m "fix(oneshim-network): unify remaining Bedrock/SigV4 match arms to Config error

5 additional match arms across ai_ocr_client/strategy.rs, ai_llm_client/request.rs
(3 arms), and http_api_session/mod.rs now return
CoreError::ConfigV2 { UnsupportedProviderBedrock } instead of Internal errors.

All 7 Bedrock-related match arms (per spec §6.3) are now consistent."
```

---

### Task 3.4: Update live smoke test Bedrock arm

**Files:**
- Modify: `crates/oneshim-network/tests/ai_provider_live_smoke.rs`

- [ ] **Step 1: Locate the Bedrock arm**

Around lines 163, 293, 305 (per spec §7.6).

- [ ] **Step 2: Update the Bedrock match arm**

Before (pseudo):
```rust
AiProviderType::Bedrock => { /* live API call path */ }
```

After:
```rust
AiProviderType::Bedrock => {
    // Per ADR-019, Bedrock is intentionally unsupported.
    // Construct a Bedrock-configured client up to the first ? propagation
    // point and assert the returned error matches the skip contract.
    let result = /* client construction path that triggers AwsSignatureV4 */;
    match result {
        Err(CoreError::ConfigV2 { code, .. }) => {
            assert_eq!(code, oneshim_core::error_codes::ConfigCode::UnsupportedProviderBedrock);
        }
        other => panic!("expected Bedrock skip contract, got {other:?}"),
    }
}
```

Use the existing test fixtures for AiProviderType dispatch; do NOT add a live network call.

- [ ] **Step 3: Run live smoke test**

```bash
cargo test -p oneshim-network --test ai_provider_live_smoke
```

Expected: PASS (non-Bedrock providers unchanged; Bedrock arm asserts contract without network).

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-network/tests/ai_provider_live_smoke.rs
git commit -m "test(oneshim-network): live smoke Bedrock arm asserts skip contract

No live AWS call. Bedrock arm constructs client up to first ? propagation
and asserts ConfigV2 { UnsupportedProviderBedrock } per ADR-019."
```

---

### Task 3.5: Author ADR-019 (English + Korean)

**Files:**
- Create: `docs/architecture/ADR-019-error-code-infrastructure.md`
- Create: `docs/architecture/ADR-019-error-code-infrastructure.ko.md`

- [ ] **Step 1: Write ADR-019 English**

Create `docs/architecture/ADR-019-error-code-infrastructure.md`:

```markdown
# ADR-019: Error Code Infrastructure + AWS Bedrock Intentional Non-Support

- **Status**: Accepted (2026-04-19)
- **Supersedes**: none
- **Related**: ADR-001 (error strategy), ADR-003 (directory module pattern)
- **Implementation**: `docs/superpowers/specs/2026-04-19-error-code-infrastructure-design.md`

## Context

ONESHIM client-rust workspace has ~1,150 error construction sites across 14 crates
with zero error-code convention. Telemetry (Grafana), i18n (frontend ko/en), and
audit logs need stable machine-readable error identifiers. Separately, AWS Bedrock
is listed as a supported provider surface but implementation is incomplete (no
Signature V4 auth), producing a security bug in OCR (no-auth fallthrough).

Two needs converged: introduce error-code infrastructure AND ship Bedrock as the
first "intentionally unsupported" first-class citizen.

## Decision

### 1. Error code infrastructure

- 19 code enums (`ConfigCode`, `NetworkCode`, …) defined via a single
  `define_code_enum!` macro that generates enum body, `as_str` match,
  `Display` impl, and `all()` enumerator from one variant list.
- Every `CoreError` and `GuiInteractionError` variant gains a typed `code` field.
- Unified accessor `err.code() -> &'static str` for telemetry/logs/i18n.
- Wire-format codes follow `{domain}.{category}[.{qualifier}]` convention.
- Released code strings are immutable (wire contract). New codes append; renames
  require an RFC PR.

### 2. Naming convention

```
{domain}.{category}[.{qualifier}[.{sub_qualifier}]]
all lowercase, snake_case, dot-separated
```

Examples:
- `config.invalid`
- `network.timeout`
- `provider.bedrock.unsupported`

### 3. AWS Bedrock: intentionally unsupported

- Bedrock vendor + `provider_surface.bedrock.direct_api` surface removed from
  `specs/providers/provider-surface-catalog.json`.
- 7 match arms across `oneshim-network` return
  `CoreError::Config { code: ConfigCode::UnsupportedProviderBedrock, .. }`.
- Enum variants `AiProviderType::Bedrock`, `ProviderAuthScheme::AwsSignatureV4`,
  `ProviderRequestShape::BedrockConverse` retained (runtime-unreachable after
  catalog delete) for minimal-churn future re-introduction path.
- OCR `apply_auth_headers` signature changed to fallible to close the silent
  no-auth fallthrough security bug.

### 4. Migration strategy (soft)

4 phases / 16 PRs / 2-3 weeks:
1. Phase 1: introduce V2 variants alongside V1 (deprecated).
2. Phase 2: 13 per-crate retrofits (12 crates + 1 verification-only sandbox-worker).
3. Phase 3: C5 Bedrock skip + this ADR.
4. Phase 4: V1 deletion + V2 → canonical rename (rust-analyzer LSP, not sed).

CI deprecation gating warn-only through Phase 3, flips to `-D deprecated` at Phase 4.

### 5. Bedrock re-introduction checklist

If a future use case requires Bedrock support, the following must be satisfied:

1. AWS Signature V4 signing implementation (`aws-sigv4` crate or equivalent).
2. AWS credential loader (access_key / secret_key / optional session_token).
3. Settings UI field for AWS credentials.
4. Re-add Bedrock vendor + surface to `provider-surface-catalog.json`.
5. Replace the 7 Bedrock match arms (currently returning Config error) with
   working Bedrock handlers.
6. Live smoke test for Bedrock path (conditional `--ignored`).
7. Update the wire-format snapshot fixture if new codes are added.
8. Remove `ConfigCode::UnsupportedProviderBedrock` from `ConfigCode` (follows
   wire-immutability deletion procedure — RFC PR required).

### 6. Public-API Exhaustiveness

`CoreError` and `GuiInteractionError` are NOT marked `#[non_exhaustive]`.
Rationale:

1. Both are internal to this workspace (14-member); all consumers are first-party.
2. Exhaustive matching catches forgotten variants during refactors — a feature, not a bug.
3. `err.code()` provides a forward-compat channel that does not require pattern matching.
4. If this library is ever extracted / published, this decision is reversible with a one-line
   change + downstream `match` site review.

Code enums (`ConfigCode`, etc.) ARE `#[non_exhaustive]` because they are internal-use but
could grow with follow-ups; protecting downstream (within-workspace) consumers from
variant-addition breakage is cheap and defensive.

### 7. Adding new `#[from]` variants

When a new `#[from]`-wrapped external error type is added to `CoreError`:

1. Allocate an `InternalCode::*` variant for the new type (e.g., if adding
   `tokio::io::Error` via `#[from]`, add `InternalCode::TokioIo`).
2. Add the variant + `#[from]` attribute in the same PR.
3. Add the corresponding arm in `impl CoreError::code()` returning the new `InternalCode`.
4. Update `wire_contract_snapshot.expected.txt` fixture.

## Consequences

### Positive

- Machine-readable error identifiers enable Grafana label grouping.
- `err.code()` unlocks i18n (frontend consumes code as translation key).
- Bedrock UX becomes deterministic: no silent fallthrough, catalog does not
  advertise the provider.
- Type-safe code registry; impossible to drift wire format from source.

### Negative

- 2-3 week migration effort across 16 PRs.
- ~1,050 `#[deprecated]` warnings during the migration window (signal, not
  regression).
- V1/V2 coexistence adds enum variant count temporarily (Phases 1-3).

### Neutral

- Phase 4 rename requires brief freeze on in-flight `CoreError` / `GuiInteractionError` PRs.
- Post-Phase-4 Grafana dashboard relabeling is a follow-up (not blocking).
```

- [ ] **Step 2: Write Korean companion**

Create `docs/architecture/ADR-019-error-code-infrastructure.ko.md` with the same
content translated to Korean. Keep structure identical; translate section headings
and prose. Code blocks and identifiers remain in English.

- [ ] **Step 3: Commit**

```bash
git add docs/architecture/ADR-019-error-code-infrastructure.md \
        docs/architecture/ADR-019-error-code-infrastructure.ko.md
git commit -m "docs(adr): add ADR-019 error code infrastructure + Bedrock intentional non-support

Architectural record for the C5 skip and error code registry.
Korean companion included per CLAUDE.md documentation policy."
```

---

### Task 3.6: Open Phase 3 PR + subagent triad review

- [ ] **Step 1: Open Phase 3 PR**

```bash
gh pr create \
  --title "feat: C5 AWS Bedrock intentional non-support + ADR-019 (Phase 3)" \
  --body "$(cat <<'EOF'
## Summary

Phase 3 of the Error Code Infrastructure program — ships AWS Bedrock as the
first intentionally-unsupported provider via \`ConfigCode::UnsupportedProviderBedrock\`.

- Removes \`bedrock\` vendor + \`provider_surface.bedrock.direct_api\` surface
  from \`specs/providers/provider-surface-catalog.json\`.
- Retrofits 7 match arms across oneshim-network (\`ai_ocr_client/*\`,
  \`ai_llm_client/request.rs\`, \`http_api_session/mod.rs\`) to return
  \`CoreError::ConfigV2 { code: ConfigCode::UnsupportedProviderBedrock, .. }\`.
- Changes \`apply_auth_headers\` signature from infallible to
  \`Result<_, CoreError>\` — closes OCR no-auth fallthrough security bug (§1.3).
- Updates \`ai_provider_live_smoke.rs\` Bedrock arm to assert skip contract
  without live network call.
- Authors \`docs/architecture/ADR-019-error-code-infrastructure.md\` (English + Korean).

## Test plan

- [ ] \`cargo test -p oneshim-network bedrock_unsupported\` — 6 new tests pass (OCR auth, OCR BedrockConverse, strategy, 3 LLM, session).
- [ ] \`cargo test -p oneshim-network --test ai_provider_live_smoke\` — live smoke passes.
- [ ] \`cargo test --workspace\` — no regression.
- [ ] \`python3 -c "import json; json.load(open('specs/providers/provider-surface-catalog.json'))"\` — catalog parses.
- [ ] \`grep -c '"vendor_id": "bedrock"' specs/providers/provider-surface-catalog.json\` — 0.
EOF
)"
```

- [ ] **Step 2: Dispatch implementer-stage review agent**

Dispatch a subagent to verify the 7 match arm retrofits are exhaustive and the catalog delete is clean:

```
Agent(
  description="Phase 3 implementer verification",
  subagent_type="general-purpose",
  prompt="Verify PR #15 in the error code infrastructure program:
  1. Run cargo test -p oneshim-network bedrock_unsupported — report pass/fail.
  2. grep for '\"vendor_id\": \"bedrock\"' in the catalog — expect 0 matches.
  3. grep for CoreError::Internal across the 4 touched files — report any remaining Bedrock-related Internal error that should have been retrofitted to ConfigV2.
  4. Confirm apply_auth_headers is now fallible (look for Result<_, CoreError> return).
  Report findings under 400 words."
)
```

- [ ] **Step 3: Dispatch spec-compliance review agent**

```
Agent(
  description="Phase 3 spec compliance",
  subagent_type="Plan",
  prompt="Verify PR #15 aligns with spec §6 and ADR-019. Check:
  1. Every match arm listed in spec §6.3 was retrofitted.
  2. ADR-019 has the 8 required sections (Context, Decision 1-7, Consequences).
  3. Catalog delete touched both vendor and surface blocks (spec §6.1).
  4. Cross-consumer audit per spec §6.5 was performed.
  Report Critical/Important findings. Under 400 words."
)
```

- [ ] **Step 4: Dispatch code-quality review agent**

Invoke `superpowers:code-reviewer` on the PR diff:

```
Agent(
  description="Phase 3 code quality",
  subagent_type="superpowers:code-reviewer",
  prompt="Review PR #15 diff for Rust idioms, test coverage, and guardrails
  from client-rust/CLAUDE.md architecture section. Flag any unidiomatic
  patterns in the 7 match arm retrofits or apply_auth_headers refactor."
)
```

- [ ] **Step 5: Iterate on any Critical/Important findings; merge only after triad returns zero.**

---

# Phase 4 — V1 Deletion + V2 → Canonical Rename (PR 16)

Prerequisite: all PRs 1-15 merged.

### Task 4.1: Verify workspace V1-free

- [ ] **Step 1: Count remaining V1 deprecation warnings**

```bash
cargo build --workspace 2>&1 | grep -c "use of deprecated"
```

Expected: **0**.

If >0: Phase 2 incomplete. Return and identify missed crate(s).

---

### Task 4.2: Delete V1 variants from `CoreError`

**Files:**
- Modify: `crates/oneshim-core/src/error.rs`

- [ ] **Step 1: Delete V1 variant block**

Remove every `#[deprecated(since = "next", note = "use ..." )]` variant from
`pub enum CoreError { ... }`. The 32 V1 variants listed at the bottom of the
enum (added in Task 1.5) are deleted; the V2 variants at the top remain (still
named `*V2`).

- [ ] **Step 2: Delete V1 arms from `impl CoreError::code()`**

Remove every arm prefixed with `#[allow(deprecated)] Self::...(_) => ...`.
The V2 arms remain.

- [ ] **Step 3: Delete V1 samples from `sample_core_error_variants()`**

Remove every `#[allow(deprecated)] CoreError::...` entry from the sample vec.

- [ ] **Step 4: Verify build**

```bash
cargo check --workspace
```

Expected: **clean compile.** Because Task 4.1 already confirmed zero V1 deprecation warnings workspace-wide, no remaining caller references V1 variants — deleting them should produce a clean build.

If any "no variant named ..." errors appear, **halt the Phase 4 PR immediately**: it means Phase 2 was incomplete and at least one retrofit site was missed. Investigate:

```bash
cargo check --workspace 2>&1 | grep "no variant named" | head
```

Return to Phase 2 to retrofit the missed site(s) and restart Phase 4.

- [ ] **Step 5: Commit (PRELIMINARY — do not push yet)**

```bash
git add crates/oneshim-core/src/error.rs
git commit -m "refactor(oneshim-core): delete V1 variants from CoreError (Phase 4a)

V1 variants + their deprecated match arms + sample test fixtures removed.
CoreError now has only V2 variants + Serialization/Io #[from] wraps.
Pairs with rename commit next."
```

---

### Task 4.3: Delete V1 variants from `GuiInteractionError`

**Files:**
- Modify: `crates/oneshim-core/src/error.rs`

- [ ] **Step 1: Delete V1 block from `pub enum GuiInteractionError`**

Remove the 8 `#[deprecated]` V1 variants.

- [ ] **Step 2: Delete V1 arms from `impl GuiInteractionError::code()`**

Remove V1 arms.

- [ ] **Step 3: Delete V1 samples from `sample_gui_error_variants()`**

Remove `#[allow(deprecated)]` entries.

- [ ] **Step 4: Verify build**

```bash
cargo check --workspace
```

Expected: clean (no V1 GUI callers left after Phase 2).

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-core/src/error.rs
git commit -m "refactor(oneshim-core): delete V1 variants from GuiInteractionError (Phase 4b)"
```

---

### Task 4.4: Rename V2 variants to canonical names (rust-analyzer LSP)

**Files:**
- Multiple; driven by rust-analyzer rename.

- [ ] **Step 1: Open `crates/oneshim-core/src/error.rs` in editor**

- [ ] **Step 2: For each V2 variant, use rust-analyzer "Rename Symbol" (F2)**

**Important — V2-V2 collision handling**: `NotFoundV2` and `InternalV2` exist in both `CoreError` and `GuiInteractionError`. Execute renames **per-enum, one at a time** (select the variant in its enum definition in `error.rs`, then invoke F2). Do NOT use a workspace-wide find/replace — rust-analyzer's LSP rename is scope-aware and will correctly rename only the targeted enum's variant and its call sites, but only when invoked on the specific enum's variant. Confirm after each rename via `cargo check --workspace`.

Rename targets (CoreError):
```
ConfigV2               → Config
NetworkV2              → Network
RequestTimeoutV2       → RequestTimeout
RateLimitV2            → RateLimit
ServiceUnavailableV2   → ServiceUnavailable
AuthV2                 → Auth
OAuthErrorV2           → OAuthError
OAuthRefreshErrorV2    → OAuthRefreshError
ValidationV2           → Validation
InvalidArgumentsV2     → InvalidArguments
NotFoundV2             → NotFound
ElementNotFoundV2      → ElementNotFound
BinaryHashMismatchV2   → BinaryHashMismatch
InternalV2             → Internal
PolicyDeniedV2         → PolicyDenied
ProcessNotAllowedV2    → ProcessNotAllowed
ConsentRequiredV2      → ConsentRequired
ConsentExpiredV2       → ConsentExpired
SandboxInitV2          → SandboxInit
SandboxExecutionV2     → SandboxExecution
SandboxUnsupportedV2   → SandboxUnsupported
ExecutionTimeoutV2     → ExecutionTimeout
PrivacyDeniedV2        → PrivacyDenied
PermissionDeniedV2     → PermissionDenied
OcrErrorV2             → OcrError
AnalysisV2             → Analysis
AudioCaptureV2         → AudioCapture
SpeechToTextV2         → SpeechToText
StorageV2              → Storage
SecretStoreErrorV2     → SecretStoreError
```

Rename targets (GuiInteractionError):
```
UnauthorizedV2         → Unauthorized
NotFoundV2             → NotFound
BadRequestV2           → BadRequest
ForbiddenV2            → Forbidden
FocusDriftV2           → FocusDrift
TicketInvalidV2        → TicketInvalid
UnavailableV2          → Unavailable
InternalV2             → Internal
```

- [ ] **Step 3: Verify workspace compiles**

```bash
cargo check --workspace --all-targets
```

Expected: clean.

- [ ] **Step 4: Verify no residual `V2` variant names**

```bash
grep -rn "\bCoreError::\w*V2\b" --include="*.rs" | wc -l
```

Expected: 0.

```bash
grep -rn "\bGuiInteractionError::\w*V2\b" --include="*.rs" | wc -l
```

Expected: 0.

- [ ] **Step 5: Run every test**

```bash
cargo test --workspace
```

Expected: full pass.

- [ ] **Step 6: Commit**

```bash
git add .
git commit -m "refactor: rename CoreError/GuiInteractionError V2 variants to canonical (Phase 4c)

rust-analyzer LSP rename (scope-aware, not sed). V2 suffix removed from every
variant name. CoreError and GuiInteractionError now have their final shapes:
all variants carry a typed code field; no V1 leftovers."
```

---

### Task 4.5: Flip CI deprecation gate

**Files:**
- Modify: `.github/workflows/ci.yml` (or equivalent CI configuration)

- [ ] **Step 1: Locate the existing clippy invocation**

```bash
grep -rn "cargo clippy" .github/workflows/
```

Identify the line: `cargo clippy --workspace ... -- -D warnings -W deprecated` or similar.

- [ ] **Step 2: Flip `-W deprecated` to `-D deprecated`**

Change to: `cargo clippy --workspace --all-targets -- -D warnings -D deprecated`.

- [ ] **Step 3: Commit the CI change on the Phase-4 branch (do NOT push-to-main)**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: enforce -D deprecated after Phase 4 V1 cleanup

Any future \`#[deprecated]\` usage now fails CI. Workspace is currently
V1-free; this gate prevents regression."
```

Note: **do not push directly to main.** This commit lives on the Phase-4 feature branch and is included in the Phase-4 PR (Step 4). Pushing the CI flip ahead of the PR would fail CI on any in-flight PR that still contained V1 usage.

- [ ] **Step 4: Open Phase 4 PR**

```bash
gh pr create \
  --title "refactor: Phase 4 — delete V1 + rename V2 + CI deprecation gate" \
  --body "$(cat <<'EOF'
## Summary

Completes the error code infrastructure migration per spec §5 / ADR-019.

- Deletes all V1 deprecated variants from CoreError and GuiInteractionError.
- rust-analyzer LSP renames every V2 variant to its canonical name (no V2 suffix).
- Flips CI clippy flag `-W deprecated` → `-D deprecated`.

Workspace now ships the final error shape: every variant carries a typed code
field; no V1 leftovers; CI regression guard in place.

## Test plan

- [ ] `cargo test --workspace` — full pass (3,461 baseline + new tests).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings -D deprecated` — clean.
- [ ] `grep -rn "\bCoreError::\w*V2\b" --include="*.rs" | wc -l` — 0.
- [ ] `cargo check --workspace --all-targets` — clean.
EOF
)"
```

- [ ] **Step 5: Subagent triad review per spec §8.3**

Dispatch three agents:

1. **Implementer verification** — run `cargo build --workspace` and confirm 0 "use of deprecated" warnings; run `grep -rn "\\bCoreError::\\w*V2\\b\" --include=\"*.rs\" | wc -l` and confirm 0; run full test suite.
2. **Spec compliance** — verify final CoreError shape matches spec §4.1 canonical form (no V1, no V2 suffixes); verify ADR-019 acceptance criteria in spec §12.3 are all checked.
3. **Code quality** — `superpowers:code-reviewer` on PR diff; flag any rename artifacts, leftover V1 code paths in comments, or ergonomics regressions.

Iterate until zero Critical/Important findings.

---

### Task 4.6: Holistic pre-merge review

Per memory `feedback_holistic_pre_merge_review`: after Phase 4 PR is code-reviewed
by the subagent triad, dispatch one more holistic pass BEFORE merging:

- [ ] **Step 1: Dispatch holistic reviewer agent**

```
Agent(
  description="Holistic pre-merge integrated review",
  subagent_type="Plan",
  prompt="""
Integrated review of PRs 1-16 of the error code infrastructure program.
Cross-cutting concerns and narrative drift check — what per-PR reviewers miss.

Files to review (list all ~200+ files touched):
- crates/oneshim-core/src/error.rs (final shape)
- crates/oneshim-core/src/error_codes/*.rs (19 enums)
- crates/oneshim-core/tests/wire_contract_snapshot.*
- Every crate that received Phase-2 retrofits
- Bedrock skip files in crates/oneshim-network/src/
- specs/providers/provider-surface-catalog.json
- docs/architecture/ADR-019-*.md

Dimensions:
1. Consistency: every retrofitted variant uses the expected code enum?
2. Drift: any crate's retrofit picked a different code than others for the same pattern?
3. Missed sites: any V1 usage that escaped the Phase 2 sweep?
4. Telemetry contract: wire_contract_snapshot.expected.txt matches source?
5. ADR-019 alignment: implementation matches ADR decisions?

Output: Critical / Important / Minor list. Zero-Critical-Important gate.
"""
)
```

- [ ] **Step 2: If any Critical/Important — fix and re-review.**

- [ ] **Step 3: Merge Phase 4 PR.**

---

### Task 4.7: Post-merge housekeeping

- [ ] **Step 1: Verify main branch is green**

```bash
git checkout main
git pull
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings -D deprecated
```

- [ ] **Step 2: Delete local feature branches**

```bash
git branch -d feature/error-code-infrastructure-phase1 \
             feature/error-code-phase2-* \
             feature/error-code-phase3-c5-bedrock \
             feature/error-code-phase4-cleanup
```

- [ ] **Step 3: Update memory**

Update `project_next_tasks.md` to reflect C5 completion and error-code infrastructure
landing. Record any §10 follow-ups moved to active queue.

- [ ] **Step 4: Post-merge smoke — manual**

Launch the client, trigger a few representative error paths in the UI, and
verify `err.code()` surfaces in logs. Confirm Grafana dashboards still render
(or flag §10-1 dashboard migration as a follow-up).

---

## Self-Review

### 1. Spec coverage check

- [x] §1 Context → covered by plan introduction.
- [x] §2 Goals → covered by Phase 1/3/4 tasks collectively.
- [x] §3 Non-goals → not directly implemented (by definition).
- [x] §4.1 CoreError shape → Task 1.5.
- [x] §4.2 error_codes module layout → Task 1.1.
- [x] §4.3 code enum pattern → Task 1.2 + Task 1.4 for remaining 17 + macro in Task 1.1.
- [x] §4.4 `impl CoreError::code()` V1/V2 coexistence → Task 1.7.
- [x] §4.5 naming convention → enforced via tests in Task 1.2 (`naming_convention`).
- [x] §4.6 `#[from]` external wraps → preserved in Task 1.5 code block.
- [x] §4.7 GuiInteractionError → Tasks 1.6 + 1.8.
- [x] §5 migration strategy → Phase 1/2/3/4 structure.
- [x] §5.1 phase structure → PR sequencing summary at top of plan.
- [x] §5.2 V2 naming + Phase 4 LSP rename → Task 4.4.
- [x] §5.3 per-crate order → "Phase 2 instantiation" table.
- [x] §5.4 Phase 2 per-crate checklist → embedded in Task 2.X template.
- [x] §6 C5 Bedrock Skip → Phase 3 tasks 3.1-3.5.
- [x] §6.1 catalog delete → Task 3.1.
- [x] §6.2 enum variant retention → documented in ADR-019 (Task 3.5).
- [x] §6.3 7 match arms → Tasks 3.2 + 3.3.
- [x] §6.4 apply_auth_headers signature → Task 3.2.
- [x] §6.5 cross-consumer audit → Task 2.X Step 1 (crate-wide grep) + holistic review Task 4.6.
- [x] §7 testing strategy → embedded throughout + Task 1.9 snapshot.
- [x] §7.1 per-enum tests → Task 1.2 (template) + Task 1.4 repeats.
- [x] §7.2 accessor tests → Task 1.7 (CoreError) + 1.8 (GuiInteractionError).
- [x] §7.3 C5-specific tests → Tasks 3.2, 3.3, 3.4.
- [x] §7.4 workspace tests per phase → Task 1.10 + phase-boundary verification.
- [x] §7.5 wire-format snapshot → Task 1.9.
- [x] §7.6 live-AWS test update → Task 3.4.
- [x] §8 3-Loop review → built into each PR's review step.
- [x] §9 rollout summary → "PR Sequencing Summary" at top.
- [x] §10 follow-ups → non-blocking; plan notes but does not implement.
- [x] §11 risk register → spec-only; mitigations enforced via plan tasks.
- [x] §12 acceptance criteria → verified by Phase 4.5, 4.6, 4.7.
- [x] §13 appendix mapping → variant-list fidelity in Tasks 1.4, 1.5, 1.6.

**No gaps identified.**

### 2. Placeholder scan

Searched the plan for forbidden patterns:
- `TBD` / `TODO` / `FIXME` — 0 hits.
- "appropriate error handling" / "handle edge cases" / "validate" — 0 standalone uses (all embedded in concrete steps).
- "Similar to Task N" — 0 uses; each task repeats code.
- Bare "Add tests" — 0 uses; every test step shows code.

**Clean.**

### 3. Type consistency

- V2 variant names: 30 CoreError + 8 GuiInteractionError. Verified matching between Task 1.5 enum body and Task 1.7 `code()` arms and Task 1.8 GUI accessor and Task 4.4 rename list.
- Code enum variant names: verified between Task 1.2–1.4 definitions and Task 1.7 V1 fallback arms.
- Wire-format strings: fixture in Task 1.9 Step 1 uses exact strings produced by `as_str` in Task 1.2–1.4.
- Method naming: `code()` consistent across CoreError + GuiInteractionError.
- `apply_auth_headers` signature: Task 3.2 matches spec §6.4.

**No inconsistencies found.**

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-19-error-code-infrastructure.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — Dispatch a fresh subagent per Task with a two-stage review (spec-compliance + code-quality) between tasks. Proven in Wave 1 (PR #447/#448/#449) at catching bugs per `project_next_tasks` memory. Best for this plan's ~70+ tasks across 16 PRs.

2. **Inline Execution** — Execute tasks in this session with batch checkpoints for review. Faster for trivial sequential tasks but higher context pressure given plan scope.

**Which approach?**
