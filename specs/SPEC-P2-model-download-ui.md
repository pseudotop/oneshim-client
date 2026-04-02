# SPEC: P2 — Whisper Model Download Manager UI

> **Status**: v3 (2nd review pass)
> **Depends on**: P1 Audio STT (merged — PR #283)
> **Scope**: `oneshim-core`, `oneshim-audio`, `src-tauri`, `oneshim-web/frontend`

---

## 1. Problem Statement

P1 added Push-to-Talk STT with local Whisper, but the model must be downloaded manually via `scripts/download-whisper-model.sh`. Users have no way to:
- See which model is installed or its download status
- Select a different model size (tiny/base/small/medium)
- Download a model from the UI
- Enable/disable audio from Settings

## 2. Goals

| # | Goal | Measure |
|---|------|---------|
| G1 | Model selection in Settings UI | User can choose tiny/base/small/medium from a dropdown |
| G2 | One-click model download | User clicks "Download" → progress bar → ready |
| G3 | Audio enable/disable toggle | User can toggle audio.enabled from Settings |
| G4 | Model status visibility | User sees: not installed / downloading / ready / error |
| G5 | Chat mic button reflects model state | Mic disabled with tooltip when model not available |

### Non-Goals (Phase 3+)
- Cloud STT fallback (OpenAI Whisper API)
- Multiple model management (only one active model at a time)
- Custom model import from local files
- Automatic model updates
- Download resume (re-download on failure for now)

## 3. Architecture

```
                   oneshim-core (ports + models)
                  ╱              |              ╲
        oneshim-audio      src-tauri         oneshim-web/frontend
     (ModelDownloader)   (IPC + wiring)        (AudioTab + Chat)

Dependency direction: adapters → core (never reverse)
oneshim-audio depends on oneshim-core (port traits + models)
src-tauri depends on all (DI wiring)
```

Key design decisions:
- **ModelDownloader port** in `oneshim-core/ports/` — implementation in `oneshim-audio`
- **Progress reporting** via `mpsc` channel (not callback) — decouples adapter from Tauri
- **Download concurrency guard** via `AtomicBool` on `AudioContext`
- **Download cancellation** via `Arc<AtomicBool>` (no new deps — already available)
- **STT engine hot-reload** via `RwLock` on `stt_engine` field
- **Model directory**: `app_data_dir()/models/` (writable, not `resource_dir`)
- **reqwest** in `oneshim-audio` behind `download` feature flag

## 4. Data Model

### 4.1 New Types (oneshim-core/config/enums.rs)

```rust
/// Available Whisper model variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WhisperModelSize {
    Tiny,    // ~75MB,  least accurate, fastest
    #[default]
    Base,    // ~142MB, good balance
    Small,   // ~466MB, better accuracy
    Medium,  // ~1.5GB, best accuracy (CPU-heavy)
}
```

### 4.2 New Types (oneshim-core/models/audio.rs)

```rust
/// Download progress event sent via channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// 0–100, None when Content-Length unknown (indeterminate).
    pub progress_pct: Option<u8>,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

/// Model download/install status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum ModelDownloadStatus {
    NotInstalled,
    Downloading { progress_pct: Option<u8>, bytes_downloaded: u64, total_bytes: Option<u64> },
    Ready { path: String, size_bytes: u64 },
    Error { message: String },
}

/// Combined audio subsystem status for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStatus {
    pub enabled: bool,
    pub selected_model: WhisperModelSize,
    pub model_status: ModelDownloadStatus,
    pub stt_provider_loaded: bool,
}
```

### 4.3 Config Changes (oneshim-core/config/sections/audio.rs)

Add `model_size` field to `AudioConfig`:

```rust
pub struct AudioConfig {
    pub enabled: bool,
    pub whisper_model_path: String,
    pub language: SttLanguage,
    pub max_recording_secs: u32,
    #[serde(default)]
    pub model_size: WhisperModelSize,  // NEW
}
```

Update the **manual** `Default` impl (not derived):
```rust
impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            whisper_model_path: String::new(),
            language: SttLanguage::Auto,
            max_recording_secs: default_max_recording_secs(),
            model_size: WhisperModelSize::default(),  // NEW — Base
        }
    }
}
```

Note: `#[serde(default)]` ensures backward compatibility with existing config files that lack this field.

### 4.4 Port Trait (oneshim-core/ports/model_downloader.rs)

```rust
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;

#[async_trait]
pub trait ModelDownloader: Send + Sync {
    /// Start downloading a Whisper model. Sends progress to `progress_tx`.
    /// Checks `cancelled` between chunks — cleans up `.part` file on cancellation.
    async fn download(
        &self,
        model: WhisperModelSize,
        dest_dir: &Path,
        progress_tx: mpsc::UnboundedSender<DownloadProgress>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<PathBuf, CoreError>;

    /// Check if a model file exists and return its status. Fast (file metadata only).
    fn model_status(&self, model: WhisperModelSize, dest_dir: &Path) -> ModelDownloadStatus;

    /// Delete a downloaded model file.
    fn delete_model(&self, model: WhisperModelSize, dest_dir: &Path) -> Result<(), CoreError>;
}
```

Design notes:
- `model_status()` and `delete_model()` are intentionally sync — single `fs::metadata`/`fs::remove_file` calls
- `download()` is async with cancellation support and channel-based progress
- The `progress_tx` channel decouples the adapter from Tauri event emission

## 5. oneshim-audio Changes

### 5.1 New: model_downloader.rs (behind `download` feature)

```rust
pub struct WhisperModelDownloader {
    client: reqwest::Client,
}
```

Feature gate in `crates/oneshim-audio/Cargo.toml`:
```toml
[features]
default = []
whisper = ["dep:whisper-rs"]
download = ["dep:reqwest", "dep:sha2", "dep:futures-util"]
```

Key design:
- `reqwest` streaming download with progress sent per chunk (~8KB)
- Downloads to `{dest_dir}/ggml-{size}.bin.part` then renames on completion (atomic)
- SHA-256 verification after download — on mismatch, warn + keep file (upstream may update models)
- `model_status()` checks file existence + size (fast) — no hash on every check
- Cancellation: checks `cancel.is_cancelled()` between chunks, cleans up `.part` file

### 5.2 Model URLs and Sizes

```rust
const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

fn model_filename(size: WhisperModelSize) -> &'static str {
    match size {
        Tiny => "ggml-tiny.bin",
        Base => "ggml-base.bin",
        Small => "ggml-small.bin",
        Medium => "ggml-medium.bin",
    }
}

fn model_expected_bytes(size: WhisperModelSize) -> u64 {
    match size {
        Tiny => 77_691_713,
        Base => 147_951_465,
        Small => 487_601_967,
        Medium => 1_533_774_781,
    }
}
```

SHA-256 hashes: computed at implementation time and hardcoded. On mismatch after download, log a warning and mark as `Ready` (not `Error`) — upstream model files may be updated without notice.

## 6. Integration

### 6.1 AppState Changes (src-tauri/runtime_state.rs)

```rust
pub struct AudioContext {
    pub capture: Option<Arc<dyn AudioCapturePort>>,
    /// RwLock allows hot-reload after model download.
    pub stt_engine: Arc<tokio::sync::RwLock<Option<Arc<dyn SttProvider>>>>,
    pub model_downloader: Option<Arc<dyn ModelDownloader>>,
    pub model_dir: PathBuf,
    /// Prevents concurrent downloads.
    pub downloading: Arc<AtomicBool>,
    /// Cancel flag for active download — set to true to abort.
    pub download_cancel: Arc<AtomicBool>,
}
```

**Breaking P1 change**: `stt_engine` moves from `Option<Arc<dyn SttProvider>>` to `Arc<RwLock<Option<Arc<dyn SttProvider>>>>`. All existing reads (`stop_and_transcribe`) must acquire a read lock. This enables `reload_stt_engine` to swap the provider at runtime.

### 6.2 IPC Commands (src-tauri/commands/audio.rs)

Add 5 new commands (4 new + modify existing):

```rust
#[command]
pub async fn get_audio_status(state: State<'_, AppState>) -> Result<AudioStatus, String>

#[command]
pub async fn download_whisper_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_size: WhisperModelSize,
) -> Result<(), String>
// Guard: check downloading AtomicBool, reject if active
// Spawns download task, bridges mpsc → app.emit("audio-model-progress", DownloadProgress)
// On complete: emit "audio-model-complete" { path, model_size, size_bytes }
// On error: emit "audio-model-error" { message }

#[command]
pub async fn cancel_model_download(state: State<'_, AppState>) -> Result<(), String>
// Triggers cancellation token

#[command]
pub async fn delete_whisper_model(
    state: State<'_, AppState>,
    model_size: WhisperModelSize,
) -> Result<(), String>

#[command]
pub async fn reload_stt_engine(state: State<'_, AppState>) -> Result<bool, String>
// Acquires write lock on stt_engine, creates new WhisperSttProvider, swaps
```

**Modify existing** `stop_and_transcribe`: acquire read lock on `stt_engine`.

### 6.3 Settings Allowlist

Add `"audio"` to `ALLOWED_KEYS` in `src-tauri/src/commands/settings.rs`.
Also update the `allowed_keys_matches_expected_set` test to include `"audio"` in the expected set.

### 6.4 Frontend — New AudioTab in Settings

New file: `crates/oneshim-web/frontend/src/pages/setting-tabs/AudioTab.tsx`

**UX separation**: Settings fields (enabled, model_size, language) follow the existing form-save pattern via `updateSettings()`. Action buttons (Download, Cancel, Delete) are standalone IPC operations — immediate side effects, not affected by Save/Revert.

Sections:
1. **Enable Audio** — toggle for `audio.enabled` (form field)
2. **Model Selection** — dropdown (tiny/base/small/medium) with size labels (form field)
3. **Model Status** — badge: Not Installed / Downloading (%) / Ready / Error
4. **Download / Cancel Button** — "Download" when idle, "Cancel" when downloading
5. **Delete Button** — removes downloaded model (confirm dialog)
6. **Language** — dropdown (Auto/English/Korean) (form field)

Update `settings-utils.ts`: add `'audio'` to `SettingsTabId` type union.

### 6.5 Frontend — Chat.tsx Mic Button Enhancement

Show context-aware tooltip:
- `"Audio disabled in Settings"` → when `!enabled`
- `"Download model in Settings"` → when `enabled` but model not ready
- `"Hold to speak"` → when ready

Query `get_audio_status` on mount + listen for model status events.

### 6.6 Frontend Event Payloads

```typescript
// audio-model-progress
{ progress_pct: number | null, bytes_downloaded: number, total_bytes: number | null }

// audio-model-complete
{ path: string, model_size: string, size_bytes: number }

// audio-model-error
{ message: string }
```

## 7. File Change Summary

| File | Change |
|------|--------|
| `crates/oneshim-core/src/config/enums.rs` | Add WhisperModelSize enum |
| `crates/oneshim-core/src/models/audio.rs` | Add DownloadProgress, ModelDownloadStatus, AudioStatus |
| `crates/oneshim-core/src/ports/model_downloader.rs` | **NEW** — ModelDownloader port trait |
| `crates/oneshim-core/src/ports/mod.rs` | Add `pub mod model_downloader;` |
| `crates/oneshim-core/src/config/sections/audio.rs` | Add `model_size` field with serde default |
| `crates/oneshim-audio/Cargo.toml` | Add `reqwest`, `sha2`, `futures-util` (feature-gated: `download`) |
| `crates/oneshim-audio/src/lib.rs` | Add `model_downloader` module (cfg download) |
| `crates/oneshim-audio/src/model_downloader.rs` | **NEW** — WhisperModelDownloader impl |
| `src-tauri/Cargo.toml` | Add `download` feature forwarding |
| `src-tauri/src/runtime_state.rs` | Refactor AudioContext (RwLock stt_engine, add fields) |
| `src-tauri/src/commands/audio.rs` | Add 5 commands, modify stop_and_transcribe |
| `src-tauri/src/commands/settings.rs` | Add "audio" to ALLOWED_KEYS |
| `src-tauri/src/main.rs` | Register new commands |
| `src-tauri/src/app_runtime_launch.rs` | Wire ModelDownloader + model_dir + RwLock |
| `crates/oneshim-web/frontend/src/pages/Settings.tsx` | Add AudioTab import + tab entry |
| `crates/oneshim-web/frontend/src/pages/setting-tabs/AudioTab.tsx` | **NEW** — Audio settings tab |
| `crates/oneshim-web/frontend/src/utils/settings-utils.ts` | Add 'audio' to SettingsTabId |
| `crates/oneshim-web/frontend/src/pages/Chat.tsx` | Enhanced mic tooltip + audio status query |

## 8. Testing Strategy

| Layer | Tests | Count |
|-------|-------|-------|
| WhisperModelSize | Serde round-trip, Default | 2 |
| DownloadProgress | Construction, Option<u8> None case | 1 |
| ModelDownloadStatus | Serde tagged enum round-trip (all variants) | 3 |
| AudioStatus | Construction + serde | 1 |
| AudioConfig | model_size default backward compat | 1 |
| WhisperModelDownloader | model_status for existing/missing files | 2 |
| WhisperModelDownloader | model_filename + expected_bytes mapping | 1 |
| IPC commands | get_audio_status, concurrent download guard | 2 |
| Frontend | AudioTab render, toggle, download button | 3 |
| **Total** | | ~16 |

Note: `download()` integration test deferred — would need `wiremock` or mock HTTP. Unit tests focus on `model_status()`, `delete_model()`, and the mapping functions. IPC tests use manual mock of `ModelDownloader` trait (ADR-001 §5).

## 9. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Large download (up to 1.5GB) | Progress bar + cancel button + disk space check |
| Network failure mid-download | `.part` file cleaned up; user re-downloads |
| Disk space insufficient | Check available space before download; show error toast |
| Model file corruption | SHA-256 hash verification (warn-only, not blocking) |
| Hot-reload STT while transcribing | RwLock — reload acquires write lock, waits for active transcription |
| Concurrent download clicks | AtomicBool guard + cancel token for active download |
| HuggingFace rate limiting | No auth needed for public models; single retry with backoff |
| `resource_dir` is read-only (signed apps) | Use `app_data_dir()/models/` instead |
| Content-Length missing from server | `progress_pct: Option<u8>` — frontend shows indeterminate spinner |
