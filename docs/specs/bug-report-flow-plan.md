# Bug Report Flow — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a privacy-preserving bug report flow with Bug ID correlation, two-channel reporting (GitHub Issue + Email), PII-filtered diagnostic bundle export, and dual-format clipboard.

**Architecture:** Extends existing support diagnostics in `oneshim-web` with a `PiiSanitizer` port trait in `oneshim-core`, implemented in `oneshim-vision`, injected via DI. Bug ID generation stays in `oneshim-web` (no crypto deps in core). New `BugReportWizard` React component replaces the current inline GitHub issue opener.

**Tech Stack:** Rust (oneshim-core, oneshim-vision, oneshim-web, src-tauri), React 18, TypeScript, Axum 0.8, sha2, tauri-plugin-dialog

**Spec:** `docs/specs/bug-report-flow-spec.md`

---

## File Structure

### New Files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/models/bug_report.rs` | `BugId` newtype with validation |
| `crates/oneshim-core/src/ports/pii_sanitizer.rs` | `PiiSanitizer` trait |
| `crates/oneshim-api-contracts/src/bug_report.rs` | `BugReportBundleDto`, `SystemInfoDto`, `ConnectionStatusDto` |
| `crates/oneshim-web/src/services/bug_report_service.rs` | Bug ID generation, bundle assembly, PII sanitization |
| `crates/oneshim-web/src/handlers/bug_report.rs` | REST endpoints + tests |
| `src-tauri/src/commands/bug_report.rs` | Tauri IPC for file export |
| `crates/oneshim-web/frontend/src/components/BugReportWizard.tsx` | 3-step wizard UI |
| `crates/oneshim-web/frontend/src/api/bug-report.ts` | API client + formatters |

### Modified Files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/models/mod.rs` | Add `pub mod bug_report;` |
| `crates/oneshim-core/src/ports/mod.rs` | Add `pub mod pii_sanitizer;` |
| `crates/oneshim-vision/src/privacy.rs` | Add `VisionPiiSanitizer` struct + `PiiSanitizer` impl |
| `crates/oneshim-api-contracts/src/lib.rs` | Add `pub mod bug_report;` |
| `crates/oneshim-api-contracts/src/support.rs` | Add `Deserialize` to existing DTOs |
| `crates/oneshim-web/Cargo.toml` | Add `sha2`, `hex`, `rand` deps |
| `crates/oneshim-web/src/lib.rs` | Add `pii_sanitizer` + `latest_bug_report` to `AppState` |
| `crates/oneshim-web/src/routes.rs` | Add bug report routes |
| `crates/oneshim-web/src/handlers/mod.rs` | Add `pub mod bug_report;` |
| `crates/oneshim-web/src/services/mod.rs` | Add `pub mod bug_report_service;` |
| `crates/oneshim-web/src/services/web_contexts/mod.rs` | Add `BugReportContext` |
| `Cargo.toml` (workspace) | Add `tauri-plugin-dialog` |
| `src-tauri/Cargo.toml` | Add `tauri-plugin-dialog` dep |
| `src-tauri/src/commands/mod.rs` | Add `pub(crate) mod bug_report;` |
| `src-tauri/src/main.rs` | Wire `VisionPiiSanitizer` → `WebServer`, register IPC commands |
| `crates/oneshim-web/frontend/src/api/contracts.ts` | Add bug report TS types |
| `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx` | Replace `handleReportBug` with wizard |
| `crates/oneshim-web/frontend/src/i18n/locales/en.json` | Add ~20 `bugReport.*` keys |
| `crates/oneshim-web/frontend/src/i18n/locales/ko.json` | Add ~20 `bugReport.*` keys |

---

## Task 1: BugId Model + PiiSanitizer Port (oneshim-core)

**Files:**
- Create: `crates/oneshim-core/src/models/bug_report.rs`
- Create: `crates/oneshim-core/src/ports/pii_sanitizer.rs`
- Modify: `crates/oneshim-core/src/models/mod.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] **Step 1: Write tests for BugId**

```rust
// crates/oneshim-core/src/models/bug_report.rs

use serde::{Deserialize, Serialize};

/// Bug report identifier for support correlation.
/// Format: `BUG-{12_hex_chars}` (16 chars total).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugId(String);

impl BugId {
    pub fn new(id: String) -> Result<Self, &'static str> {
        if id.starts_with("BUG-") && id.len() == 16 {
            Ok(Self(id))
        } else {
            Err("Bug ID must match format BUG-{12_hex_chars}")
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BugId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_bug_id() {
        let id = BugId::new("BUG-a1b2c3d4e5f6".to_string());
        assert!(id.is_ok());
        assert_eq!(id.unwrap().as_str(), "BUG-a1b2c3d4e5f6");
    }

    #[test]
    fn rejects_short_id() {
        assert!(BugId::new("BUG-abc".to_string()).is_err());
    }

    #[test]
    fn rejects_wrong_prefix() {
        assert!(BugId::new("ERR-a1b2c3d4e5f6".to_string()).is_err());
    }

    #[test]
    fn rejects_too_long() {
        assert!(BugId::new("BUG-a1b2c3d4e5f6aa".to_string()).is_err());
    }

    #[test]
    fn serializes_as_string() {
        let id = BugId::new("BUG-a1b2c3d4e5f6".to_string()).unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"BUG-a1b2c3d4e5f6\"");
    }

    #[test]
    fn deserializes_from_string() {
        let id: BugId = serde_json::from_str("\"BUG-a1b2c3d4e5f6\"").unwrap();
        assert_eq!(id.as_str(), "BUG-a1b2c3d4e5f6");
    }

