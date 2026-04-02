# SPEC: P1 — Audio STT (Push-to-Talk + Local Whisper)

> **Status**: v2 (post-review)  
> **Scope**: `oneshim-core`, `oneshim-audio` (NEW), `src-tauri`, `oneshim-web/frontend`

---

## 1. Problem Statement

The Chat page only supports text input. Users cannot use voice to compose messages.

## 2. Goals

| # | Goal | Measure |
|---|------|---------|
| G1 | Push-to-Talk mic button in Chat | User holds button, speaks, text appears in input |
| G2 | Local STT (no network required) | Whisper base model, downloaded on first use |
| G3 | Cross-platform audio capture | macOS, Windows, Linux |
| G4 | Korean + English | Whisper multilingual model |

### Non-Goals (Phase 2+)
- VAD, cloud STT fallback, model download UI, streaming transcription

## 3. Architecture

```
Frontend (Chat.tsx)
  └─ Mic button (PTT) → IPC → AudioCapture::start/stop → SttProvider::transcribe → stt-result event

oneshim-core   → AudioBuffer, TranscriptionResult models + SttProvider port + AudioConfig
oneshim-audio  → AudioCapture (cpal+rubato) + WhisperSttProvider (feature-gated)
src-tauri      → AppState.audio: AudioContext + IPC commands + wiring
```

## 4. Data Model

### 4.1 Audio Types (oneshim-core/models/audio.rs)

```rust
/// Raw 16kHz mono f32 PCM audio buffer.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,      // always 16000
    pub duration_secs: f32,
}

/// STT transcription result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language: Option<String>,
    pub duration_secs: f32,
    pub processing_secs: f32,
}
```

### 4.2 Port Trait (oneshim-core/ports/stt_provider.rs)

```rust
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Transcribe audio buffer to text. Takes ownership to avoid clone.
    async fn transcribe(&self, audio: AudioBuffer) -> Result<TranscriptionResult, CoreError>;
    fn provider_name(&self) -> &str;
}
```

### 4.3 Error Variants (oneshim-core/error.rs)

```rust
#[error("Audio capture error: {0}")]
AudioCapture(String),

#[error("Speech-to-text error: {0}")]
SpeechToText(String),
```

### 4.4 Config (oneshim-core/config/sections/monitoring.rs)

Add to existing `monitoring.rs` alongside VisionConfig/MonitorConfig:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default)]
    pub enabled: bool,                    // default false (opt-in)
    #[serde(default)]
    pub whisper_model_path: String,       // empty = auto-download base model
    #[serde(default = "default_stt_language")]
    pub language: SttLanguage,            // Auto/En/Ko enum
    #[serde(default = "default_max_recording_secs")]
    pub max_recording_secs: u32,          // default 60
}
```

`SttLanguage` enum in `config/enums.rs`:
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SttLanguage { #[default] Auto, En, Ko }
```

## 5. oneshim-audio Crate

### 5.1 Cargo.toml

```toml
[package]
name = "oneshim-audio"
description = "Audio capture and speech-to-text — cpal + whisper-rs"
edition.workspace = true
version.workspace = true

[features]
default = []
whisper = ["dep:whisper-rs"]   # Feature-gated: requires cmake + C++ toolchain

[dependencies]
oneshim-core = { workspace = true }
cpal = { workspace = true }
rubato = { workspace = true }
whisper-rs = { workspace = true, optional = true }
tokio = { workspace = true }
tracing = { workspace = true }
parking_lot = { workspace = true }
```

Workspace deps (add to root `Cargo.toml [workspace.dependencies]`):
```toml
cpal = "0.17"
rubato = "1"
whisper-rs = "0.16"
```

### 5.2 AudioCapture (capture.rs)

```rust
pub struct AudioCapture {
    buffer: Arc<Mutex<Vec<f32>>>,
    capturing: Arc<AtomicBool>,
    stream: Mutex<Option<cpal::Stream>>,
}

impl AudioCapture {
    pub fn new() -> Result<Self, CoreError>;
    pub fn start(&self) -> Result<(), CoreError>;
    pub fn stop(&self) -> Result<AudioBuffer, CoreError>;
    pub fn is_capturing(&self) -> bool;
}
```

- `start()`: get default input device, create resampler (native→16kHz), start cpal stream
- `stop()`: stop stream, drain buffer, return AudioBuffer
- Resampler created per-start (not stored) since sample rate may change between devices

### 5.3 WhisperSttProvider (whisper.rs, behind `#[cfg(feature = "whisper")]`)

```rust
pub struct WhisperSttProvider {
    ctx: Mutex<WhisperContext>,
    language: SttLanguage,
    transcribing: AtomicBool,  // prevents concurrent transcriptions
}

impl WhisperSttProvider {
    pub fn new(model_path: &Path, language: SttLanguage) -> Result<Self, CoreError>;
}

#[async_trait]
impl SttProvider for WhisperSttProvider {
    async fn transcribe(&self, audio: AudioBuffer) -> Result<TranscriptionResult, CoreError> {
        if self.transcribing.swap(true, Ordering::SeqCst) {
            return Err(CoreError::SpeechToText("transcription already in progress".into()));
        }
        // ... spawn_blocking with whisper-rs ...
        // Set transcribing = false in drop guard
    }
}
```

