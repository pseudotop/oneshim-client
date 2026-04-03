# Bug Report Flow — Design Specification

**Date**: 2026-04-03
**Status**: Draft
**Version**: 0.4.16 target

## 1. Overview

Enhance the existing support diagnostics system into a full bug report flow with Bug ID correlation, two-channel reporting (GitHub Issue + Email), PII-filtered diagnostic bundle export, and dual-format clipboard output.

### Goals

- Users can report bugs with minimal friction while maintaining privacy
- Support can correlate GitHub issues with detailed diagnostic bundles via Bug ID
- All exported data passes through PII filtering before leaving the device
- Users can preview exactly what will be shared before submission (view-before-send)

### Non-Goals

- Automatic crash reporting (opt-in only, no telemetry)
- Server-side diagnostic bundle storage (local export + email only)
- Remote log collection without explicit user action

## 2. Research-Informed Design Decisions

| Decision | Inspired By | Rationale |
|----------|-------------|-----------|
| Bug ID (SHA-256 prefix) | Tailscale `BUG-[key]-[ts]-[rand]` | Correlation without PII. 12 hex chars = 48-bit collision space, sufficient for support tracking |
| Two-channel reporting | Tailscale (ID + bundle), Firefox (dual format) | Separate sensitive from non-sensitive data. GitHub issue is public; email is private |
| View-before-send preview | Signal Desktop, Firefox | User must see exactly what leaves the device. Trust through transparency |
| Dual-format clipboard | Firefox about:support | JSON for developers/automation, plain text for pasting in forums/issues |
| PII filter on all exports | All projects (learned from OBS/Signal failures) | Never export unfiltered data. Apply existing `PiiFilterLevel::Standard` minimum |
| Opt-in, default off | Brave, Firefox crash reporter | No data leaves device without explicit user action |
| Local-first storage | All projects | Diagnostic bundle generated and stored locally. Upload/share is separate step |

## 3. Architecture

### 3.1 Component Overview

```
┌─────────────────────────────────────────────────────────┐
│                      Frontend (React)                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │BugReportWiz  │  │ PreviewPanel │  │ ExportPanel  │  │
│  │ (3-step)     │  │ (view data)  │  │ (copy/save)  │  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  │
│         └─────────────────┼──────────────────┘          │
│                           │ REST API                     │
├───────────────────────────┼─────────────────────────────┤
│                     oneshim-web                          │
│  ┌────────────────────────┴────────────────────────┐    │
│  │           BugReportService                       │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │    │
│  │  │ BugIdGen │ │ Bundler  │ │Arc<dyn PiiSanit> │ │    │
│  │  └──────────┘ └──────────┘ └────────┬─────────┘ │    │
│  └─────────────────────────────────────┼────────────┘    │
│                                        │ port            │
│                              oneshim-core                │
│                           ┌────────────┴───────────┐     │
│                           │ PiiSanitizer trait      │     │
│                           │ BugId model             │     │
│                           └────────────┬───────────┘     │
│                                        │ impl            │
│                              oneshim-vision              │
│                           ┌────────────┴───────────┐     │
│                           │ VisionPiiSanitizer      │     │
│                           │ (wraps privacy.rs)      │     │
│                           └────────────────────────┘     │
│                                                          │
│  src-tauri/main.rs: wires VisionPiiSanitizer → WebServer │
└──────────────────────────────────────────────────────────┘
```

### 3.2 Crate Responsibilities

| Crate | New Code | Purpose |
|-------|----------|---------|
| `oneshim-core` | `models/bug_report.rs` | `BugId` newtype model |
| `oneshim-core` | `ports/pii_sanitizer.rs` | `PiiSanitizer` trait — text sanitization port |
| `oneshim-vision` | (modify `privacy.rs`) | Implement `PiiSanitizer` trait on existing functions |
| `oneshim-web` | `services/bug_report_service.rs` | Bug ID generation, bundle assembly, PII sanitization orchestration |
| `oneshim-web` | `handlers/bug_report.rs` | REST endpoints for bug report flow |
| `oneshim-web/frontend` | `BugReportWizard.tsx` | 3-step wizard UI component |
| `oneshim-api-contracts` | `bug_report.rs` | Shared DTOs between frontend and backend |
| `src-tauri` | `commands/bug_report.rs` | Tauri IPC commands (file save dialog, system info) |

### 3.3 Data Flow

