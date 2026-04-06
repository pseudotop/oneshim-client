# Phase 15: Auto-Update UX Enhancement

## Status: REVIEWED (R1)

**Branch**: `feature/phase15-auto-update` (create from current `feature/phase13-ai-ml` HEAD)
**Depends on**: Phase 13 complete (v0.4.28-rc.2)

---

## Current Infrastructure Summary

The updater is already **fully functional** with:
- Custom updater (`src-tauri/src/updater/`) with state machine coordinator
- GitHub Releases API integration (Stable/PreRelease/Nightly channels)
- SHA256 + Ed25519 signature verification + rollback
- Frontend UI (Updates page + UpdatePanel + SSE streaming)
- System tray menu items + periodic background checks
- `self-replace` binary replacement
- Broadcast channel (`status_tx`) for event propagation to Tauri events + SSE

**What's missing:** Release notes display, real download progress, delta updates, staged rollout.

---

## U1: Release Notes Display + Update Notification

### Gap Analysis

| Component | Status | Detail |
|-----------|--------|--------|
| Fetch release notes from GitHub | **Done** | `ReleaseInfo.body: Option<String>` in `updater/mod.rs:72` |
| Propagate to API contract | **Missing** | `PendingUpdateInfo` lacks `release_notes` field |
| Display in UI | **Missing** | Updates.tsx shows version but no changelog |
| Native notification on update | **Missing** | `NotificationManager` exists but not used for updates |
| Tauri event for update alert | **Partial** | `update:status-changed` emitted but no dedicated notification |

### Design

#### 1. Extend PendingUpdateInfo

In `crates/oneshim-api-contracts/src/update.rs`, add to `PendingUpdateInfo`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub release_notes: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub download_size_bytes: Option<u64>,
```

#### 2. Propagate in Coordinator

In `src-tauri/src/update_coordinator.rs` (~line 273), when building `PendingUpdateInfo`, add:

```rust
release_notes: release.body.clone(),
download_size_bytes: asset.map(|a| a.size),
```

#### 3. Native Update Notification via Event Bridge

**Architecture constraint**: The coordinator does NOT have access to `AppHandle` (it runs in a pure async task with `config, state, action_rx, status_tx`). Passing `AppHandle` would require invasive signature changes.

**Solution**: Use the existing event bridge pattern. The Tauri event bridge (`runtime_bridges.rs::spawn_update_event_bridge`) already listens to `status_tx` and emits Tauri events. Add notification emission there:

```rust
// In runtime_bridges.rs, inside the event bridge loop:
// When PendingApproval is detected, emit a desktop notification
if status.phase == UpdatePhase::PendingApproval {
    if let Some(ref pending) = status.pending {
        let _ = tauri_plugin_notification::NotificationExt::notification(&app_handle)
            .builder()
            .title("Update Available")
            .body(format!("Version {} is ready to install", pending.latest_version))
            .show();
    }
}
```

This works because `spawn_update_event_bridge` already receives `AppHandle`.

#### 4. Frontend Release Notes Display

In `Updates.tsx`, add a release notes section below the version info when `PendingApproval`:
- Render as plain text with `<pre className="whitespace-pre-wrap">` (no markdown parser dependency)
- Max height `max-h-64 overflow-y-auto` with scroll for long notes
- Collapsible via disclosure toggle

### Affected Files

| File | Changes |
|------|---------|
| `crates/oneshim-api-contracts/src/update.rs` | Add `release_notes`, `download_size_bytes` to PendingUpdateInfo |
| `src-tauri/src/update_coordinator.rs` | Propagate release body + asset size |
| `src-tauri/src/runtime_bridges.rs` | Add desktop notification on PendingApproval |
| `crates/oneshim-web/frontend/src/pages/Updates.tsx` | Display release notes section |
| `crates/oneshim-web/frontend/src/components/UpdatePanel.tsx` | Show download size |

### Estimated Tests: ~4

---

## U2: Background Download + Real Progress

### Gap Analysis

| Component | Status | Detail |
|-----------|--------|--------|
| Background async download | **Missing** | `download_update()` blocks via `response.bytes().await` |
| Download progress reporting | **Missing** | No progress metrics; UI shows hardcoded fake 60% bar |
| Separate download/install phases | **Missing** | Single "Installing" conflates both |
| "Ready to install" state | **Missing** | No intermediate state after download |
| Resumable downloads | **Missing** | No Range headers (out of scope for Phase 15) |
| Download size in UI | **Missing** | `ReleaseAsset.size` exists but not propagated (fixed by U1) |

### Design

#### 1. New UpdatePhase States

Extend the `UpdatePhase` enum in `crates/oneshim-api-contracts/src/update.rs`:

```rust
pub enum UpdatePhase {
    Idle,
    Checking,
    PendingApproval,
    Downloading,      // NEW: download in progress
    ReadyToInstall,   // NEW: download complete, awaiting install approval
    Installing,       // NOW: only the binary replacement step
    Updated,
    Deferred,
    Error,
}
```

**Match coverage required** (all places that check UpdatePhase):
- `src-tauri/src/update_coordinator.rs:155` — re-check eligibility guard
- `crates/oneshim-web/frontend/src/pages/Updates.tsx` — multiple conditionals
- `crates/oneshim-web/frontend/src/components/UpdatePanel.tsx` — badge/button rendering
- `src-tauri/src/update_runtime.rs` — implicit phase handling in tests

#### 2. Download Progress Model

Add to `crates/oneshim-api-contracts/src/update.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f32,
}
```

Add to `UpdateStatus`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub download_progress: Option<DownloadProgress>,
```