    #[test]
    fn display_impl() {
        let id = BugId::new("BUG-a1b2c3d4e5f6".to_string()).unwrap();
        assert_eq!(format!("{id}"), "BUG-a1b2c3d4e5f6");
    }
}
```

- [ ] **Step 2: Run BugId tests**

Run: `cargo test -p oneshim-core bug_report`
Expected: all 7 tests pass

- [ ] **Step 3: Create PiiSanitizer port trait**

```rust
// crates/oneshim-core/src/ports/pii_sanitizer.rs

//! PII (Personally Identifiable Information) sanitization port.
//! Implemented by `VisionPiiSanitizer` in `oneshim-vision`.

use crate::config::PiiFilterLevel;

/// Sanitizes text by replacing PII patterns with redaction markers.
pub trait PiiSanitizer: Send + Sync {
    /// Replace PII in `text` according to the given filter level.
    /// Returns sanitized text with markers like `[EMAIL]`, `[PHONE]`, `[USER]`.
    fn sanitize_text(&self, text: &str, level: PiiFilterLevel) -> String;
}
```

- [ ] **Step 4: Register modules**

Add to `crates/oneshim-core/src/models/mod.rs`:
```rust
pub mod bug_report;
```

Add to `crates/oneshim-core/src/ports/mod.rs`:
```rust
pub mod pii_sanitizer;
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p oneshim-core`
Expected: compiles with 0 errors

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-core/src/models/bug_report.rs crates/oneshim-core/src/ports/pii_sanitizer.rs crates/oneshim-core/src/models/mod.rs crates/oneshim-core/src/ports/mod.rs
git commit -m "feat(core): add BugId model and PiiSanitizer port trait"
```

---

## Task 2: VisionPiiSanitizer Implementation (oneshim-vision)

**Files:**
- Modify: `crates/oneshim-vision/src/privacy.rs`

- [ ] **Step 1: Add VisionPiiSanitizer struct and impl**

At the end of `crates/oneshim-vision/src/privacy.rs` (before the `#[cfg(test)]` block), add:

```rust
/// Adapter implementing [`PiiSanitizer`] by delegating to this module's
/// `sanitize_title_with_level` function.
pub struct VisionPiiSanitizer;

impl oneshim_core::ports::pii_sanitizer::PiiSanitizer for VisionPiiSanitizer {
    fn sanitize_text(
        &self,
        text: &str,
        level: oneshim_core::config::PiiFilterLevel,
    ) -> String {
        sanitize_title_with_level(text, level)
    }
}
```

- [ ] **Step 2: Add test for VisionPiiSanitizer**

Inside the existing `#[cfg(test)] mod tests` block in `privacy.rs`, add:

```rust
    #[test]
    fn vision_pii_sanitizer_trait_delegates() {
        use oneshim_core::config::PiiFilterLevel;
        use oneshim_core::ports::pii_sanitizer::PiiSanitizer;

        let sanitizer = super::VisionPiiSanitizer;
        let result = sanitizer.sanitize_text("email: user@example.com path", PiiFilterLevel::Standard);
        assert!(result.contains("[EMAIL]"));
        assert!(!result.contains("user@example.com"));
    }
```

- [ ] **Step 3: Run test**

Run: `cargo test -p oneshim-vision vision_pii_sanitizer_trait_delegates`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-vision/src/privacy.rs
git commit -m "feat(vision): implement PiiSanitizer port on VisionPiiSanitizer"
```

---

## Task 3: Bug Report DTOs (oneshim-api-contracts)

**Files:**
- Create: `crates/oneshim-api-contracts/src/bug_report.rs`
- Modify: `crates/oneshim-api-contracts/src/lib.rs`
- Modify: `crates/oneshim-api-contracts/src/support.rs`

- [ ] **Step 1: Add Deserialize to existing support DTOs**

In `crates/oneshim-api-contracts/src/support.rs`, add `Deserialize` to the derive macros of:
- `DiagnosticsBundleDto`
- `DiagnosticsHealthDto`
- `RuntimeLogSnapshotDto`
- `AuditEntryDto`

For example, change `#[derive(Debug, Serialize)]` to `#[derive(Debug, Serialize, Deserialize)]` for each.

- [ ] **Step 2: Create bug_report.rs DTOs**