```
User clicks "Report Bug"
  → Step 1: Generate Bug ID (local, instant)
  → Step 2: Assemble DiagnosticsBundle (extended)
  → Step 3: Apply PII filter (Standard level minimum)
  → Step 4: Preview panel shows sanitized data
  → User chooses action:
     ├─ "Open GitHub Issue" → pre-filled template with Bug ID only (no sensitive data)
     ├─ "Copy to Clipboard" → JSON or plain text format
     ├─ "Export Report" → PII-filtered JSON file download (Tauri save dialog)
     └─ "Email Support" → mailto: link with Bug ID in subject, instructions to attach exported file
```

## 4. Detailed Design

### 4.1 Bug ID Generation

The `BugId` type is a lightweight newtype in `oneshim-core` (no crypto deps). The generation logic lives in `oneshim-web`'s `BugReportService`, which already depends on `sha2`/`hex`/`rand` transitively and is an appropriate place for infrastructure logic.

```rust
// oneshim-core/src/models/bug_report.rs — type only, no generation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugId(String);

impl BugId {
    /// Create from a pre-computed string. Validated at construction.
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
```

```rust
// oneshim-web/src/services/bug_report_service.rs — generation logic
fn generate_bug_id(app_version: &str, os_info: &str) -> BugId {
    use sha2::{Sha256, Digest};
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
            .to_le_bytes()
    );
    hasher.update(&rand::random::<[u8; 8]>());
    let hash = hasher.finalize();
    BugId::new(format!("BUG-{}", hex::encode(&hash[..6]))).expect("format is valid")
}
```

**Properties:**
- 12 hex chars = 6 bytes of hash output. Birthday-bound collision resistance: ~2^24 (~16 million) — more than sufficient for support ticket correlation
- Includes 8 random bytes so same device + same second produces different IDs
- `BUG-` prefix makes it grep-able in logs and issues
- No PII in the ID itself (version + OS are public metadata)
- No crypto deps added to `oneshim-core` — generation stays in `oneshim-web`

### 4.2 Extended DiagnosticsBundle

Extend the existing `DiagnosticsBundleDto` with:

```rust
// oneshim-api-contracts/src/bug_report.rs

#[derive(Debug, Serialize, Deserialize)]
pub struct BugReportBundleDto {
    /// Bug identifier for correlation
    pub bug_id: String,
    /// Existing diagnostics (health, settings, audit, policy)
    pub diagnostics: DiagnosticsBundleDto,
    /// System info (new)
    pub system: SystemInfoDto,
    /// Connection status (new)
    pub connection: ConnectionStatusDto,
    /// Runtime log snippet (optional, Tauri only)
    pub runtime_logs: Option<RuntimeLogSnapshotDto>,
    /// PII filter level applied to this bundle
    pub pii_filter_level: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfoDto {
    pub app_version: String,
    pub os_name: String,
    pub os_version: String,
    pub arch: String,
    pub runtime: String, // "tauri-desktop" | "web"
    pub cpu_count: usize,
    pub memory_total_mb: u64,
    pub memory_available_mb: u64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionStatusDto {
    pub server_reachable: bool,
    pub last_sync_at: Option<String>,
    pub grpc_enabled: bool,
    pub websocket_connected: bool,
}
```

**Note:** The existing `DiagnosticsBundleDto`, `DiagnosticsHealthDto`, and `RuntimeLogSnapshotDto` in `oneshim-api-contracts/src/support.rs` currently derive only `Serialize`. Since `BugReportBundleDto` embeds them and the frontend needs to receive the response, `Deserialize` must be added to these existing types. This is a backward-compatible change (adding a derive does not break existing serialization).

```text
Files to update with #[derive(Deserialize)]:
- oneshim-api-contracts/src/support.rs: DiagnosticsBundleDto, DiagnosticsHealthDto, RuntimeLogSnapshotDto, AuditEntryDto
```

### 4.3 PII Sanitization for Bug Reports

Since `oneshim-web` cannot depend on `oneshim-vision` (adapter-to-adapter dependency forbidden), PII sanitization is exposed via a port trait in `oneshim-core`, implemented in `oneshim-vision`, and injected into `BugReportService` via `Arc<dyn PiiSanitizer>`.

```rust
// oneshim-core/src/ports/pii_sanitizer.rs
pub trait PiiSanitizer: Send + Sync {
    fn sanitize_text(&self, text: &str, level: PiiFilterLevel) -> String;
}
```