#### 3. Streaming Download with Progress

Refactor `src-tauri/src/updater/install.rs` `download_update()` to stream chunks:

```rust
use futures_util::stream::StreamExt;

pub async fn download_update_with_progress(
    &self,
    url: &str,
    progress_tx: tokio::sync::watch::Sender<DownloadProgress>,
) -> Result<PathBuf, UpdateError> {
    let response = self.http_client.get(validated_url).send().await?;
    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut file = tokio::fs::File::create(&temp_path).await?;

    let mut stream = response.bytes_stream(); // requires reqwest "stream" feature (already enabled)
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let _ = progress_tx.send(DownloadProgress {
            bytes_downloaded: downloaded,
            total_bytes: total,
            percent: if total > 0 { (downloaded as f32 / total as f32) * 100.0 } else { 0.0 },
        });
    }
    // ... checksum verification, signature verification on the saved file ...
}
```

Note: `reqwest` "stream" feature is already enabled in Cargo.toml. `futures_util::StreamExt` is available via existing `futures-util` dependency.

#### 4. Two-Phase Install Flow

Update the coordinator state machine:

```
PendingApproval --[Approve]--> Downloading --[complete]--> ReadyToInstall --[auto/manual]--> Installing --[complete]--> Updated
```

If `auto_install` is true, skip `ReadyToInstall` and go directly to `Installing`.
If `auto_install` is false, pause at `ReadyToInstall` until user sends `Approve` action.

#### 5. Frontend Real Progress Bar

Replace the fake 60% bar in `Updates.tsx` with real progress from `download_progress`.

### Affected Files

| File | Changes |
|------|---------|
| `crates/oneshim-api-contracts/src/update.rs` | `Downloading`, `ReadyToInstall` variants, `DownloadProgress` struct, `download_progress` on UpdateStatus |
| `src-tauri/src/updater/install.rs` | Streaming download with progress callback |
| `src-tauri/src/update_coordinator.rs` | Two-phase flow (Downloading → ReadyToInstall → Installing), progress forwarding |
| `crates/oneshim-web/frontend/src/pages/Updates.tsx` | Real progress bar, ReadyToInstall UI state |
| `crates/oneshim-web/frontend/src/components/UpdatePanel.tsx` | Progress display, new phase badges |

### Estimated Tests: ~8

---

## U3: Delta Updates (bsdiff)

### Gap Analysis

| Component | Status | Detail |
|-----------|--------|--------|
| bsdiff/bspatch dependency | **Missing** | No delta crate in any Cargo.toml |
| Delta patch module | **Missing** | No `delta.rs` in updater |
| Patch asset discovery | **Missing** | GitHub asset selection only matches full binaries |
| CI/CD patch generation | **Missing** | `release.yml` only builds full binaries |
| Current binary path resolution | **Missing** | Updater doesn't track running binary location |
| Fallback to full binary | **N/A** | Must work when no patch available |

### Design

#### 1. Add bsdiff Dependency

In workspace `Cargo.toml`:

```toml
[workspace.dependencies]
bsdiff = "0.2"
```

In `src-tauri/Cargo.toml`:

```toml
bsdiff = { workspace = true }
```

`bsdiff 0.2` is pure Rust, includes both `diff()` and `patch()` functions. Signature: `bsdiff::patch(old: &[u8], patch: impl Read, new: impl Write) -> io::Result<()>`.

#### 2. Current Binary Path Resolution

The `self-replace` crate (already used for install) internally calls `std::env::current_exe()`. For delta patching, we need the same path:

```rust
// In updater/mod.rs or install.rs:
fn current_binary_path() -> Result<PathBuf, UpdateError> {
    std::env::current_exe()
        .map_err(|e| UpdateError::Install(format!("Cannot determine current binary path: {e}")))
}
```

This is reliable on all 3 platforms when the binary is installed normally (not running from a deleted/moved path).

#### 3. Delta Patch Module

Create `src-tauri/src/updater/delta.rs`:

```rust
use std::io::Cursor;
use super::error::UpdateError;

/// Apply a bsdiff patch to produce the updated binary.
pub fn apply_patch(old_binary: &[u8], patch_data: &[u8]) -> Result<Vec<u8>, UpdateError> {
    let mut new_binary: Vec<u8> = Vec::new();
    bsdiff::patch(old_binary, &mut Cursor::new(patch_data), &mut new_binary)
        .map_err(|e| UpdateError::PatchFailed(format!("bsdiff patch failed: {e}")))?;
    Ok(new_binary)
}
```

#### 4. Asset Selection Logic

In `src-tauri/src/updater/github.rs`, extend asset matching to prefer patches:

Naming convention for patch assets:
```
oneshim-{platform}-{from_version}-to-{to_version}.patch
oneshim-{platform}-{from_version}-to-{to_version}.patch.sha256
```

Selection priority:
1. Try delta patch matching `current_version → latest_version`
2. Fall back to full binary if no patch available

#### 5. Install Flow with Delta Path

In `src-tauri/src/updater/install.rs`:

```rust
pub async fn apply_delta_update(
    &self,
    patch_path: &Path,
) -> Result<PathBuf, UpdateError> {
    let current_binary = current_binary_path()?;
    let old_bytes = tokio::fs::read(&current_binary).await?;
    let patch_bytes = tokio::fs::read(patch_path).await?;
    let new_bytes = delta::apply_patch(&old_bytes, &patch_bytes)?;

    let temp_path = self.write_temp_file("oneshim-patched", &new_bytes).await?;

    // Verify SHA256 of patched binary against expected full-binary checksum
    self.verify_checksum(&temp_path, expected_checksum)?;

    Ok(temp_path)
}
```

**Critical safety**: The SHA256 checksum must be the full binary's checksum (published alongside the full release asset), not the patch's checksum. This ensures the patched result is identical to a fresh download.

#### 6. CI/CD Patch Generation

In `.github/workflows/release.yml`, add a step after binary build:

```yaml
- name: Generate delta patches
  if: steps.previous_release.outputs.exists == 'true'
  run: |
    # Download previous release binary for this platform
    # Generate bsdiff patch: bsdiff old_binary new_binary patch_file
    # Generate SHA256 for patch file
    # Upload patch + patch.sha256 as release assets
```

### Scope Boundaries

- **In scope**: Client-side patch application, asset selection, fallback to full binary, CI/CD patch generation
- **Out of scope**: Multi-version patches (only `current → latest`), resumable patch downloads
- **Constraint**: Patched binary checksum MUST match full-binary checksum (same SHA256)

### Affected Files

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace) | Add `bsdiff = "0.2"` dependency |
| `src-tauri/Cargo.toml` | Add `bsdiff = { workspace = true }` |
| `src-tauri/src/updater/delta.rs` | NEW: `apply_patch()`, `current_binary_path()` |
| `src-tauri/src/updater/mod.rs` | Register module, `UpdateAssetType` enum |
| `src-tauri/src/updater/github.rs` | Patch asset discovery + selection priority |
| `src-tauri/src/updater/install.rs` | `apply_delta_update()` method |
| `.github/workflows/release.yml` | Patch generation step |

### Estimated Tests: ~6

---

## U4: Staged Rollout (Client-Side Probabilistic)

### Gap Analysis

| Component | Status | Detail |
|-----------|--------|--------|
| Device/installation ID | **Partial** | `IntegrationConfig.device_id` exists but not in UpdateConfig |
| Rollout bucketing logic | **Missing** | No percentage-based gating |
| Rollout metadata source | **Missing** | No way to specify rollout % per release |
| Adoption tracking | **Missing** | No telemetry for update adoption |

### Design

**Architecture choice: Client-side probabilistic bucketing** (simplest, no server dependency).

#### 1. Installation ID

Add to `UpdateConfig` in `crates/oneshim-core/src/config/sections/storage.rs`:

```rust
pub struct UpdateConfig {
    // ... existing fields ...
    #[serde(default)]
    pub installation_id: Option<String>,
}
```