```rust
// crates/oneshim-api-contracts/src/bug_report.rs

use serde::{Deserialize, Serialize};

use crate::support::{DiagnosticsBundleDto, RuntimeLogSnapshotDto};

/// Complete bug report bundle for export/sharing.
#[derive(Debug, Serialize, Deserialize)]
pub struct BugReportBundleDto {
    pub bug_id: String,
    pub diagnostics: DiagnosticsBundleDto,
    pub system: SystemInfoDto,
    pub connection: ConnectionStatusDto,
    pub runtime_logs: Option<RuntimeLogSnapshotDto>,
    pub pii_filter_level: String,
}

/// System hardware and software information.
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfoDto {
    pub app_version: String,
    pub os_name: String,
    pub os_version: String,
    pub arch: String,
    pub runtime: String,
    pub cpu_count: usize,
    pub memory_total_mb: u64,
    pub memory_available_mb: u64,
    pub uptime_seconds: u64,
}

/// Server connection status snapshot.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionStatusDto {
    pub server_reachable: bool,
    pub last_sync_at: Option<String>,
    pub grpc_enabled: bool,
    pub websocket_connected: bool,
}

/// Request parameters for creating a bug report.
#[derive(Debug, Deserialize)]
pub struct CreateBugReportRequest {
    #[serde(default = "default_include_logs")]
    pub include_logs: bool,
    #[serde(default)]
    pub pii_level: Option<String>,
}

fn default_include_logs() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_info_serializes() {
        let info = SystemInfoDto {
            app_version: "0.4.16".to_string(),
            os_name: "macOS".to_string(),
            os_version: "15.4".to_string(),
            arch: "aarch64".to_string(),
            runtime: "tauri-desktop".to_string(),
            cpu_count: 10,
            memory_total_mb: 16384,
            memory_available_mb: 8192,
            uptime_seconds: 3600,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"cpu_count\":10"));
    }

    #[test]
    fn connection_status_serializes() {
        let status = ConnectionStatusDto {
            server_reachable: true,
            last_sync_at: Some("2026-04-03T10:00:00Z".to_string()),
            grpc_enabled: false,
            websocket_connected: false,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("server_reachable"));
    }

    #[test]
    fn create_request_defaults() {
        let req: CreateBugReportRequest =
            serde_json::from_str("{}").unwrap();
        assert!(req.include_logs);
        assert!(req.pii_level.is_none());
    }
}
```

- [ ] **Step 3: Register module**

Add to `crates/oneshim-api-contracts/src/lib.rs`:
```rust
pub mod bug_report;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-api-contracts bug_report`
Expected: all 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-api-contracts/src/bug_report.rs crates/oneshim-api-contracts/src/lib.rs crates/oneshim-api-contracts/src/support.rs
git commit -m "feat(contracts): add bug report DTOs and Deserialize to existing support types"
```

---

## Task 4: BugReportService (oneshim-web backend)

**Files:**
- Create: `crates/oneshim-web/src/services/bug_report_service.rs`
- Modify: `crates/oneshim-web/src/services/mod.rs`
- Modify: `crates/oneshim-web/Cargo.toml`

- [ ] **Step 1: Add dependencies to oneshim-web**

Add to `crates/oneshim-web/Cargo.toml` under `[dependencies]`:
```toml
sha2 = { workspace = true }
hex = { workspace = true }
rand = { workspace = true }
```

- [ ] **Step 2: Create bug_report_service.rs**

```rust
// crates/oneshim-web/src/services/bug_report_service.rs

use oneshim_api_contracts::bug_report::{
    BugReportBundleDto, ConnectionStatusDto, CreateBugReportRequest, SystemInfoDto,
};
use oneshim_api_contracts::support::RuntimeLogSnapshotDto;
use oneshim_core::config::PiiFilterLevel;
use oneshim_core::models::bug_report::BugId;
use oneshim_core::ports::pii_sanitizer::PiiSanitizer;
use std::sync::Arc;

use crate::services::support_service::SupportDiagnosticsQueryService;
use crate::services::web_contexts::BugReportContext;

pub struct BugReportService {
    ctx: BugReportContext,
}

impl BugReportService {
    pub fn new(ctx: BugReportContext) -> Self {
        Self { ctx }
    }

    pub async fn create_report(
        &self,
        include_logs: bool,
        pii_level: Option<String>,
    ) -> BugReportBundleDto {
        let diagnostics = SupportDiagnosticsQueryService::new(self.ctx.support.clone())
            .get_diagnostics()
            .await;

        let system = self.collect_system_info();
        let connection = self.collect_connection_status();

        let runtime_logs = if include_logs {
            self.ctx.runtime_logs.clone()
        } else {
            None
        };

        let level = parse_pii_level(pii_level.as_deref());
        let bug_id = generate_bug_id(&system.app_version, &system.os_name);

        let mut bundle = BugReportBundleDto {
            bug_id: bug_id.to_string(),
            diagnostics,
            system,
            connection,
            runtime_logs,
            pii_filter_level: format!("{level:?}"),
        };

        if let Some(ref sanitizer) = self.ctx.pii_sanitizer {
            sanitize_bundle(sanitizer.as_ref(), &mut bundle, level);
        }

        bundle
    }

    fn collect_system_info(&self) -> SystemInfoDto {
        SystemInfoDto {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os_name: std::env::consts::OS.to_string(),
            os_version: String::new(),
            arch: std::env::consts::ARCH.to_string(),
            runtime: if cfg!(feature = "tauri") {
                "tauri-desktop"
            } else {
                "web"
            }
            .to_string(),
            cpu_count: 0,
            memory_total_mb: 0,
            memory_available_mb: 0,
            uptime_seconds: 0,
        }
    }

    fn collect_connection_status(&self) -> ConnectionStatusDto {
        ConnectionStatusDto {
            server_reachable: false,
            last_sync_at: None,
            grpc_enabled: false,
            websocket_connected: false,
        }
    }
}

fn generate_bug_id(app_version: &str, os_info: &str) -> BugId {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(app_version.as_bytes());
    hasher.update(b"|");
    hasher.update(os_info.as_bytes());
    hasher.update(b"|");
    hasher.update(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_le_bytes(),
    );
    let random_bytes: [u8; 8] = rand::random();
    hasher.update(random_bytes);
    let hash = hasher.finalize();
    BugId::new(format!("BUG-{}", hex::encode(&hash[..6]))).expect("format is valid")
}

fn parse_pii_level(level: Option<&str>) -> PiiFilterLevel {
    match level {
        Some("strict") => PiiFilterLevel::Strict,
        _ => PiiFilterLevel::Standard,
    }
}