```rust
// oneshim-vision/src/privacy.rs — add impl
pub struct VisionPiiSanitizer;

impl PiiSanitizer for VisionPiiSanitizer {
    fn sanitize_text(&self, text: &str, level: PiiFilterLevel) -> String {
        sanitize_title_with_level(text, level)
    }
}
```

```rust
// oneshim-web/src/services/bug_report_service.rs
impl BugReportService {
    /// Sanitize the bundle by applying PII filter to all string fields.
    /// Minimum level: Standard (emails, phones, cards, Korean IDs, user paths).
    fn sanitize_bundle(&self, bundle: &mut BugReportBundleDto, level: PiiFilterLevel) {
        let effective_level = match level {
            PiiFilterLevel::Off | PiiFilterLevel::Basic => PiiFilterLevel::Standard,
            other => other,
        };
        let s = &self.sanitizer; // Arc<dyn PiiSanitizer>

        // Sanitize audit entry details
        for entry in &mut bundle.diagnostics.recent_audit_entries {
            if let Some(ref mut details) = entry.details {
                *details = s.sanitize_text(details, effective_level);
            }
        }

        // Sanitize runtime logs
        if let Some(ref mut logs) = bundle.runtime_logs {
            logs.recent_text = s.sanitize_text(&logs.recent_text, effective_level);
            logs.log_dir = s.sanitize_text(&logs.log_dir, effective_level);
            if let Some(ref mut file) = logs.log_file {
                *file = s.sanitize_text(file, effective_level);
            }
        }

        // Sanitize health paths
        if let Some(ref mut path) = bundle.diagnostics.health.frames_dir_path {
            *path = s.sanitize_text(path, effective_level);
        }
        if let Some(ref mut err) = bundle.diagnostics.health.storage_error {
            *err = s.sanitize_text(err, effective_level);
        }

        // Sanitize policy event details (same structure as audit)
        for entry in &mut bundle.diagnostics.recent_policy_events {
            if let Some(ref mut details) = entry.details {
                *details = s.sanitize_text(details, effective_level);
            }
        }
    }
}
```

**Key decisions:**
- Minimum filter level is `Standard` — cannot export with `Off` or `Basic`
- All sanitization goes through `Arc<dyn PiiSanitizer>` port (no adapter-to-adapter dependency)
- `VisionPiiSanitizer` wraps existing `sanitize_title_with_level()` functions
- Settings snapshot: `AppSettings` contains only typed config values (booleans, numbers, enums), not free-text strings — no sanitization needed for settings fields

### 4.4 DI Wiring and AppState

`BugReportContext` requires a `PiiSanitizer` and system info access. The threading path:

1. `src-tauri/src/main.rs`: construct `VisionPiiSanitizer` → wrap in `Arc<dyn PiiSanitizer>`
2. Pass to `WebServer` via new `with_pii_sanitizer()` builder method on `WebServerRuntimeBindings`
3. `AppState` gets new field: `pii_sanitizer: Option<Arc<dyn PiiSanitizer>>`
4. `BugReportContext` implements `FromRef<AppState>`, extracting `pii_sanitizer` + existing `SupportDiagnosticsContext` fields
5. Latest report stored as `latest_bug_report: Arc<Mutex<Option<BugReportBundleDto>>>` in `AppState` — single-slot, replaced on each generation

**Modified files for wiring:**
- `crates/oneshim-web/src/lib.rs` — `AppState` + `WebServerRuntimeBindings` (add `pii_sanitizer` field + `latest_bug_report`)
- `src-tauri/src/main.rs` — construct and pass `VisionPiiSanitizer`
- `crates/oneshim-web/src/services/web_contexts/mod.rs` — `BugReportContext` struct + `FromRef<AppState>`

### 4.5 REST API Endpoints

New endpoints under `/support/`:

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/support/bug-report` | Generate Bug ID + assemble sanitized bundle |
| `GET` | `/support/bug-report/latest` | Retrieve the most recently generated report (held in-memory, not persisted) |

```rust
// oneshim-web/src/handlers/bug_report.rs