Generate on first startup if missing (in startup initialization code).

#### 2. Rollout Metadata via GitHub Release

Parse `<!-- rollout:N -->` from `release.body`:

```rust
/// Parse rollout percentage from GitHub release body.
/// Returns 100 if no rollout tag found or parsing fails (full rollout).
fn parse_rollout_percent(body: &Option<String>) -> u8 {
    let Some(body) = body else { return 100; };
    // Simple substring search — no regex dependency needed
    if let Some(start) = body.find("<!-- rollout:") {
        let after = &body[start + 13..]; // skip "<!-- rollout:"
        if let Some(end) = after.find("-->") {
            if let Ok(percent) = after[..end].trim().parse::<u8>() {
                return percent.min(100);
            }
        }
    }
    100 // Default: full rollout
}
```

No regex dependency needed — simple string parsing with explicit fallback to 100%.

#### 3. Stable Bucketing Logic

**Critical**: `std::collections::hash_map::DefaultHasher` is NOT deterministic across Rust versions. Use FNV-1a hash which is stable:

```rust
/// Deterministic FNV-1a hash for rollout bucketing.
/// Same installation_id + version always produces the same bucket.
fn fnv1a_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV offset basis
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3); // FNV prime
    }
    hash
}

fn is_eligible_for_rollout(
    installation_id: &str,
    version: &str,
    rollout_percent: u8,
) -> bool {
    if rollout_percent >= 100 { return true; }
    if rollout_percent == 0 { return false; }
    let mut data = installation_id.as_bytes().to_vec();
    data.extend_from_slice(version.as_bytes());
    let hash = fnv1a_hash(&data);
    (hash % 100) < rollout_percent as u64
}
```

FNV-1a is a well-defined algorithm — same input always produces same output regardless of Rust version. No external crate needed.

#### 4. Integration into Check Flow

In `check_for_updates()`, after finding a new version:

```rust
let rollout_percent = parse_rollout_percent(&release.body);
if let Some(ref id) = config.installation_id {
    if !is_eligible_for_rollout(id, &latest_version, rollout_percent) {
        debug!("Update v{latest_version} available but not in rollout bucket ({rollout_percent}%)");
        return Ok(UpdateCheckResult::NoUpdate);
    }
}
```

### Scope Boundaries

- **In scope**: Client-side bucketing with stable FNV-1a hash, installation ID auto-generation, release body parsing
- **Out of scope**: Server-side rollout control, adoption dashboards, pause/resume, CI validation of rollout tag
- **Constraint**: Hash must be deterministic across Rust versions (no DefaultHasher)
- **Fallback**: Missing rollout tag = 100% rollout. Missing installation_id = always eligible.

### Affected Files

| File | Changes |
|------|---------|
| `crates/oneshim-core/src/config/sections/storage.rs` | Add `installation_id` to UpdateConfig + Default impl |
| `src-tauri/src/updater/mod.rs` | `parse_rollout_percent()`, `fnv1a_hash()`, `is_eligible_for_rollout()`, integration in check flow |
| Startup code (main.rs or config init) | Auto-generate installation_id on first run |

### Estimated Tests: ~5

---

## Implementation Order

1. **U1 first** (simplest, user-visible, ~4 tests)
2. **U2 second** (UX improvement, builds on U1 contract changes, ~8 tests)
3. **U4 third** (independent, simple bucketing, ~5 tests)
4. **U3 last** (most complex, CI/CD changes, ~6 tests)

## Estimated Impact

- **New files**: 1 (`updater/delta.rs`)
- **Modified files**: ~12
- **New tests**: ~23
- **New dependency**: `bsdiff 0.2` (pure Rust)
- **Lines added**: ~600-800
- **Lines modified**: ~150-200

---

## Review History

### R1 (2026-04-06): 3 CRITICAL + 2 IMPORTANT issues found and resolved

| Issue | Resolution |
|-------|-----------|
| **CRITICAL**: AppHandle unavailable in coordinator for notifications | Redesigned: use existing event bridge (`runtime_bridges.rs`) which already has AppHandle |
| **CRITICAL**: Current binary path not addressed for delta updates | Added `current_binary_path()` via `std::env::current_exe()` |
| **CRITICAL**: DefaultHasher not deterministic across Rust versions | Replaced with inline FNV-1a hash (stable algorithm, no external crate) |
| **IMPORTANT**: UpdatePhase expansion coverage underspecified | Listed all 4 files that match on UpdatePhase |
| **IMPORTANT**: Release body rollout parsing fragile | Simplified to substring search with explicit 100% fallback, no regex |