fn sanitize_bundle(
    sanitizer: &dyn PiiSanitizer,
    bundle: &mut BugReportBundleDto,
    level: PiiFilterLevel,
) {
    let effective = match level {
        PiiFilterLevel::Off | PiiFilterLevel::Basic => PiiFilterLevel::Standard,
        other => other,
    };

    for entry in &mut bundle.diagnostics.recent_audit_entries {
        if let Some(ref mut details) = entry.details {
            *details = sanitizer.sanitize_text(details, effective);
        }
    }

    for entry in &mut bundle.diagnostics.recent_policy_events {
        if let Some(ref mut details) = entry.details {
            *details = sanitizer.sanitize_text(details, effective);
        }
    }

    if let Some(ref mut logs) = bundle.runtime_logs {
        logs.recent_text = sanitizer.sanitize_text(&logs.recent_text, effective);
        logs.log_dir = sanitizer.sanitize_text(&logs.log_dir, effective);
        if let Some(ref mut file) = logs.log_file {
            *file = sanitizer.sanitize_text(file, effective);
        }
    }

    if let Some(ref mut path) = bundle.diagnostics.health.frames_dir_path {
        *path = sanitizer.sanitize_text(path, effective);
    }
    if let Some(ref mut err) = bundle.diagnostics.health.storage_error {
        *err = sanitizer.sanitize_text(err, effective);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_bug_id_format() {
        let id = generate_bug_id("0.4.16", "macos");
        let s = id.as_str();
        assert!(s.starts_with("BUG-"));
        assert_eq!(s.len(), 16);
    }

    #[test]
    fn generate_bug_id_unique() {
        let a = generate_bug_id("0.4.16", "macos");
        let b = generate_bug_id("0.4.16", "macos");
        assert_ne!(a, b);
    }

    #[test]
    fn parse_pii_level_defaults_to_standard() {
        assert!(matches!(parse_pii_level(None), PiiFilterLevel::Standard));
        assert!(matches!(
            parse_pii_level(Some("anything")),
            PiiFilterLevel::Standard
        ));
    }

    #[test]
    fn parse_pii_level_strict() {
        assert!(matches!(
            parse_pii_level(Some("strict")),
            PiiFilterLevel::Strict
        ));
    }

    struct MockSanitizer;
    impl PiiSanitizer for MockSanitizer {
        fn sanitize_text(&self, text: &str, _level: PiiFilterLevel) -> String {
            text.replace("user@example.com", "[EMAIL]")
                .replace("/Users/alice", "[USER]")
        }
    }

    #[test]
    fn sanitize_bundle_filters_audit_details() {
        use oneshim_api_contracts::support::{
            AuditEntryDto, DiagnosticsBundleDto, DiagnosticsHealthDto,
        };

        let mut bundle = BugReportBundleDto {
            bug_id: "BUG-000000000000".to_string(),
            diagnostics: DiagnosticsBundleDto {
                schema_version: "test".to_string(),
                generated_at: "now".to_string(),
                health: DiagnosticsHealthDto {
                    storage_ok: true,
                    storage_error: None,
                    frames_dir_configured: false,
                    frames_dir_path: Some("/Users/alice/frames".to_string()),
                    frames_dir_exists: None,
                    config_manager_configured: false,
                    automation_controller_configured: false,
                    update_control_configured: false,
                },
                settings_snapshot: Default::default(),
                storage_stats: None,
                recent_audit_entries: vec![AuditEntryDto {
                    entry_id: "1".to_string(),
                    timestamp: "t".to_string(),
                    session_id: "s".to_string(),
                    command_id: "c".to_string(),
                    action_type: "test".to_string(),
                    status: "ok".to_string(),
                    details: Some("contact user@example.com".to_string()),
                    execution_time_ms: None,
                }],
                recent_policy_events: vec![],
            },
            system: SystemInfoDto {
                app_version: "0.4.16".to_string(),
                os_name: "macos".to_string(),
                os_version: "15.4".to_string(),
                arch: "aarch64".to_string(),
                runtime: "tauri-desktop".to_string(),
                cpu_count: 10,
                memory_total_mb: 16384,
                memory_available_mb: 8192,
                uptime_seconds: 3600,
            },
            connection: ConnectionStatusDto {
                server_reachable: false,
                last_sync_at: None,
                grpc_enabled: false,
                websocket_connected: false,
            },
            runtime_logs: None,
            pii_filter_level: "Standard".to_string(),
        };

        sanitize_bundle(&MockSanitizer, &mut bundle, PiiFilterLevel::Standard);

        let details = bundle.diagnostics.recent_audit_entries[0]
            .details
            .as_ref()
            .unwrap();
        assert!(details.contains("[EMAIL]"));
        assert!(!details.contains("user@example.com"));

        let path = bundle.diagnostics.health.frames_dir_path.as_ref().unwrap();
        assert!(path.contains("[USER]"));
        assert!(!path.contains("/Users/alice"));
    }

    #[test]
    fn sanitize_bundle_enforces_minimum_standard() {
        use oneshim_api_contracts::support::{DiagnosticsBundleDto, DiagnosticsHealthDto};

        let mut bundle = BugReportBundleDto {
            bug_id: "BUG-000000000000".to_string(),
            diagnostics: DiagnosticsBundleDto {
                schema_version: "test".to_string(),
                generated_at: "now".to_string(),
                health: DiagnosticsHealthDto {
                    storage_ok: true,
                    storage_error: Some("error at /Users/alice/db".to_string()),
                    frames_dir_configured: false,
                    frames_dir_path: None,
                    frames_dir_exists: None,
                    config_manager_configured: false,
                    automation_controller_configured: false,
                    update_control_configured: false,
                },
                settings_snapshot: Default::default(),
                storage_stats: None,
                recent_audit_entries: vec![],
                recent_policy_events: vec![],
            },
            system: SystemInfoDto {
                app_version: "0.4.16".to_string(),
                os_name: "macos".to_string(),
                os_version: "15.4".to_string(),
                arch: "aarch64".to_string(),
                runtime: "web".to_string(),
                cpu_count: 4,
                memory_total_mb: 8192,
                memory_available_mb: 4096,
                uptime_seconds: 100,
            },
            connection: ConnectionStatusDto {
                server_reachable: false,
                last_sync_at: None,
                grpc_enabled: false,
                websocket_connected: false,
            },
            runtime_logs: None,
            pii_filter_level: "Off".to_string(),
        };

        // Even with Off level, sanitize_bundle enforces Standard minimum
        sanitize_bundle(&MockSanitizer, &mut bundle, PiiFilterLevel::Off);

        let err = bundle.diagnostics.health.storage_error.as_ref().unwrap();
        assert!(err.contains("[USER]"));
    }
}
```

- [ ] **Step 3: Register module**

Add to `crates/oneshim-web/src/services/mod.rs`:
```rust
pub mod bug_report_service;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-web bug_report`
Expected: all tests pass (generate_bug_id_format, generate_bug_id_unique, parse_pii_level_*, sanitize_bundle_*)

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-web/Cargo.toml crates/oneshim-web/src/services/bug_report_service.rs crates/oneshim-web/src/services/mod.rs
git commit -m "feat(web): add BugReportService with ID generation and PII sanitization"
```

---

## Task 5: AppState + BugReportContext + REST Handler

**Files:**
- Create: `crates/oneshim-web/src/handlers/bug_report.rs`
- Modify: `crates/oneshim-web/src/lib.rs`
- Modify: `crates/oneshim-web/src/handlers/mod.rs`
- Modify: `crates/oneshim-web/src/services/web_contexts/mod.rs`
- Modify: `crates/oneshim-web/src/routes.rs`

- [ ] **Step 1: Add fields to AppState**

In `crates/oneshim-web/src/lib.rs`, add import:
```rust
use oneshim_core::ports::pii_sanitizer::PiiSanitizer;
```

Add two fields to `AppState` struct (after `pomodoro`):
```rust
    pub pii_sanitizer: Option<Arc<dyn PiiSanitizer>>,
    pub latest_bug_report: Arc<std::sync::Mutex<Option<oneshim_api_contracts::bug_report::BugReportBundleDto>>>,
```

Add to `WebServer::new()` initialization:
```rust
                pii_sanitizer: None,
                latest_bug_report: Arc::new(std::sync::Mutex::new(None)),
```

Add builder method (after existing `with_*` methods):
```rust
    pub fn with_pii_sanitizer(mut self, sanitizer: Arc<dyn PiiSanitizer>) -> Self {
        self.state.pii_sanitizer = Some(sanitizer);
        self
    }
```

- [ ] **Step 2: Create BugReportContext**

In `crates/oneshim-web/src/services/web_contexts/mod.rs`, add:

```rust
use oneshim_core::ports::pii_sanitizer::PiiSanitizer;
use oneshim_api_contracts::bug_report::BugReportBundleDto;
use oneshim_api_contracts::support::RuntimeLogSnapshotDto;

#[derive(Clone)]
pub struct BugReportContext {
    pub support: SupportDiagnosticsContext,
    pub pii_sanitizer: Option<Arc<dyn PiiSanitizer>>,
    pub runtime_logs: Option<RuntimeLogSnapshotDto>,
    pub latest: Arc<std::sync::Mutex<Option<BugReportBundleDto>>>,
}

impl FromRef<AppState> for BugReportContext {
    fn from_ref(state: &AppState) -> Self {
        Self {
            support: SupportDiagnosticsContext::from_ref(state),
            pii_sanitizer: state.pii_sanitizer.clone(),
            runtime_logs: None,
            latest: state.latest_bug_report.clone(),
        }
    }
}
```

- [ ] **Step 3: Create handler**

```rust
// crates/oneshim-web/src/handlers/bug_report.rs

use axum::extract::State;
use axum::Json;
use oneshim_api_contracts::bug_report::{BugReportBundleDto, CreateBugReportRequest};

use crate::error::ApiError;
use crate::services::bug_report_service::BugReportService;
use crate::services::web_contexts::BugReportContext;

pub async fn create_bug_report(
    State(ctx): State<BugReportContext>,
) -> Result<Json<BugReportBundleDto>, ApiError> {
    let latest = ctx.latest.clone();
    let service = BugReportService::new(ctx);
    let bundle = service.create_report(true, None).await;

    if let Ok(mut guard) = latest.lock() {
        *guard = Some(bundle.clone());
    }

    Ok(Json(bundle))
}

pub async fn create_bug_report_with_params(
    State(ctx): State<BugReportContext>,
    Json(params): Json<CreateBugReportRequest>,
) -> Result<Json<BugReportBundleDto>, ApiError> {
    let latest = ctx.latest.clone();
    let service = BugReportService::new(ctx);
    let bundle = service
        .create_report(params.include_logs, params.pii_level)
        .await;

    if let Ok(mut guard) = latest.lock() {
        *guard = Some(bundle.clone());
    }

    Ok(Json(bundle))
}

pub async fn get_latest_bug_report(
    State(ctx): State<BugReportContext>,
) -> Result<Json<BugReportBundleDto>, ApiError> {
    let guard = ctx
        .latest
        .lock()
        .map_err(|_| ApiError::Internal("lock poisoned".to_string()))?;

    match guard.as_ref() {
        Some(bundle) => Ok(Json(bundle.clone())),
        None => Err(ApiError::NotFound("no bug report generated yet".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_request_deserializes_defaults() {
        let req: CreateBugReportRequest = serde_json::from_str("{}").unwrap();
        assert!(req.include_logs);
        assert!(req.pii_level.is_none());
    }

    #[test]
    fn create_request_with_params() {
        let req: CreateBugReportRequest =
            serde_json::from_str(r#"{"include_logs":false,"pii_level":"strict"}"#).unwrap();
        assert!(!req.include_logs);
        assert_eq!(req.pii_level.as_deref(), Some("strict"));
    }
}
```

- [ ] **Step 4: Register handler module**

Add to `crates/oneshim-web/src/handlers/mod.rs`:
```rust
pub mod bug_report;
```

- [ ] **Step 5: Add routes**

In `crates/oneshim-web/src/routes.rs`, add inside `api_routes()`:
```rust
        .route(
            "/support/bug-report",
            get(handlers::bug_report::create_bug_report)
                .post(handlers::bug_report::create_bug_report_with_params),
        )
        .route(
            "/support/bug-report/latest",
            get(handlers::bug_report::get_latest_bug_report),
        )
```

- [ ] **Step 6: Run tests and check**

Run: `cargo check -p oneshim-web && cargo test -p oneshim-web bug_report`
Expected: compiles and tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/oneshim-web/src/lib.rs crates/oneshim-web/src/handlers/bug_report.rs crates/oneshim-web/src/handlers/mod.rs crates/oneshim-web/src/services/web_contexts/mod.rs crates/oneshim-web/src/routes.rs
git commit -m "feat(web): add bug report REST endpoints and BugReportContext"
```

---

## Task 6: Tauri IPC Command + DI Wiring

**Files:**
- Create: `src-tauri/src/commands/bug_report.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `Cargo.toml` (workspace)
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add tauri-plugin-dialog to workspace**

In root `Cargo.toml` under `[workspace.dependencies]`:
```toml
tauri-plugin-dialog = "2"
```

In `src-tauri/Cargo.toml` under `[dependencies]`:
```toml
tauri-plugin-dialog = { workspace = true }
```

- [ ] **Step 2: Create Tauri IPC command**

```rust
// src-tauri/src/commands/bug_report.rs

#[tauri::command]
pub async fn export_bug_report(
    app: tauri::AppHandle,
    bug_id: String,
    bundle_json: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let path = app
        .dialog()
        .file()
        .set_file_name(format!("oneshim-report-{bug_id}.json"))
        .add_filter("JSON", &["json"])
        .blocking_save_file();

    match path {
        Some(file_path) => {
            let p = file_path.as_path().ok_or("invalid path")?;
            tokio::fs::write(p, &bundle_json)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Some(p.display().to_string()))
        }
        None => Ok(None),
    }
}
```

- [ ] **Step 3: Register module**

Add to `src-tauri/src/commands/mod.rs`:
```rust
pub(crate) mod bug_report;
```

- [ ] **Step 4: Wire VisionPiiSanitizer in main.rs**

In `src-tauri/src/main.rs`, find where `WebServer` is built (search for `WebServer::new`). After the existing `.with_*()` chain, add:

```rust
use oneshim_vision::privacy::VisionPiiSanitizer;
use oneshim_core::ports::pii_sanitizer::PiiSanitizer;

// In the WebServer builder chain:
.with_pii_sanitizer(Arc::new(VisionPiiSanitizer) as Arc<dyn PiiSanitizer>)
```

Also register the Tauri IPC command and dialog plugin. Find the `.invoke_handler(tauri::generate_handler![...])` call and add `commands::bug_report::export_bug_report`.

Add dialog plugin: `.plugin(tauri_plugin_dialog::init())`

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p oneshim-app` (the src-tauri package)
Expected: compiles with 0 errors

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src-tauri/Cargo.toml src-tauri/src/commands/bug_report.rs src-tauri/src/commands/mod.rs src-tauri/src/main.rs
git commit -m "feat(tauri): add bug report export IPC and PiiSanitizer DI wiring"
```

---

## Task 7: Frontend Types + API Client

**Files:**
- Create: `crates/oneshim-web/frontend/src/api/bug-report.ts`
- Modify: `crates/oneshim-web/frontend/src/api/contracts.ts`

- [ ] **Step 1: Add TypeScript types**

At the end of `crates/oneshim-web/frontend/src/api/contracts.ts`:

```typescript
// --- Bug Report ---

export interface BugReportBundle {
  bug_id: string
  diagnostics: DiagnosticsBundleResponse
  system: SystemInfo
  connection: ConnectionStatus
  runtime_logs: RuntimeLogSnapshot | null
  pii_filter_level: string
}

export interface SystemInfo {
  app_version: string
  os_name: string
  os_version: string
  arch: string
  runtime: string
  cpu_count: number
  memory_total_mb: number
  memory_available_mb: number
  uptime_seconds: number
}

export interface ConnectionStatus {
  server_reachable: boolean
  last_sync_at: string | null
  grpc_enabled: boolean
  websocket_connected: boolean
}

export interface RuntimeLogSnapshot {
  generated_at: string
  log_dir: string
  log_file: string | null
  line_count: number
  recent_text: string
}
```

- [ ] **Step 2: Create API client + formatters**

```typescript
// crates/oneshim-web/frontend/src/api/bug-report.ts

import { apiBase } from './client'
import type { BugReportBundle } from './contracts'

export async function createBugReport(
  includeLog = true,
  piiLevel?: string,
): Promise<BugReportBundle> {
  const res = await fetch(`${apiBase}/support/bug-report`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      include_logs: includeLog,
      pii_level: piiLevel ?? null,
    }),
  })
  if (!res.ok) throw new Error(`Bug report failed: ${res.status}`)
  return res.json()
}

export type ClipboardFormat = 'json' | 'text'

export function formatBundleForClipboard(
  bundle: BugReportBundle,
  format: ClipboardFormat,
): string {
  if (format === 'json') {
    return JSON.stringify(bundle, null, 2)
  }
  return [
    '=== ONESHIM Bug Report ===',
    `Bug ID: ${bundle.bug_id}`,
    `Generated: ${bundle.diagnostics.generated_at}`,
    '',
    '--- System ---',
    `App Version: ${bundle.system.app_version}`,
    `OS: ${bundle.system.os_name} ${bundle.system.os_version} (${bundle.system.arch})`,
    `Runtime: ${bundle.system.runtime}`,
    `CPU: ${bundle.system.cpu_count} cores`,
    `Memory: ${bundle.system.memory_available_mb}/${bundle.system.memory_total_mb} MB`,
    '',
    '--- Health ---',
    `Storage OK: ${bundle.diagnostics.health.storage_ok}`,
    `Frames Dir: ${bundle.diagnostics.health.frames_dir_exists ?? 'unknown'}`,
    '',
    '--- Connection ---',
    `Server: ${bundle.connection.server_reachable ? 'reachable' : 'unreachable'}`,
    `Last Sync: ${bundle.connection.last_sync_at ?? 'never'}`,
    `gRPC: ${bundle.connection.grpc_enabled ? 'enabled' : 'disabled'}`,
    '',
    `--- Recent Audit (${bundle.diagnostics.recent_audit_entries.length}) ---`,
    ...bundle.diagnostics.recent_audit_entries
      .slice(0, 10)
      .map((e) => `  [${e.timestamp}] ${e.action_type}: ${e.status}`),
    bundle.diagnostics.recent_audit_entries.length > 10
      ? `  ... and ${bundle.diagnostics.recent_audit_entries.length - 10} more`
      : '',
  ]
    .filter(Boolean)
    .join('\n')
}

const ISSUE_REPO = 'https://github.com/pseudotop/oneshim-client/issues/new'

export function buildBugReportIssueUrl(bundle: BugReportBundle): string {
  const params = new URLSearchParams({
    title: `Bug report: ${bundle.system.app_version}`,
    body: [
      '## Summary',
      '<!-- Describe the issue here -->',
      '',
      '## Bug ID',
      `\`${bundle.bug_id}\``,
      '',
      '## Environment',
      `- App version: ${bundle.system.app_version}`,
      `- Runtime: ${bundle.system.runtime}`,
      `- OS: ${bundle.system.os_name} ${bundle.system.os_version} (${bundle.system.arch})`,
      `- Storage OK: ${bundle.diagnostics.health.storage_ok}`,
      `- Connection: ${bundle.connection.server_reachable ? 'server reachable' : 'server unreachable'}`,
      '',
      '## Reproduction',
      '1. ',
      '',
      '## Expected',
      '',
      '## Actual',
      '',
      '## Notes',
      '- If you exported a diagnostic report, please email it to support@oneshim.dev with this Bug ID in the subject line.',
    ].join('\n'),
  })
  return `${ISSUE_REPO}?${params.toString()}`
}

