# Error Code Infrastructure + C5 AWS Bedrock Skip — Design Spec

- **Date**: 2026-04-19
- **Status**: Draft (pending review loop)
- **Author**: richard.kim0828@gmail.com
- **Related**:
  - C5 entry of `docs/reviews/2026-04-16-feature-gaps-analysis.md`
  - ADR-019 (to be authored alongside implementation) — Error Code Infrastructure
  - ADR-001 §1 (Rust client error strategy)
  - ADR-003 (Directory module pattern)
  - Memory patterns: `feedback_cross_consumer_audit`, `feedback_holistic_pre_merge_review`, `feedback_3loop_quality_gate`

---

## 1. Context & Problem

### 1.1 Trigger

Wave 2 entry item **C5** from the 2026-04-16 feature gap analysis noted that AWS Signature V4 is unimplemented in three AI client surfaces (OCR / LLM / Session). Original decision choice was *implement (~1.5 weeks)* vs *skip via documentation (~0.5 day)*. User chose **skip** because current user base has no AWS Bedrock / Textract requirement.

### 1.2 Expanded scope discovered during design

During design of the skip, review revealed a deeper infrastructure gap: the workspace has **zero error-code convention** despite:

- **Scale**: 14 crates, 200+ source files, 3,461 tests, **1,052** `CoreError` construction + match + reference sites across 185 Rust files + **97** `GuiInteractionError` sites. (Line-based count via `grep -rn "CoreError::" --include="*.rs" | grep -cE "CoreError::[A-Z]"` on 2026-04-19. Note: earlier draft estimate of 2,104/194 double-counted lines with multi-matches due to `grep -o` semantics; corrected here. Actual construction-only site count is a subset — distinct pattern counts below.)
- **Telemetry pipeline**: Grafana dashboards consume production logs and would benefit from stable `code` labels for error grouping (currently grouping by human-readable message — fragile).
- **i18n already active** on the frontend (ko/en). Rust-originated error messages currently bypass i18n because they are opaque strings.
- **Audit logs** (oneshim-automation) benefit from stable codes for compliance.

Two options emerged for scope:
1. Minimal — handle only C5 with ad-hoc `CoreError::Config` messages (original 0.5 d).
2. Maximal — retrofit the full workspace with structured error codes, then implement C5 on top of it.

User selected option 2 with explicit acknowledgement of the 2–3 week timeline cost: *"오래 걸려도 구조적으로 문제가 없도록 하려고 한다"*.

### 1.3 Current state

```rust
// crates/oneshim-core/src/error.rs — existing shape (32 variants)
pub enum CoreError {
    Config(String),
    Network(String),
    Internal(String),
    Validation { field: String, message: String },
    ConsentExpired,
    // ... 32 variants total, all message-only or ad-hoc structured
}

// Construction / match / reference sites workspace-wide (top variants by line count):
// Internal 416  Config 85  Network 84  ServiceUnavailable 43  Auth 43
// InvalidArguments 37  Storage 30  SandboxExecution 30  NotFound 30
// AudioCapture 26  ...  (long tail through to single-digit variants)
// Workspace total: 1,052 CoreError lines + 97 GuiInteractionError lines = 1,149.
```

**Gaps**:
1. No machine-readable discriminator beyond variant type → Grafana must grep human text.
2. Category information lossy at logs (`Config(...)` of `bedrock unsupported` looks same as `Config(...)` of `bad json`).
3. Refactoring error text breaks telemetry silently.
4. `ai_ocr_client::apply_auth_headers` silently falls through on `AwsSignatureV4` → **existing security bug** (no-auth request sent to Bedrock endpoints if provider misconfigured).

### 1.4 C5 anchor

The AWS Bedrock skip is the first **concrete fine-grained error code** on top of the new infrastructure, validating the design end-to-end.

---

## 2. Goals

1. Introduce a **centrally managed, type-safe error code registry** covering all `CoreError` and `GuiInteractionError` variants.
2. Every variant carries a `code` field of a strongly-typed enum (not `&'static str`), enforcing category × code consistency at compile time.
3. Provide a single entry point `err.code() -> &'static str` for telemetry, logs, and (future) i18n consumers.
4. Naming convention: `{domain}.{category}[.{qualifier}]` dotted lowercase snake_case, stable across releases once shipped.
5. **Soft migration (V1→V2→V1)**: every intermediate state on `main` must build and pass tests.
6. **Full conversion**: no V1 variants remain at end state.
7. C5 AWS Bedrock unsupported realized as `ConfigCode::UnsupportedProviderBedrock` → `"provider.bedrock.unsupported"` across 7 match arms. Bedrock catalog entry deleted. OCR no-auth fallthrough security bug fixed en route.
8. ADR-019 authored as governance record.

---

## 3. Non-goals

- Retrofitting external error types (`reqwest::Error`, `rusqlite::Error`, etc.) — they remain wrapped via `#[from]` with inherited codes (see §4.2).
- i18n localization of error `message` strings — the code registry is the foundation; translation is a separate future initiative.
- Error code governance for `anyhow::Error` in the `src-tauri` binary crate — `anyhow` is a catch-all for top-level bubbling; code lookup on the anyhow chain is a future concern.
- Implementing AWS Signature V4 — explicitly deferred; re-introduction requires ADR-019 §"Re-enable checklist".
- Consolidating existing overlapping variants (e.g., `SandboxInit` + `SandboxExecution` + `SandboxUnsupported`) — out of scope for this spec. If done, a separate follow-up.
- Granular `Internal` code taxonomy beyond a handful of categories — Internal is catch-all and will have `Generic` as 80%+ default; granular refinement is out of scope.
- Updating Grafana dashboards to use the new `err.code()` label — dashboard work is a follow-up (see §10).

---

## 4. Design — Error Code Registry

### 4.1 CoreError variant shape (after migration)

All variants carry a typed `code` field plus their existing payload:

```rust
pub enum CoreError {
    // Tuple variants → struct variants
    Config { code: ConfigCode, message: String },
    Network { code: NetworkCode, message: String },
    Internal { code: InternalCode, message: String },
    Auth { code: AuthCode, message: String },
    // ...

    // Unit variants → struct with just code
    ConsentExpired { code: ConsentCode },  // code: ConsentCode::Expired

    // Structured variants → existing fields + code
    Validation { code: ValidationCode, field: String, message: String },
    NotFound { code: NotFoundCode, resource_type: String, id: String },
    RequestTimeout { code: NetworkCode, timeout_ms: u64 },  // code: NetworkCode::Timeout
    RateLimit { code: NetworkCode, retry_after_secs: u64 },  // code: NetworkCode::RateLimit
    BinaryHashMismatch { code: IntegrityCode, expected: String, actual: String },
    ExecutionTimeout { code: SandboxCode, timeout_ms: u64 },
    OAuthError { code: OAuthCode, provider: String, message: String },
    OAuthRefreshError { code: OAuthCode, provider: String, kind: OAuthErrorKind, message: String },

    // #[from]-wrapped external errors keep wrapper + gain derived code via impl
    Serialization(#[from] serde_json::Error),
    Io(#[from] std::io::Error),
    // code resolution: match arm returns InternalCode::Serialization / InternalCode::Io
}
```