### 5.4 Model Strategy

**No model bundled in source repo.** Model is downloaded on first use:

- `resources/ggml-base.bin` in `.gitignore`
- `scripts/download-whisper-model.sh` — fetches from huggingface
- At runtime: check `config.whisper_model_path` → fallback to `app.path().resource_dir()/ggml-base.bin`
- If model file absent → `stt_engine` is None → mic button disabled with "Download model" prompt
- CI: model downloaded as build step for release builds only
- Development: `cargo check/test` works without model (feature-gated)

## 6. Integration

### 6.1 AppState (src-tauri/runtime_state.rs)

```rust
pub struct AudioContext {
    pub capture: Option<Arc<AudioCapture>>,
    pub stt_engine: Option<Arc<dyn SttProvider>>,
}

pub struct AppState {
    // ... existing ...
    pub audio: AudioContext,
}
```

### 6.2 IPC Commands (src-tauri/commands/audio.rs)

```rust
#[command]
pub async fn start_audio_capture(state: tauri::State<'_, AppState>) -> Result<(), String>

#[command]
pub async fn stop_and_transcribe(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<TranscriptionResult, String>
```

`stop_and_transcribe`:
1. `capture.stop()` → AudioBuffer
2. `stt_engine.transcribe(buffer)` → TranscriptionResult
3. Return result (frontend handles insertion into textarea)

### 6.3 macOS Permission

Add to `src-tauri/assets/Info.plist`:
```xml
<key>NSMicrophoneUsageDescription</key>
<string>ONESHIM uses the microphone for voice-to-text input in the AI chat.</string>
```

### 6.4 Frontend (Chat.tsx)

- Mic button next to send button
- **mousedown**: `ipc('start_audio_capture')`
- **mouseup**: `ipc('stop_and_transcribe')` → insert `result.text` into textarea
- Visual: pulsing red dot while recording, "Transcribing..." spinner after release
- Disabled when `!audioEnabled` or `isReadOnly` (historical session)

## 7. File Change Summary

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add members + workspace deps (cpal, rubato, whisper-rs) |
| `crates/oneshim-audio/` | **NEW CRATE** — Cargo.toml, src/lib.rs, capture.rs, whisper.rs |
| `crates/oneshim-core/src/models/audio.rs` | **NEW** — AudioBuffer, TranscriptionResult |
| `crates/oneshim-core/src/models/mod.rs` | Add `pub mod audio;` |
| `crates/oneshim-core/src/ports/stt_provider.rs` | **NEW** — SttProvider trait |
| `crates/oneshim-core/src/ports/mod.rs` | Add `pub mod stt_provider;` |
| `crates/oneshim-core/src/error.rs` | Add AudioCapture, SpeechToText variants |
| `crates/oneshim-core/src/config/enums.rs` | Add SttLanguage enum |
| `crates/oneshim-core/src/config/sections/monitoring.rs` | Add AudioConfig struct |
| `crates/oneshim-core/src/config/mod.rs` | Add `audio: AudioConfig` to AppConfig |
| `src-tauri/Cargo.toml` | Add `oneshim-audio` dep (optional, feature-gated) |
| `src-tauri/src/runtime_state.rs` | Add AudioContext sub-struct |
| `src-tauri/src/commands/audio.rs` | **NEW** — 2 IPC commands |
| `src-tauri/src/commands/mod.rs` | Add `pub mod audio;` |
| `src-tauri/src/main.rs` | Register audio commands |
| `src-tauri/src/app_runtime_launch.rs` | Wire AudioCapture + WhisperSttProvider |
| `src-tauri/assets/Info.plist` | Add NSMicrophoneUsageDescription |
| `scripts/download-whisper-model.sh` | **NEW** — model download script |
| `crates/oneshim-web/frontend/src/pages/Chat.tsx` | Mic button + PTT logic |

## 8. Testing Strategy

| Layer | Tests | Count |
|-------|-------|-------|
| AudioBuffer | Construction, duration calc, empty buffer | 3 |
| AudioCapture | Start/stop lifecycle (mock device) | 2 |
| WhisperSttProvider | Concurrent guard, error handling | 2 |
| Config | AudioConfig serde round-trip, SttLanguage enum | 2 |
| Integration | Command registration, missing engine error | 2 |
| **Total** | | ~11 |

Hardware-dependent tests (real mic, real Whisper model) are `#[ignore]`.

## 9. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| whisper-rs build requires cmake | Feature-gated; `cargo check` works without it |
| Model download (57-142MB) | Download script + runtime detection; no git bloat |
| cpal platform differences | macOS primary; CI tests Linux |
| Whisper latency (5-10s on CPU) | Spinner UX; Metal/CUDA accelerated on supported HW |
| Mic permission denied | Graceful error + platform guidance toast |
| Concurrent PTT presses | AtomicBool guard in WhisperSttProvider |