export function buildMailtoUrl(bugId: string): string {
  const subject = encodeURIComponent(`Bug Report ${bugId}`)
  const body = encodeURIComponent(
    `Bug ID: ${bugId}\n\nPlease attach the exported diagnostic report (oneshim-report-${bugId}.json) to this email.\n\nDescribe the issue:\n`,
  )
  return `mailto:support@oneshim.dev?subject=${subject}&body=${body}`
}
```

- [ ] **Step 3: Verify frontend lint**

Run: `cd crates/oneshim-web/frontend && pnpm lint`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-web/frontend/src/api/bug-report.ts crates/oneshim-web/frontend/src/api/contracts.ts
git commit -m "feat(frontend): add bug report API client, formatters, and types"
```

---

## Task 8: BugReportWizard Component + i18n

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/BugReportWizard.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/en.json`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/ko.json`

This task is large and represents the frontend wizard UI. The implementer should:

- [ ] **Step 1: Add i18n keys**

Add to `en.json` under `settings` namespace (or new `bugReport` namespace):
```json
"bugReport": {
  "title": "Bug Report",
  "desc": "Generate a bug report with diagnostic data for troubleshooting.",
  "generating": "Generating report...",
  "bugIdLabel": "Bug ID",
  "bugIdCopied": "Bug ID copied to clipboard",
  "stepGenerate": "Generate",
  "stepReview": "Review Data",
  "stepShare": "Share",
  "piiLevel": "Privacy Level",
  "piiStandard": "Standard",
  "piiStrict": "Strict",
  "includeLogs": "Include runtime logs",
  "openGithub": "Open GitHub Issue",
  "copyJson": "Copy JSON",
  "copyText": "Copy Text",
  "exportReport": "Export Report",
  "emailSupport": "Email Support",
  "copied": "Copied to clipboard",
  "exported": "Report exported",
  "exportCancelled": "Export cancelled",
  "exportFailed": "Export failed",
  "noReportYet": "Click Generate to create a bug report"
}
```