**Rationale**:

- Tuple variants migrated to **struct variants** for field-name clarity (`code` + `message`) and to enable future field additions without call-site breakage.
- Unit variants (`ConsentExpired`) get `code` field for uniformity; `ConsentCode::Expired` is the only code here but keeping the shape consistent avoids special-casing at `err.code()`.
- `#[from]` variants keep their wrapper but the `code()` accessor derives codes from variant type (e.g., `InternalCode::Serialization`, `InternalCode::Io`).

**`#[non_exhaustive]` on `CoreError` / `GuiInteractionError` — intentionally omitted**.

Unlike the code enums in §4.3 (which are `#[non_exhaustive]` to protect downstream-of-core consumers from variant additions), `CoreError` and `GuiInteractionError` are **not** marked `#[non_exhaustive]`. Rationale:

1. Both types are *internal* to this workspace. Every consumer is a first-party crate within the 14-member workspace (per §1.2) where we control call sites.
2. Exhaustive matching on the outer error type is a desirable property during Phase 2 retrofits: if a reviewer forgets to handle a new variant, rustc surfaces the gap rather than a silent `_ => ...` fallthrough.
3. `err.code()` gives consumers a wire-format-stable discriminator that does not require pattern matching, so forward-compat concerns can be addressed via `code()` rather than `#[non_exhaustive]` where needed.
4. If this library is ever extracted / published outside the workspace, revisiting this decision is a cheap one-line change + a follow-up review of downstream `match` sites.

This decision is logged in ADR-019 §"Public-API Exhaustiveness" as an architectural default that can be revisited.

### 4.2 `error_codes` module (directory module style per ADR-003)

ADR-003 prescribes splitting files >500 lines with SRP violations. `error.rs` is currently ~148 lines so the split is not *triggered* by ADR-003, but the new `error_codes/` directory follows the same `mod.rs + sibling files + pub use` structure as style guidance.


```
crates/oneshim-core/src/
├── error.rs            # CoreError, GuiInteractionError, impl *::code()
└── error_codes/        # new directory module
    ├── mod.rs          # public re-exports + shared helper macro (`define_code!`)
    ├── config.rs       # ConfigCode
    ├── network.rs      # NetworkCode
    ├── auth.rs         # AuthCode
    ├── internal.rs     # InternalCode
    ├── validation.rs   # ValidationCode
    ├── not_found.rs    # NotFoundCode
    ├── consent.rs      # ConsentCode
    ├── integrity.rs    # IntegrityCode  (BinaryHashMismatch)
    ├── sandbox.rs      # SandboxCode  (SandboxInit/Execution/Unsupported/ExecutionTimeout)
    ├── policy.rs       # PolicyCode  (PolicyDenied/ProcessNotAllowed)
    ├── permission.rs   # PermissionCode  (PermissionDenied/PrivacyDenied)
    ├── oauth.rs        # OAuthCode  (OAuthError/OAuthRefreshError)
    ├── secret.rs       # SecretCode
    ├── provider.rs     # ProviderCode  (OcrError/Analysis/plus UnsupportedProvider)
    ├── audio.rs        # AudioCode  (AudioCapture/SpeechToText)
    ├── storage.rs      # StorageCode
    ├── ui.rs           # ElementNotFound → UiCode
    ├── service.rs      # ServiceUnavailable → ServiceCode
    └── gui.rs          # GuiCode  (for GuiInteractionError)
```

**19 code enum files** (one per logical category). Grouping maps **many-to-one** from existing `CoreError` variants — e.g., `SandboxCode` serves 4 existing variants. This reduces enum proliferation without losing semantic fidelity. `CoreError::Io` and `CoreError::Serialization` (both `#[from]` wrapped external errors) derive their codes from `InternalCode::Io` / `InternalCode::Serialization` — no dedicated `IoCode` enum.

### 4.3 Code enum pattern (template)

```rust
// crates/oneshim-core/src/error_codes/config.rs

/// Config 카테고리 에러 코드.
///
/// 네이밍: `config.*` 접두사. 신규 코드 추가 시 ADR-019 §2 컨벤션 준수.
///
/// `#[non_exhaustive]` — downstream 패턴매치가 exhaustive match를 요구하지 않도록
/// 하여, 향후 variant 추가(§10-3 follow-up)가 소비자 빌드를 깨뜨리지 않게 보호.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConfigCode {
    /// 설정 파일 파싱 실패 또는 스키마 불일치.
    Invalid,
    /// 필수 설정 필드 누락.
    Missing,
    /// 설정 값이 허용 범위 밖.
    OutOfRange,
    /// AWS Bedrock 의도적 미지원 (ADR-019 §5 재도입 체크리스트 참조).
    UnsupportedProviderBedrock,
    /// 세분화 미완료 — Phase 2 일괄 이관 시 기본값. §10-7 lint가 crate별 사용량 감시.
    Generic,
}

impl ConfigCode {
    /// Wire 포맷 코드 문자열. 한번 릴리스되면 불변 (Grafana/로그 contract).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Invalid => "config.invalid",
            Self::Missing => "config.missing",
            Self::OutOfRange => "config.out_of_range",
            Self::UnsupportedProviderBedrock => "provider.bedrock.unsupported",
            Self::Generic => "config.generic",
        }
    }
}

impl std::fmt::Display for ConfigCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_round_trip_unique() {
        // 모든 variant의 as_str이 서로 달라야 함 (코드 충돌 방지)
        let all = [
            ConfigCode::Invalid,
            ConfigCode::Missing,
            ConfigCode::OutOfRange,
            ConfigCode::UnsupportedProviderBedrock,
            ConfigCode::Generic,
        ];
        let codes: Vec<_> = all.iter().map(|c| c.as_str()).collect();
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(codes.len(), unique.len(), "duplicate codes: {codes:?}");
    }

    #[test]
    fn naming_convention() {
        // 모든 코드는 소문자 + dot + snake_case
        for c in [ConfigCode::Invalid, ConfigCode::Generic /* ... */] {
            let s = c.as_str();
            assert!(s.chars().all(|ch| ch.is_ascii_lowercase() || ch == '.' || ch == '_'));
            assert!(s.contains('.'));
        }
    }
}
```

**Key properties**:

- `as_str` is `const fn` → usable in `const` contexts, trivial compile-time evaluation.
- `Display` implementation → `format!("{}", code)` yields wire format.
- `Generic` per-domain fallback → Phase 2 bulk migration seed. Remains available long-term as a fallback; drift controlled via §10-7 lint follow-up. Not required to be eliminated by Phase 4.
- Per-enum **test module** enforces (a) unique `as_str` output (b) naming convention.

**Implementation note**: the hand-written form above is pedagogical. Phase-1 actually defines each code enum via the `define_code_enum!` macro specified in §7.5 — that macro auto-generates the enum body, the `as_str` match, the `Display` impl, the `all()` helper, and the `#[non_exhaustive]` attribute from a single variant list. This eliminates drift between match, array, and fixture. The per-enum test module (the `tests` block above) stays hand-written or lives in a sibling `tests.rs`.