/// POST /support/bug-report
/// Generates a Bug ID, assembles the full diagnostic bundle,
/// applies PII sanitization, and returns the result.
pub async fn create_bug_report(
    State(ctx): State<BugReportContext>,
    Json(params): Json<CreateBugReportRequest>,
) -> Result<Json<BugReportBundleDto>, ApiError> {
    let service = BugReportService::new(ctx);
    let bundle = service.create_report(params.include_logs, params.pii_level).await?;
    Ok(Json(bundle))
}

#[derive(Debug, Deserialize)]
pub struct CreateBugReportRequest {
    #[serde(default = "default_true")]
    pub include_logs: bool,
    #[serde(default)]
    pub pii_level: Option<PiiFilterLevel>, // Standard (default) or Strict
}
```

**Error responses:**
- `500 Internal Server Error` — diagnostics assembly failed (storage unavailable, audit logger error)
- The `PiiSanitizer` is always available (injected at startup). If missing, `BugReportContext::from_ref` falls back to a no-op sanitizer that returns text unchanged.

### 4.6 GitHub Issue Template

Enhanced from the existing `buildIssueUrl()`:

```markdown
## Summary
<!-- Describe the issue here -->

## Bug ID
`BUG-a1b2c3d4e5f6`

## Environment
- App version: 0.4.16
- Runtime: tauri-desktop
- OS: macOS 15.4 (arm64)
- Storage OK: true
- Connection: server reachable

## Reproduction
1.

## Expected

## Actual

## Notes
- If you exported a diagnostic report, please email it to support@oneshim.dev with this Bug ID in the subject line.
```

**Changes from current:**
- Added Bug ID section (new)
- Removed user agent string (contains OS-specific detail already covered)
- Added connection status
- Added email instruction for diagnostic bundle
- Removed internal config details (frames_dir_path, automation_controller_configured — moved to bundle only)

### 4.7 Email Channel

No automatic email sending. The flow is:

1. User exports diagnostic bundle as JSON file
2. User clicks "Email Support" button
3. Opens `mailto:support@oneshim.dev?subject=Bug Report BUG-{id}&body=...`
4. Body contains instructions to attach the exported JSON file
5. User attaches file manually in their email client

**Rationale:** Avoids SMTP dependency, works offline (draft saved in email client), user has full control over what they send. This is the same pattern as Tailscale.

### 4.8 Dual-Format Clipboard

```typescript
// Frontend: clipboard format selection

type ClipboardFormat = 'json' | 'text';

function formatBundleForClipboard(
  bundle: BugReportBundle,
  format: ClipboardFormat
): string {
  if (format === 'json') {
    return JSON.stringify(bundle, null, 2);
  }
  // Plain text format (Firefox about:support style)
  return [
    `=== ONESHIM Bug Report ===`,
    `Bug ID: ${bundle.bug_id}`,
    `Generated: ${bundle.diagnostics.generated_at}`,
    ``,
    `--- System ---`,
    `App Version: ${bundle.system.app_version}`,
    `OS: ${bundle.system.os_name} ${bundle.system.os_version} (${bundle.system.arch})`,
    `Runtime: ${bundle.system.runtime}`,
    `CPU: ${bundle.system.cpu_count} cores`,
    `Memory: ${bundle.system.memory_available_mb}/${bundle.system.memory_total_mb} MB`,
    ``,
    `--- Health ---`,
    `Storage OK: ${bundle.diagnostics.health.storage_ok}`,
    `Frames Dir: ${bundle.diagnostics.health.frames_dir_exists ?? 'unknown'}`,
    ``,
    `--- Connection ---`,
    `Server: ${bundle.connection.server_reachable ? 'reachable' : 'unreachable'}`,
    `Last Sync: ${bundle.connection.last_sync_at ?? 'never'}`,
    `gRPC: ${bundle.connection.grpc_enabled ? 'enabled' : 'disabled'}`,
    ``,
    `--- Recent Audit (${bundle.diagnostics.recent_audit_entries.length}) ---`,
    ...bundle.diagnostics.recent_audit_entries.slice(0, 10).map(
      e => `  [${e.timestamp}] ${e.action_type}: ${e.status}`
    ),
    bundle.diagnostics.recent_audit_entries.length > 10 ? `  ... and ${bundle.diagnostics.recent_audit_entries.length - 10} more` : '',
  ].filter(Boolean).join('\n');
}
```

### 4.9 Configuration Report Export

File save via Tauri dialog. Requires adding `tauri-plugin-dialog` to `src-tauri/Cargo.toml` and `tauri.conf.json` plugin permissions.

```rust
// src-tauri/src/commands/bug_report.rs