Add equivalent Korean keys to `ko.json`.

- [ ] **Step 2: Create BugReportWizard component**

Create `crates/oneshim-web/frontend/src/components/BugReportWizard.tsx` with a 3-step dialog wizard:

1. Step 1 (Generate): PII level selector, include logs toggle, Generate button
2. Step 2 (Review): Structured preview of the sanitized bundle data
3. Step 3 (Share): Four action buttons (GitHub Issue, Copy JSON/Text, Export, Email)

The component uses the existing `Dialog`, `DialogContent`, `DialogTitle`, `DialogBody`, `DialogFooter`, `Button`, `Card`, `Alert`, `Checkbox` components and the `addToast` utility.

Key behaviors:
- Calls `createBugReport()` API on generate
- Shows Bug ID prominently with copy-to-clipboard
- Uses `formatBundleForClipboard()` for clipboard operations
- Uses `buildBugReportIssueUrl()` for GitHub issue link
- Uses `buildMailtoUrl()` for email link
- Uses `invokeDesktop('export_bug_report', { bugId, bundleJson })` for file export (Tauri only)

- [ ] **Step 3: Update GeneralTab.tsx**

Replace the `handleReportBug` function in `SupportToolsCard` to open the `BugReportWizard` dialog instead of directly calling `window.open(buildIssueUrl(...))`.