### 4.4 Unified accessor `impl CoreError::code()`

During Phases 1–3, V1 (deprecated tuple) and V2 (new struct) variants coexist in the same enum. Per §5.2, V1 keeps its original name (`Config(String)`), V2 uses the suffixed name (`ConfigV2 { code, message }`). The `code()` accessor must cover **both** for exhaustiveness and to avoid runtime panics on any un-retrofitted V1 callsite.

**V1 fallback code policy**: each V1 variant returns a code appropriate to its narrow semantics:
- **Default**: the domain's `Generic` code (e.g., `Config(_) → ConfigCode::Generic`).
- **Narrow-specific override**: if the V1 variant's name uniquely maps to a specific code in the domain enum (e.g., `RequestTimeout { .. }` → `NetworkCode::Timeout`, `InvalidArguments(_)` → `ValidationCode::InvalidArguments`), use that specific code. Rationale: V1 variants like `RequestTimeout` have narrow semantics already; returning `Generic` would lose information during the coexistence window unnecessarily.
- **Sole-variant domains**: for domains without a `Generic` variant (`NotFoundCode`, `UiCode`, `IntegrityCode`, `SandboxCode`), use the sole/most-matching specific variant.

V1 arms are deleted in Phase 4 alongside the V1 variant itself, so this V1 code policy is transitional — post-Phase-4 only the V2 `code` field value is live.