#[tauri::command]
async fn export_bug_report(
    app: tauri::AppHandle,
    bug_id: String,
    bundle_json: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let file_path = app.dialog()
        .file()
        .set_file_name(&format!("oneshim-report-{bug_id}.json"))
        .add_filter("JSON", &["json"])
        .save_file()
        .await;

    match file_path {
        Some(path) => {
            tokio::fs::write(&path, &bundle_json)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Some(path.to_string()))
        }
        None => Ok(None), // User cancelled
    }
}
```

### 4.10 Frontend: Bug Report Wizard (3-Step)

Replaces the current inline dialog in `SupportToolsCard`:

**Step 1 — Generate & Preview**
- Auto-generates Bug ID
- Shows sanitized diagnostic data in a structured preview
- PII filter level selector (Standard / Strict)
- Toggle: include runtime logs (Tauri only)

**Step 2 — Review Data**
- Full preview of what will be shared
- Highlighted sections showing what was PII-filtered (before/after diff would be privacy-leaking, so just show "[FILTERED]" markers)
- User can expand/collapse sections

**Step 3 — Share**
- Four action buttons:
  - **Open GitHub Issue** — pre-filled template, Bug ID only (non-sensitive)
  - **Copy to Clipboard** — format toggle (JSON / Text)
  - **Export Report** — save as JSON file (Tauri save dialog)
  - **Email Support** — mailto: link with Bug ID

**UX notes:**
- The wizard is a dialog (same `<Dialog>` component pattern)
- Bug ID is prominently displayed and copyable at the top
- "What data is included" expandable info section
- Link to Privacy page for reset options

## 5. File Changes Summary

### New Files

| File | Purpose |
|------|---------|
| `crates/oneshim-core/src/models/bug_report.rs` | `BugId` struct + generation |
| `crates/oneshim-core/src/ports/pii_sanitizer.rs` | `PiiSanitizer` port trait |
| `crates/oneshim-api-contracts/src/bug_report.rs` | `BugReportBundleDto`, `SystemInfoDto`, `ConnectionStatusDto` DTOs |
| `crates/oneshim-web/src/services/bug_report_service.rs` | Report assembly + PII orchestration |
| `crates/oneshim-web/src/handlers/bug_report.rs` | REST endpoint handler |
| `src-tauri/src/commands/bug_report.rs` | Tauri IPC: export file, system info |
| `crates/oneshim-web/frontend/src/components/BugReportWizard.tsx` | 3-step wizard UI |
| `crates/oneshim-web/frontend/src/api/bug-report.ts` | API client functions |

### Modified Files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/models/mod.rs` | Add `pub mod bug_report;` |
| `crates/oneshim-core/src/ports/mod.rs` | Add `pub mod pii_sanitizer;` |
| `crates/oneshim-vision/src/privacy.rs` | Implement `PiiSanitizer` trait |
| `crates/oneshim-api-contracts/src/lib.rs` | Add `pub mod bug_report;` |
| `crates/oneshim-web/src/routes.rs` | Add bug report routes |
| `crates/oneshim-web/src/handlers/mod.rs` | Add `pub mod bug_report;` |
| `crates/oneshim-web/src/services/mod.rs` | Add `pub mod bug_report_service;` |
| `crates/oneshim-web/src/lib.rs` | Add `pii_sanitizer` + `latest_bug_report` fields to `AppState` |
| `crates/oneshim-web/src/services/web_contexts/mod.rs` | Add `BugReportContext` (extends `SupportDiagnosticsContext` with `Arc<dyn PiiSanitizer>`) |
| `src-tauri/src/commands/mod.rs` | Add `pub mod bug_report;` |
| `src-tauri/src/main.rs` | Wire `VisionPiiSanitizer` → `WebServer` |
| `src-tauri/Cargo.toml` | Add `tauri-plugin-dialog` dependency |
| `src-tauri/tauri.conf.json` | Add dialog plugin permissions |
| `crates/oneshim-api-contracts/src/support.rs` | Add `Deserialize` derive to existing DTOs |
| `crates/oneshim-web/frontend/src/pages/setting-tabs/GeneralTab.tsx` | Replace inline report with wizard |
| `crates/oneshim-web/frontend/src/i18n/locales/en.json` | Add ~20 bug report i18n keys under `bugReport.*` namespace |
| `crates/oneshim-web/frontend/src/i18n/locales/ko.json` | Mirror en.json bug report keys (ko translations) |
| `crates/oneshim-web/frontend/src/api/contracts.ts` | Add bug report types |