Import the new component and add state for its visibility:
```typescript
import { BugReportWizard } from '../../components/BugReportWizard'

// In SupportToolsCard:
const [wizardOpen, setWizardOpen] = useState(false)

// Replace handleReportBug button onClick with:
onClick={() => setWizardOpen(true)}

// Add after the existing Dialog:
<BugReportWizard open={wizardOpen} onClose={() => setWizardOpen(false)} />
```

Remove the old `buildIssueUrl()` function from GeneralTab.tsx (it's now in `bug-report.ts`).

- [ ] **Step 4: Verify frontend**

Run: `cd crates/oneshim-web/frontend && pnpm lint && pnpm build`
Expected: 0 errors, build succeeds

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-web/frontend/src/components/BugReportWizard.tsx crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx crates/oneshim-web/frontend/src/i18n/locales/en.json crates/oneshim-web/frontend/src/i18n/locales/ko.json
git commit -m "feat(frontend): add BugReportWizard 3-step dialog with i18n"
```

---

## Task 9: E2E Tests

**Files:**
- Create or modify: `crates/oneshim-web/frontend/e2e/settings-support.spec.ts`

- [ ] **Step 1: Write Playwright E2E tests**

```typescript
import { test, expect } from '@playwright/test'
import { mockApiRoutes } from './helpers/mock-api'

test.describe('Bug Report Wizard', () => {
  test.beforeEach(async ({ page }) => {
    await mockApiRoutes(page)
    await page.goto('/settings')
  })

  test('opens wizard from Report Bug button', async ({ page }) => {
    await page.getByRole('button', { name: /report bug/i }).click()
    await expect(page.getByText(/bug report/i)).toBeVisible()
  })

  test('generates bug ID on create', async ({ page }) => {
    // Mock the POST endpoint
    await page.route('**/support/bug-report', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          bug_id: 'BUG-a1b2c3d4e5f6',
          diagnostics: {
            schema_version: 'support.diagnostics.v1',
            generated_at: '2026-04-03T10:00:00Z',
            health: { storage_ok: true, storage_error: null, frames_dir_configured: true, frames_dir_path: null, frames_dir_exists: true, config_manager_configured: true, automation_controller_configured: true, update_control_configured: true },
            settings_snapshot: {},
            storage_stats: null,
            recent_audit_entries: [],
            recent_policy_events: [],
          },
          system: { app_version: '0.4.16', os_name: 'macOS', os_version: '15.4', arch: 'aarch64', runtime: 'web', cpu_count: 10, memory_total_mb: 16384, memory_available_mb: 8192, uptime_seconds: 3600 },
          connection: { server_reachable: true, last_sync_at: '2026-04-03T10:00:00Z', grpc_enabled: false, websocket_connected: false },
          runtime_logs: null,
          pii_filter_level: 'Standard',
        }),
      }),
    )

    await page.getByRole('button', { name: /report bug/i }).click()
    await page.getByRole('button', { name: /generate/i }).click()
    await expect(page.getByText('BUG-a1b2c3d4e5f6')).toBeVisible()
  })
})
```

- [ ] **Step 2: Run E2E tests**

Run: `cd crates/oneshim-web/frontend && pnpm exec playwright test e2e/settings-support.spec.ts`
Expected: tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/e2e/settings-support.spec.ts
git commit -m "test(e2e): add bug report wizard Playwright tests"
```

---

## Task 10: Full Workspace Verification

- [ ] **Step 1: Run full Rust test suite**

Run: `cargo test --workspace`
Expected: all tests pass, 0 failures

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace`
Expected: 0 warnings

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: no formatting changes needed

- [ ] **Step 4: Run frontend lint + build**

Run: `cd crates/oneshim-web/frontend && pnpm lint && pnpm build`
Expected: 0 errors

- [ ] **Step 5: Run full E2E suite**

Run: `cd crates/oneshim-web/frontend && pnpm exec playwright test`
Expected: all tests pass including new bug report tests

- [ ] **Step 6: Commit any fixes**

If any issues found, fix and commit with descriptive message.