```rust
impl CoreError {
    /// Wire-format 에러 코드. UI, 로그, 텔레메트리 진입점.
    ///
    /// V1/V2 coexistence rule (Phases 1–3): every V1 deprecated variant returns
    /// its domain's `Generic` code as a transitional default. V1 arms are deleted
    /// in Phase 4 alongside the V1 variant itself.
    pub fn code(&self) -> &'static str {
        match self {
            // --- V2 struct variants (new shape, pre-Phase-4 named `*V2`) ---
            Self::ConfigV2 { code, .. } => code.as_str(),
            Self::NetworkV2 { code, .. } => code.as_str(),
            Self::RequestTimeoutV2 { code, .. } => code.as_str(),
            Self::RateLimitV2 { code, .. } => code.as_str(),
            Self::InternalV2 { code, .. } => code.as_str(),
            Self::AuthV2 { code, .. } => code.as_str(),
            Self::ValidationV2 { code, .. } => code.as_str(),
            Self::NotFoundV2 { code, .. } => code.as_str(),
            Self::ConsentExpiredV2 { code } => code.as_str(),
            Self::ConsentRequiredV2 { code, .. } => code.as_str(),
            // ... all V2 variants

            // --- V1 deprecated variants (Phases 1–3 only; removed in Phase 4) ---
            #[allow(deprecated)]
            Self::Config(_) => ConfigCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::Network(_) => NetworkCode::Generic.as_str(),
            #[allow(deprecated)]
            Self::Internal(_) => InternalCode::Generic.as_str(),
            // ... all V1 tuple/struct variants during coexistence

            // --- `#[from]`-wrapped external variants (unchanged across phases) ---
            // `code()` is derived here rather than stored on the variant.
            Self::Serialization(_) => InternalCode::Serialization.as_str(),
            Self::Io(_) => InternalCode::Io.as_str(),
        }
    }
}
```

**Phase 4 transition**: the V1 block is deleted, then `sed`-style (or rust-analyzer-rename) `s/V2 { /{ /` collapses V2 arms to canonical names (`Self::ConfigV2 { code, .. }` → `Self::Config { code, .. }`). §11 risk register tracks the `sed` clobbering hazard; implementation uses rust-analyzer rename to avoid substring collisions.

Equivalent `impl GuiInteractionError::code()` for the GUI error type follows the identical V1/V2 coexistence pattern.

**Test** (one per error enum):

```rust
#[test]
fn every_variant_has_code() {
    // 모든 variant 구성 → code() 호출 → panic 없음 + non-empty 문자열
    // (컴파일러 exhaustiveness로 이미 보장되지만 방어적 체크)
    for err in sample_variants() {
        let c = err.code();
        assert!(!c.is_empty());
        assert!(c.contains('.'));
    }
}
```

### 4.5 Naming convention (ADR-019 §2)

- Format: `{domain}.{category}[.{qualifier}[.{sub_qualifier}]]`
- `{domain}`: variant 그룹의 기능 영역 (`config`, `network`, `auth`, `provider`, `sandbox`, `oauth`, `gui`, ...).
- `{category}`: 그 안의 세분 (`invalid`, `timeout`, `missing`, `unsupported`, `expired`, ...).
- `{qualifier}`: 추가 세분 필요 시 (`provider.bedrock.unsupported`의 `bedrock`).
- 모두 **소문자 snake_case + dot 구분자**.
- **이미 릴리스된 code 문자열은 변경 불가** — wire contract. 레거시 이름 변경 필요 시 새 코드 추가 + 기존 코드 deprecated.

### 4.6 `#[from]`-wrapped external error types

Existing `#[from]` variants for `serde_json::Error` / `std::io::Error` keep their wrapper shape (tuple with the wrapped error). Their `code()` is derived in the `impl CoreError::code()` match arm (returning `InternalCode::Serialization.as_str()` / `InternalCode::Io.as_str()`) rather than stored as a field — the wrapped external error carries no code of its own.

### 4.7 GuiInteractionError identical pattern

```rust
// crates/oneshim-core/src/error_codes/gui.rs
pub enum GuiCode {
    Unauthorized,
    NotFound,
    BadRequest,
    Forbidden,
    FocusDrift,
    TicketInvalid,
    Unavailable,
    InternalError,
    Generic,
}

impl GuiCode { pub const fn as_str(self) -> &'static str { /* "gui.*" prefix */ } }
```

GuiInteractionError variants migrated identically: 8 variants → struct variants with `code: GuiCode`.

---

## 5. Migration Strategy (Soft V1→V2→V1 Rename)

### 5.1 Phase structure

| Phase | Scope | PR count | Build safety |
|-------|-------|----------|--------------|
| **Phase 1** | Introduce `error_codes` module + V2 variants alongside V1 variants in `CoreError` / `GuiInteractionError`. Mark V1 as `#[deprecated(...)]`. | 1 PR | V1 and V2 both exist; callsites unchanged; builds. |
| **Phase 2** | Per-crate retrofit of all V1 construction sites and match patterns to V2. Starts with leaf crates (no inter-crate dependency changes), works up to `src-tauri`. | 13 PRs — 12 retrofit + 1 verification-only (oneshim-sandbox-worker), see §5.3 | Each PR internally consistent; `#[deprecated]` warnings visible but non-fatal. |
| **Phase 3** | C5 specific: catalog delete, 7 match arms to `ConfigCode::UnsupportedProviderBedrock`, OCR no-auth security fix, ADR-019 authoring. | 1 PR | Builds clean. Uses infra from Phase 1. |
| **Phase 4** | Delete V1 variants. Rename V2 variants to clean names (e.g., `ConfigV2 { ... }` → `Config { ... }`). CI enforces zero `#[deprecated]` warnings from this point. | 1 PR (mechanical rename) | One rustc pass after rename verifies completeness. |

**Total**: **16 PRs** (1 Phase-1 + 13 Phase-2 + 1 Phase-3 + 1 Phase-4). Expected 2–3 weeks realistic timeline with per-PR review loop (§7).

### 5.2 V2 variant naming during migration

During Phases 1–3, V2 variants live under **alias names** to avoid colliding with V1:

```rust
pub enum CoreError {
    // V1 (to be deprecated and deleted)
    #[deprecated(since = "next", note = "use ConfigV2 — see ADR-019")]
    Config(String),

    // V2 (will be renamed to Config in Phase 4)
    ConfigV2 { code: ConfigCode, message: String },

    // ... same V1/V2 split per variant
}
```

**Phase 4 rename**: V1 deletion followed by rust-analyzer "Rename Symbol" (LSP-aware, substring-safe) applied to each `ConfigV2` → `Config` etc. Avoid `sed` for the rename — substrings like `ConfigV2Something` (if any exist in comments) would be corrupted; LSP rename is scope-aware. rustc exhaustiveness checking confirms completeness post-rename.

**CI deprecation gating across phases**:

| Phase | Behavior | CI flag |
|-------|----------|---------|
| Phase 1 | V1 declared with `#[deprecated]` | `-W deprecated` (warn only — pre-existing V1 usages are expected) |
| Phase 2 | Per-crate retrofit PRs | `-W deprecated` still warn-only — warnings are the migration signal |
| Phase 3 | C5 Bedrock skip | `-W deprecated` warn-only |
| Phase 4 | V1 deletion + V2 rename | flip to `-D deprecated` — any residual V1 usage fails CI |

The earlier edit ("`-D deprecated` blocks V1 usage pre-Phase-4") was incorrect and is corrected here.

### 5.3 Per-crate retrofit order (Phase 2)

Ordered by dependency depth (leaves first) so inter-crate code stabilizes progressively:

1. `oneshim-api-contracts` (no internal deps beyond oneshim-core)
2. `oneshim-embedding`
3. `oneshim-audio`
4. `oneshim-monitor`
5. `oneshim-storage`
6. `oneshim-vision`
7. `oneshim-analysis`
8. `oneshim-network`
9. `oneshim-suggestion`
10. `oneshim-automation`
11. `oneshim-web`
12. `oneshim-sandbox-worker` — *verification-only PR*: crate has zero current `CoreError::*` construction sites (verified via grep on 2026-04-19) but depends on `oneshim-core`. PR confirms absence of retrofit needs, adds the crate to the Phase-4 CI deprecation gate, and includes a defensive test asserting no V1 usage.
13. `src-tauri`

**Not in list**: `oneshim-lint` — workspace member without an `oneshim-core` dependency (its `Cargo.toml` only references `serde_json`). Therefore no `CoreError` usage is possible and no PR slot is needed. Phase 4 CI gate still applies at workspace level, but this crate contributes zero usages.

**Parallelization semantics** (explicit):

- The ordering above is **suggested, not required by build dependencies**. Every Phase-2 PR compiles independently against the Phase-1 baseline (V1 variants `#[deprecated]` but present; V2 variants added). Retrofitting one crate's V1 usages to V2 has no runtime or build coupling to any other Phase-2 PR.
- **Why suggest an order at all?** Reviewer cognitive locality. Starting with `oneshim-api-contracts` (leaf) lets the reviewer see the full retrofit pattern on a small crate before scaling to `oneshim-network` / `src-tauri` (largest).
- **PR-2 landing before PR-1 is acceptable** — there is no rustc-enforced dependency. Loop-3 gating in §8.3 is per-PR, not per-sequence.
- **Practical recommendation**: open PRs 1–6 in staggered batches of 2–3 rather than 13-at-once; a mass-parallel approach fragments reviewer attention and creates merge-conflict amplification on shared files (e.g. `error.rs` is untouched by Phase-2 PRs, so shared-file conflicts should be rare in practice).

**Total Phase 2**: 13 PRs (12 retrofit + 1 verification-only). This adjusts §5.1's earlier "12 PRs" count to **13 PRs**, making the total program count **15 → 16 PRs**.

### 5.4 Phase 2 per-crate PR checklist

Each Phase 2 crate PR:

- [ ] Retrofit every `CoreError::VariantV1(...)` construction → `CoreError::VariantV2 { code, message }`.
- [ ] Retrofit every `match` / `if let` on V1 → V2.
- [ ] If unclear which specific code applies, use `XxxCode::Generic` (transient; flagged in §9 follow-ups for refinement).
- [ ] Crate tests all pass.
- [ ] No new `#[deprecated]` warnings in that crate for variants owned by this crate's code.
- [ ] Cross-consumer audit performed (see §7.3) — grep for sibling consumers of each changed variant to ensure no missed callsite.

### 5.5 Why soft not hard

- ~1,150 callsites in one hard-break PR is unreviewable even at the corrected count (memory: `feedback_multi_pass_review`). Spread across 185 files crossing every major crate boundary, a single PR would exceed any practical reviewer attention budget.
- Soft allows per-crate local reasoning.
- `#[deprecated]` surfaces unchanged callsites as compile warnings during Phase 2 → any forgotten site is visible.
- `main` never breaks; CI always green; rollback per-PR.

---

## 6. C5 AWS Bedrock Skip (Phase 3 deliverable)

### 6.1 Catalog delete (`specs/providers/provider-surface-catalog.json`)

- **Remove** line 89–104: `vendors[]` object for `bedrock`.
- **Remove** line 2264–2398: `surfaces[]` object for `provider_surface.bedrock.direct_api` (~135 lines).

No other catalog references to Bedrock survive. Verified by grep on 2026-04-19.

### 6.2 `AiProviderType::Bedrock` / `AwsSignatureV4` / `BedrockConverse` enum variant retention

Per Q2 decision (churn minimization): **variants retained** across `oneshim-core::AiProviderType`, `oneshim-api-contracts::ProviderAuthScheme`, `oneshim-api-contracts::ProviderRequestShape`. Runtime-unreachable after catalog delete but remain as compile-time symbols. Reason logged in ADR-019.

### 6.3 7 match arm retrofit

| File | Line (verified 2026-04-19) | Before | After |
|------|-----------------------------|--------|-------|
| `oneshim-network/src/ai_ocr_client/mod.rs:54-58` | OCR auth | Silent no-auth fallthrough (**security bug**) | `return Err(CoreError::ConfigV2 { code: ConfigCode::UnsupportedProviderBedrock, message: "AWS Bedrock is intentionally unsupported in this build".into() })` |
| `ai_ocr_client/mod.rs:373-378` | OCR request shape | `Internal("Bedrock Converse ... not yet supported for OCR extraction")` | `ConfigV2 { code: UnsupportedProviderBedrock, message: ... }` |
| `ai_ocr_client/strategy.rs:32-35` | OCR strategy | `Internal(...)` | same as above |
| `ai_llm_client/request.rs:110-114` | LLM request build | `Internal(...)` | same |
| `ai_llm_client/request.rs:146-151` | LLM auth | `Internal(...)` | same |
| `ai_llm_client/request.rs:189-194` | LLM response parse | `Internal(...)` | same |
| `http_api_session/mod.rs:207-212` | Session auth | `Internal(...)` | same |

Note: line numbers reflect the file state on 2026-04-19 pre-Phase-2. After Phase 2 retrofits the same files to V2 variants, line numbers will shift — implementer must re-anchor by pattern (`ProviderAuthScheme::AwsSignatureV4` / `ProviderRequestShape::BedrockConverse`), not by line.

### 6.4 `apply_auth_headers` signature change

```rust
// Before: infallible
fn apply_auth_headers(
    auth_scheme: ProviderAuthScheme,
    builder: reqwest::RequestBuilder,
    api_key: &str,
) -> reqwest::RequestBuilder;

// After: fallible (to propagate Bedrock unsupported error)
fn apply_auth_headers(
    auth_scheme: ProviderAuthScheme,
    builder: reqwest::RequestBuilder,
    api_key: &str,
) -> Result<reqwest::RequestBuilder, CoreError>;
```

Callers at `ai_ocr_client/mod.rs:390` and `:393` already inside `async fn` returning `Result<_, CoreError>` → `?` propagation, one-line change each.

### 6.5 Cross-consumer audit (per `feedback_cross_consumer_audit`)

- [ ] All string references to `"bedrock"` / `"aws-bedrock"` / `"amazon-bedrock"` / `"aws_signature_v4"` / `"bedrock_converse"` across `specs/providers/` and `crates/` audited.
- [ ] `AiProviderType::Bedrock` arm kept in `match` sites (analysis_client, http_api_session, lib.rs, ai_provider_live_smoke) — returns `ConfigV2 { code: UnsupportedProviderBedrock }` or equivalent where flow reaches them.
- [ ] `crates/oneshim-web/src/services/ai_model_catalog_web_service.rs` — verify UI list omits Bedrock post catalog-delete (do not surface selector).

---

## 7. Testing Strategy

### 7.1 Unit tests per code enum file

Each `error_codes/{xyz}.rs` ships with:

- `as_str_round_trip_unique` — all variants produce distinct strings.
- `naming_convention` — lowercase + dot + snake_case format.

Total new unit tests in code enums: **19 files × 2 tests = 38 tests**.

### 7.2 Error accessor tests (in `error.rs`)

Two outer error types (`CoreError`, `GuiInteractionError`) each get:

- `every_variant_has_code` — constructs one sample per variant via a helper fn, calls `.code()`, asserts non-empty + contains `.` separator. rustc exhaustiveness guarantees no variant is forgotten; the test defends against future `#[allow(unreachable_patterns)]` regressions.
- `code_matches_registry` — for each variant, asserts `code()` equals the specific expected constant (e.g., `CoreError::ConfigV2 { code: ConfigCode::Invalid, .. }` → `"config.invalid"`).

Total accessor tests: **2 outer-error-types × 2 tests = 4 tests**, each parameterized via per-variant fixtures (one fixture per variant). Fixture count: ~40 per-variant fixtures across 32 CoreError variants + 8 GuiInteractionError variants.

### 7.3 Cross-consumer audit tests

For the C5 Bedrock path (Phase 3):

- 7 unit tests — one per match arm — verifying the `CoreError::ConfigV2` variant and `UnsupportedProviderBedrock` code are returned. Assert both the variant via pattern match and the `code()` output string.
- 1 catalog test — asserts `specs/providers/provider-surface-catalog.json` parses successfully **and** contains no vendor with `vendor_id == "bedrock"` (regression guard preventing accidental re-add).
- 1 security regression test — OCR `apply_auth_headers` with `AwsSignatureV4` returns `Err(...)` and does not construct an unauthenticated request (guards the security fix).

### 7.4 Integration / workspace tests

- Existing `src-tauri/tests/` integration tests must pass unchanged (no code() consumer yet).
- `cargo test --workspace` green after each phase PR lands.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
  - Exception: Phase 2 PRs may emit `#[deprecated]` warnings on V1 usage — those are expected during migration. Gate lifts in Phase 4.

### 7.5 Telemetry contract snapshot (required, Phase 1 deliverable)

**Approach**: hand-rolled (the workspace does not currently depend on `insta` — adding a new dev-dep for this single test is scope creep).

**Test file**: `crates/oneshim-core/tests/wire_contract_snapshot.rs`. The test:

1. Builds a sorted `Vec<&'static str>` of every `{Xyz}Code::*.as_str()` output via `oneshim_core::error_codes::all_codes()`.
2. Reads the expected snapshot from `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` (a static fixture file committed to the repo).
3. Asserts equality. On mismatch, the test failure message instructs the developer to update the fixture with the diff.

**`all_codes()` function — mechanics under `#[non_exhaustive]`**:

The collection function lives **inside `oneshim-core`'s `error_codes` module** (specifically `crates/oneshim-core/src/error_codes/mod.rs`). Because the integration test at `tests/wire_contract_snapshot.rs` compiles as an **external crate**, `all_codes()` must be declared `pub` (visibility constraint — `pub(crate)` would make the function invisible to `tests/*.rs`). To discourage external-crate use while permitting the test:

```rust
// crates/oneshim-core/src/error_codes/mod.rs
#[doc(hidden)]  // excluded from rustdoc; signal "internal test helper"
pub fn all_codes() -> Vec<&'static str> {
    let mut codes = Vec::new();
    for c in ConfigCode::all() { codes.push(c.as_str()); }
    for c in NetworkCode::all() { codes.push(c.as_str()); }
    // ... one `{XxxCode}::all()` call per code enum
    codes.sort();
    codes
}
```

**Single-source-of-truth pattern for each enum**: define one `define_code_enum!` macro (Phase-1 implementation detail; variant-list-readable at review time — declarative macros with variant expansion are not rustfmt-formatted inside the invocation body, so the variant list reads as a flat table) that takes the variant list and generates in one shot:

```rust
// crates/oneshim-core/src/error_codes/macros.rs  (internal)
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
            pub const fn as_str(self) -> &'static str {
                match self { $( Self::$variant => $wire, )+ }
            }

            /// Compile-time exhaustive enumeration. Updating this array
            /// is enforced by the exhaustive match in `as_str` — adding
            /// a variant without updating it fails `cargo build`.
            #[allow(dead_code)]  // only called from all_codes() + tests
            pub(super) const fn all() -> &'static [Self] {
                &[ $( Self::$variant, )+ ]
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
```

Usage:

```rust
// crates/oneshim-core/src/error_codes/config.rs
define_code_enum! {
    /// Config 카테고리 에러 코드 — ADR-019 §2 컨벤션.
    pub enum ConfigCode {
        /// 설정 파일 파싱 실패 또는 스키마 불일치.
        Invalid => "config.invalid",
        /// 필수 설정 필드 누락.
        Missing => "config.missing",
        /// 설정 값이 허용 범위 밖.
        OutOfRange => "config.out_of_range",
        /// AWS Bedrock 의도적 미지원 (ADR-019 §5 재도입 체크리스트 참조).
        UnsupportedProviderBedrock => "provider.bedrock.unsupported",
        /// 세분화 미완료 — Phase 2 일괄 이관 시 기본값.
        Generic => "config.generic",
    }
}
```

**Why the macro approach is required**:

- **Single source of truth**: one variant list produces (a) the enum body (b) `as_str` match (c) `all()` array. A new variant added to the list flows to all three without drift.
- **Compile-time exhaustiveness**: `as_str`'s exhaustive `match` is the enforcement point — adding a variant to the enum without adding to the list becomes a syntax error (match arm missing); impossible to forget. `all()` derives from the same list, so drift between match and array is architecturally impossible.
- **`#[non_exhaustive]` vs internal `all()`**: `#[non_exhaustive]` is applied by the macro, so downstream consumers cannot match exhaustively. `all()` is `pub(super)` — only `mod.rs`'s `all_codes()` aggregator calls it. External consumers must go through `all_codes()` (which is `#[doc(hidden)] pub`) or use `err.code()`.

**Why required, not optional**: §4.5 declares released code strings wire-immutable. Without this macro pattern + snapshot test, a drive-by rename of a variant's wire string or a new-variant addition could ship silently. The macro makes the "wire contract" and "variant list" the same text.

**When to update**: additions (new codes) — add a line to the macro invocation's variant list AND update the fixture as part of the PR. The compile-time enforcement requires both. Deletions/renames — forbidden per §4.5 (released strings immutable); if truly required, a separate RFC PR justifies the wire break.

### 7.6 No live-AWS test added

Given AWS Bedrock is "intentionally unsupported," no `--ignored` live smoke test for Bedrock is added. `crates/oneshim-network/tests/ai_provider_live_smoke.rs` currently has a `Bedrock` arm in its provider-type match. Phase 3 updates that arm to:

- **Not** attempt a live network request.
- Instead construct a Bedrock-configured client path up to (but not including) the first `?` propagation point and assert the returned `Err(CoreError::ConfigV2 { code: ConfigCode::UnsupportedProviderBedrock, .. })` pattern.

This guarantees the "intentional unsupport" contract is regression-tested without external network dependency.

---

## 8. Review Loop (Per User Directive §"3-loop quality gate")

Three review loops per `feedback_3loop_quality_gate` memory. Each loop iterates until **zero Critical + zero Important** issues:

### 8.1 Loop 1 — Spec review (this document)

- Tool: `superpowers:code-reviewer` subagent + Codex rescue (`codex:rescue`) for second opinion on design ambiguity.
- Scope: this spec file.
- Exit: zero Critical / zero Important findings.

### 8.2 Loop 2 — Plan review

- Input: implementation plan (to be written after spec approval via `superpowers:writing-plans` skill).
- Tool: `superpowers:code-reviewer` + optional Codex second-opinion.
- Scope: per-phase task list, PR sequencing, acceptance criteria.
- Exit: zero Critical / zero Important.

### 8.3 Loop 3 — Implementation review (per-PR)

- Applied at each Phase PR (Phase 1 / Phase 2 ×13 / Phase 3 / Phase 4 = **16 PRs**).
- Per-PR subagent review (proven in Wave 1 per memory `project_next_tasks.md`):
  1. Implementer agent (Opus) delivers the PR.
  2. Spec-compliance reviewer agent (Opus) verifies alignment to this spec + ADR-019.
  3. Code-quality reviewer agent (`superpowers:code-reviewer`) validates Rust idioms, test coverage, guardrails (`architecture-guardrails` section of `client-rust/CLAUDE.md`).
- Exit: each PR merges only after all three passes produce zero Critical / zero Important findings.
- Final pre-merge holistic review per `feedback_holistic_pre_merge_review` after all 16 PRs: one integrated pass by Opus to catch cross-cutting drift.

### 8.4 Tooling

- `ralph-loop:ralph-loop` orchestrates loops 1–3.
- Individual reviewer agents: `superpowers:code-reviewer`, `codex:rescue` for heavy diagnostics.

---

## 9. Rollout & PR Sequencing Summary

```
Phase 1 ─── 1 PR:  error_codes module + V2 variants + deprecations
    │
    ├── Phase 2 ─── 13 PRs (12 retrofit + 1 verification-only, per §5.3)
    │       │
    │       └── [each PR: loop-3 review → merge]
    │
    ├── Phase 3 ─── 1 PR:  C5 Bedrock skip + ADR-019 + catalog delete
    │
    └── Phase 4 ─── 1 PR:  V1 delete + V2 rename (rust-analyzer LSP rename, not sed)

Total: 16 PRs, ~2-3 weeks
```

**Gating**:
- Phase 2 cannot start until Phase 1 lands.
- Phase 3 can start after Phase 2 retrofit of `oneshim-network` (its target crate) lands; it does not require all Phase 2 PRs.
- Phase 4 cannot start until all Phase 2 PRs + Phase 3 land. Before the Phase-4 PR opens, a freeze on in-flight PRs touching `CoreError` / `GuiInteractionError` is requested to avoid rename conflicts.

---

## 10. Open Questions / Follow-ups (Non-blocking)

These do not block spec approval; tracked for post-implementation work:

1. **Grafana dashboard label migration** — once `err.code()` is available in logs, update dashboards to group by `code` label instead of regex on `message`. Estimated 0.5 day, schedulable after Phase 4.
2. **i18n for `message` field** — frontend i18n system (ko/en) can consume `code` as translation key. Separate spec required. Post-Phase-4.
3. **`Internal` variant taxonomy refinement** — this spec adds `InternalCode::Generic` + a handful of specifics (Serialization, Io). Given Internal has ~416 callsites (top variant by frequency), further refinement is an evergreen follow-up driven by production telemetry signals.
4. **Sandbox variant consolidation** — `SandboxInit` + `SandboxExecution` + `SandboxUnsupported` + `ExecutionTimeout` overlap semantically. Consider unifying to single `Sandbox { code: SandboxCode, ... }` variant in a separate refactor. Out of scope here.
5. **Unify `ProcessNotAllowed` into `PolicyCode`** — single-use variant; could merge into `PolicyDenied` with `PolicyCode::ProcessNotAllowed`. Out of scope here.
6. **`impl std::error::Error::provide` for structured metadata** — Rust 2024 error `provide` API could expose `code: &'static str` to upstream `anyhow` consumers. Future enhancement.
7. **Update `oneshim-lint` check** — add a lint rule that warns on `CoreError::*::Generic` usage beyond N occurrences per crate to push refinement. Post-Phase-4.

---

## 11. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Phase 2 per-crate PR drift (different reviewers pick different codes for same pattern) | Medium | Medium | `ConfigCode::Generic` as fallback; per-PR review loop catches divergence; Phase 2 checklist (§5.4) requires cross-consumer audit. |
| Phase 4 rename PR conflicts with concurrent work on `CoreError` / `GuiInteractionError` | Medium | Low | PR freeze on error types before Phase 4 (§9 gating); rust-analyzer LSP rename (scope-aware) rather than `sed` to avoid substring clobbering. |
| `sed`-style rename corrupts substrings (e.g. a hypothetical `ConfigV2Something` literal in comments / docs / fixture files) | Low | Medium | §5.2 & §9: rust-analyzer LSP rename mandatory for Phase 4; `sed` is explicitly prohibited. |
| Telemetry dashboards silently break post-Phase-4 rename | Low | High | §10-1 follow-up tracked; pre-Phase-4 screenshot of dashboards archived; wire-format snapshot test (§7.5) guards code strings. |
| `Generic` fallback becomes permanent across many crates | Medium | Medium | §10-7 lint check on `{Xyz}Code::Generic` usage count per crate; quarterly audit. |
| Bedrock re-introduction requires more than spec anticipates | Low | Low | ADR-019 re-introduction checklist; enum variants retained so diff is surgical. |
| Live AWS test regression (for consumers NOT selecting Bedrock) | Low | Medium | §7.6 live smoke already covers non-Bedrock providers unchanged. |
| Future `#[from]` variant wraps a second error type that conflicts with existing derived codes (e.g., `tokio::io::Error` alongside `std::io::Error`) | Low | Low | `impl CoreError::code()` match arm is the single decision point — new `#[from]` variants require adding a new arm; design accommodates without changing existing arms. ADR-019 §"Adding new `#[from]` variants" documents the pattern: allocate an `InternalCode::*` variant, add `#[from]` + `impl CoreError::code()` arm in the same PR, update the wire-contract fixture. |

---

## 12. Acceptance Criteria

### 12.1 Spec acceptance (Loop 1 exit)

- [ ] §1–§11 review produces zero Critical / Important findings.
- [ ] User (richard.kim0828@gmail.com) approves spec.

### 12.2 Plan acceptance (Loop 2 exit)

- [ ] Writing-plans skill produces per-phase task list covering all §5 phases.
- [ ] Plan review produces zero Critical / Important.
- [ ] Acceptance criteria per PR defined in plan.

### 12.3 Implementation acceptance (Loop 3 exit)

- [ ] All 16 PRs merged.
- [ ] `cargo check/test/clippy/fmt --workspace` clean on main post Phase 4.
- [ ] `cargo clippy --workspace --all-targets -- -D deprecated` passes (enforced Phase 4 onward).
- [ ] `docs/architecture/ADR-019-error-code-infrastructure.md` + `.ko.md` companion committed.
- [ ] No V1 variant remains (grep for `#[deprecated]` on `CoreError` / `GuiInteractionError` yields zero results).
- [ ] `err.code()` returns non-empty, dot-containing string for every variant (`every_variant_has_code` test passes for both `CoreError` and `GuiInteractionError`).
- [ ] Wire-format snapshot test (`wire_contract_snapshot.rs`) passes against committed fixture.
- [ ] C5 Bedrock path: 7 match arms return `ConfigCode::UnsupportedProviderBedrock`; catalog has no Bedrock entry; OCR `apply_auth_headers` returns error on `AwsSignatureV4` (no silent no-auth).
- [ ] `oneshim-sandbox-worker` verification test confirms no V1 usage introduced during migration window.

---

## 13. Appendix — Existing variant → Code enum mapping

| CoreError variant | Code enum | Initial variant(s) |
|-------------------|-----------|---------------------|
| `Config` | `ConfigCode` | `Invalid`, `Missing`, `OutOfRange`, `UnsupportedProviderBedrock`, `Generic` |
| `Network` | `NetworkCode` | `Failed`, `Generic` |
| `RequestTimeout` | `NetworkCode` | `Timeout` |
| `RateLimit` | `NetworkCode` | `RateLimit` |
| `ServiceUnavailable` | `ServiceCode` | `Unavailable`, `Generic` |
| `Auth` | `AuthCode` | `Failed`, `Generic` |
| `OAuthError` | `OAuthCode` | `Failed`, `Generic` |
| `OAuthRefreshError` | `OAuthCode` | `RefreshFailed` |
| `Validation` | `ValidationCode` | `InvalidField`, `Generic` |
| `NotFound` | `NotFoundCode` | `ResourceMissing` |
| `ElementNotFound` | `UiCode` | `ElementMissing` |
| `BinaryHashMismatch` | `IntegrityCode` | `HashMismatch` |
| `Io` (`#[from]`) | `InternalCode` | (derived via `impl CoreError::code()` match arm — no stored field) |
| `Serialization` (`#[from]`) | `InternalCode` | (derived via `impl CoreError::code()` match arm — no stored field) |
| `Internal` | `InternalCode` | `Generic`, `Io`, `Serialization` (the latter two used only by the derived `#[from]` arms above) |
| `PolicyDenied` | `PolicyCode` | `Denied`, `Generic` |
| `ProcessNotAllowed` | `PolicyCode` | `ProcessDenied` |
| `InvalidArguments` | `ValidationCode` | `InvalidArguments` |
| `ConsentRequired` | `ConsentCode` | `Required`, `Generic` |
| `ConsentExpired` | `ConsentCode` | `Expired` |
| `SandboxInit` | `SandboxCode` | `InitFailed` |
| `SandboxExecution` | `SandboxCode` | `ExecutionFailed` |
| `SandboxUnsupported` | `SandboxCode` | `UnsupportedPlatform` |
| `ExecutionTimeout` | `SandboxCode` | `Timeout` |
| `PrivacyDenied` | `PermissionCode` | `PrivacyDenied` |
| `PermissionDenied` | `PermissionCode` | `PermissionDenied`, `Generic` |
| `OcrError` | `ProviderCode` | `OcrFailed`, `Generic` |
| `AudioCapture` | `AudioCode` | `CaptureFailed`, `Generic` |
| `SpeechToText` | `AudioCode` | `SttFailed` |
| `Storage` | `StorageCode` | `Failed`, `Generic` |
| `Analysis` | `ProviderCode` | `AnalysisFailed` |
| `SecretStoreError` | `SecretCode` | `Failed`, `Generic` |

| GuiInteractionError variant | Code enum | Initial variant |
|-----------------------------|-----------|------------------|
| `Unauthorized` | `GuiCode` | `Unauthorized` |
| `NotFound` | `GuiCode` | `NotFound` |
| `BadRequest` | `GuiCode` | `BadRequest` |
| `Forbidden` | `GuiCode` | `Forbidden` |
| `FocusDrift` | `GuiCode` | `FocusDrift` |
| `TicketInvalid` | `GuiCode` | `TicketInvalid` |
| `Unavailable` | `GuiCode` | `Unavailable` |
| `Internal` | `GuiCode` | `InternalError`, `Generic` |

Total new code enums: **19**.
Total initial code variants across all enums (Phase-1 deliverable; computed from the appendix tables above): **57** (CoreError domains: Config 5 + Network 4 + Service 2 + Auth 2 + OAuth 3 + Validation 3 + NotFound 1 + Ui 1 + Integrity 1 + Internal 3 + Policy 3 + Consent 3 + Sandbox 4 + Permission 3 + Provider 3 + Audio 3 + Storage 2 + Secret 2 = 48; GuiCode variants from §4.7: `Unauthorized`, `NotFound`, `BadRequest`, `Forbidden`, `FocusDrift`, `TicketInvalid`, `Unavailable`, `InternalError`, `Generic` = 9). Post-Phase-1 refinements (per §10) are additive.

Refinement post-Phase-4 can add more codes without touching this mapping (additive). `#[non_exhaustive]` on every code enum (§4.3) guarantees additive changes do not break downstream consumers; the §7.5 compile-time enumeration guard prevents new variants from silently missing the wire-contract fixture.

---

## 14. Changelog

- **2026-04-19 (initial draft)** — Authored during brainstorming session.
- **2026-04-19 (Loop 1 revision 1)** — Applied review findings:
  - §1.2/§1.3 corrected callsite counts (2,104 → 1,052 CoreError; 194 → 97 GuiInteractionError) after verifying `grep -o` double-counting in initial estimate.
  - §4.2 softened ADR-003 framing (style guidance, not triggering rule).
  - §4.3 added `#[non_exhaustive]` to code enum template.
  - §4.4 made V1/V2 coexistence explicit in `impl CoreError::code()` match body.
  - §5.2 documented phased CI deprecation gating; corrected earlier `-D deprecated pre-Phase-4` claim.
  - §5.2 required rust-analyzer LSP rename (not `sed`) for Phase 4.
  - §5.3 added `oneshim-sandbox-worker` as verification-only PR (13 Phase-2 PRs instead of 12); clarified `oneshim-lint` as workspace member without `oneshim-core` dep.
  - §5.1/§5.3/§5.5/§8.3/§9/§12.3 updated PR count 15 → 16.
  - §6.3 line ranges refined to exact file state as of 2026-04-19; added instruction to re-anchor by pattern after Phase 2.
  - §7.1/§7.2 tightened test math (38 enum tests + 4 accessor tests, not "~40+40").
  - §7.5 committed to hand-rolled snapshot at `wire_contract_snapshot.rs`.
  - §10-3 corrected Internal callsite count 832 → 416.
  - §11 added two risks (sed corruption, multi-`#[from]` collision); rewrote Phase 4 rename mitigation.
  - §12.3 added new acceptance checks (deprecated CI gate, snapshot test, sandbox-worker verification).
  - §13 corrected `InternalCode` and external-error mapping (Io/Serialization derived, not stored).
- **2026-04-19 (Loop 1 revision 2)** — Applied second-pass review findings:
  - §4.1 added explicit decision: `CoreError` / `GuiInteractionError` are NOT `#[non_exhaustive]` (internal-only public API, exhaustive matching aids Phase 2 review).
  - §5.3 made parallelization semantics explicit: ordering is suggested (reviewer cognitive locality), not dependency-required; each Phase-2 PR is independent from every other.
  - §7.5 added compile-time exhaustiveness guard pattern for `all_codes()` / `enumerate_xyz_code()` — required to prevent `#[non_exhaustive]` + variant-addition from silently missing the wire-contract fixture.
  - §11 committed the multi-`#[from]` collision risk to an ADR-019 documented pattern (no longer "TODO").
  - §13 replaced approximate "~58" count with enumeration derived from appendix tables.
- **2026-04-19 (Loop 1 revision 3)** — Applied third-pass review findings:
  - §7.5 corrected `pub(crate) fn all_codes()` visibility bug — integration tests under `tests/` compile as external crate and cannot see `pub(crate)`. Changed to `#[doc(hidden)] pub fn all_codes()`.
  - §7.5 replaced the dual-source-of-truth pattern (separate `_compile_time_exhaustiveness` match + array) with a single-source `define_code_enum!` macro that generates enum body, `as_str` match, and `all()` array from one variant list. Architectural guarantee that drift between enum and enumeration is impossible.
  - §13 corrected final tally 53 → 57 (GuiCode has 9 variants per §4.7, not 5 as previously tallied).
  - §4.1 harmonized "13 crates" → "14-member workspace" to match §1.2.