### Dependencies

| Crate | New Dependency | Purpose |
|-------|----------------|---------|
| `oneshim-web` | `sha2` (already in workspace) | Bug ID hash generation |
| `oneshim-web` | `hex` (already in workspace) | Hex encoding for Bug ID |
| `oneshim-web` | `rand` (already in workspace) | Random bytes for Bug ID entropy |
| `src-tauri` | `tauri-plugin-dialog` | Native file save dialog for report export |

`oneshim-core` gains no new dependencies. `sha2`/`hex`/`rand` are workspace-level deps already used by other crates. `tauri-plugin-dialog` is a new Tauri plugin and must be added to `src-tauri/Cargo.toml` and `tauri.conf.json` permissions.

## 6. Testing Strategy

### Unit Tests

| Component | Tests | Coverage Target |
|-----------|-------|-----------------|
| `BugId::generate()` | Format validation, uniqueness, determinism with fixed seed | 100% |
| `sanitize_bundle()` | Each PII type filtered, minimum level enforcement, nested field coverage | 100% |
| `BugReportService` | Assembly with/without logs, PII level handling, error propagation | 90%+ |
| `formatBundleForClipboard()` | JSON output validity, text format structure, empty field handling | 100% |
| `buildIssueUrl()` (updated) | Bug ID inclusion, no PII leakage, URL length within limits | 100% |

### Integration Tests

| Scenario | Validation |
|----------|------------|
| `POST /support/bug-report` | Returns valid bundle, Bug ID format correct, PII filtered |
| `GET /support/bug-report/latest` | Returns most recent report or 404 if none generated |
| Export flow (Tauri) | File created at user-selected path, valid JSON |

### E2E Tests (Playwright)

| Test | Steps |
|------|-------|
| Bug report wizard opens | Click "Report Bug" → wizard dialog appears |
| Bug ID displayed | Generate → Bug ID shown in `BUG-xxxxxxxxxxxx` format |
| Preview shows sanitized data | Check no email/phone/path patterns in preview |
| Copy JSON to clipboard | Click copy → clipboard contains valid JSON |
| Copy text to clipboard | Click copy (text mode) → clipboard contains structured text |
| GitHub issue opens | Click "Open GitHub Issue" → new tab with pre-filled template |
| Export report (Tauri) | Click export → file save dialog → file written |

## 7. Privacy & Compliance

| Requirement | Implementation |
|-------------|----------------|
| GDPR Art. 6 (Lawful basis) | Explicit consent: user must click through 3-step wizard |
| GDPR Art. 5(1)(c) (Data minimization) | Only diagnostic data needed for bug resolution. No browsing history, no file contents |
| GDPR Art. 7 (Consent conditions) | Clear preview of all data before export. No pre-checked consent boxes |
| GDPR Art. 17 (Right to erasure) | Exported file is local — user controls deletion. Email attachments follow email retention |
| No automatic upload | All sharing requires explicit user action |
| PII filtering minimum | Standard level enforced (emails, phones, cards, Korean IDs, user paths) |
| No tracking | Bug ID contains no PII. Cannot identify user from ID alone |

## 8. Migration from Current UI

The existing `SupportToolsCard` component (~260 lines) will be refactored:
- The card remains as the entry point (buttons: "View Details", "Report Bug")
- "View Details" opens the existing diagnostics dialog (unchanged)
- "Report Bug" opens the new `BugReportWizard` (replaces current `handleReportBug`)
- Copy diagnostics and copy logs buttons remain in the details dialog
- The `buildIssueUrl()` function moves to `bug-report.ts` and is enhanced with Bug ID

**Breaking changes:** None. The wizard is an enhancement of the existing flow.

## 9. Future Considerations (Out of Scope)

- **Crash handler integration**: Auto-capture crash dumps and associate with Bug ID (future phase)
- **Server-side bundle storage**: Upload diagnostic bundle to ONESHIM server for persistent storage
- **Bracketed reproduction**: Tailscale-style `--record` flag to mark start/end of reproduction steps
- **Safe Mode**: Auto-detect crash-on-startup and offer safe mode (OBS pattern)
- **Extension/feature bisect**: VS Code-style binary search for problematic features
