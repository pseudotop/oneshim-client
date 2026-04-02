# SPEC: P4 — Voice Activity Detection (VAD)

> **Status**: v2 (post-review)
> **Depends on**: P1 Audio STT (PR #283), P2 Model Download (PR #284), P3 Cloud STT (PR #285)
> **Scope**: `oneshim-core`, `oneshim-audio`, `src-tauri`, `oneshim-web/frontend`

---

## 1. Problem Statement

P1 added Push-to-Talk (PTT) for voice input — the user must hold a button while speaking. This is cumbersome for hands-free workflows. Users should be able to click a "Listen" toggle that automatically detects when they start and stop speaking, triggering transcription without holding a button.

## 2. Goals

| # | Goal | Measure |
|---|------|---------|
| G1 | Auto-detect speech start/end | Audio stream analyzed for voice activity in real-time |
| G2 | Toggle mode in Chat UI | User clicks mic once to start listening, clicks again to stop |
| G3 | Visual feedback during listening | Pulsing indicator while listening, solid while speech detected |
| G4 | Configurable sensitivity | Threshold setting in AudioTab (Settings) |
| G5 | Coexist with PTT | Both modes available — user picks in Settings or toggles in Chat |

### Non-Goals
- Wake word detection ("Hey Oneshim")
- Continuous dictation (transcribe indefinitely — only single utterances)
- GPU-accelerated VAD models (keep it lightweight, CPU-only)
- Speaker diarization (who is speaking)

## 3. Architecture

```
AudioCapture (cpal stream — already exists)
  └─ cpal callback: downmix → mono samples
     └─ VadDetector.process_chunk(samples) → VadEvent
        └─ SpeechStarted → start accumulating in speech_buffer
        └─ SpeechEnded → send signal via mpsc (lightweight, no buffer copy)
           └─ Receiver task (tokio): drain speech_buffer, resample to 16kHz, transcribe
```

Key design decisions:
- **Energy-based VAD** (not ML model) — RMS energy threshold. No new dependencies. Pure math.
- **VadDetector owned by cpal callback closure** — no mutex needed for VAD state. Separate `speech_buffer: Arc<Mutex<Vec<f32>>>` for accumulated speech samples.
- **Signal-based callback**: cpal callback sends `SpeechEnded` signal to mpsc channel (no AudioBuffer construction on audio thread). Receiver task drains the speech_buffer, resamples, and transcribes.
- **Resampling off audio thread**: The receiver task calls `resample()` (same function used by `stop()`). This avoids blocking the cpal callback.
- **Mutual exclusion**: `start()` fails if VAD active; `start_vad()` fails if PTT capturing. Enforced via `AtomicBool` flags.
- **Port trait**: VAD methods have default implementations returning `Err("VAD not supported")` for backward compat.
- **vad_state "transcribing"**: Tracked at IPC layer (not in VadDetector), set when speech ends and cleared when transcription completes.

## 4. Data Model

### 4.1 New Enum (oneshim-core/config/enums.rs)

```rust
/// Mic input mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MicInputMode {
    #[default]
    PushToTalk,
    VoiceActivity,
}
```

### 4.2 Config Changes (oneshim-core/config/sections/audio.rs)

Add fields to `AudioConfig`:

```rust
    #[serde(default)]
    pub mic_input_mode: MicInputMode,
    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,          // RMS energy threshold, 0.0–1.0, default 0.02
    #[serde(default = "default_vad_silence_ms")]
    pub vad_silence_ms: u32,         // Silence duration to end utterance, default 800ms
    #[serde(default = "default_vad_min_speech_ms")]
    pub vad_min_speech_ms: u32,      // Minimum speech duration to trigger transcription, default 300ms
```

### 4.3 VadConfig (oneshim-core/models/audio.rs)

```rust
/// Configuration for Voice Activity Detection.
#[derive(Debug, Clone)]
pub struct VadConfig {
    pub threshold: f32,
    pub silence_ms: u32,
    pub min_speech_ms: u32,
}
```

### 4.4 VadDetector (oneshim-audio — internal, not a port)

```rust
pub struct VadDetector {
    threshold: f32,
    silence_ms: u32,
    min_speech_ms: u32,
    state: VadState,
    speech_start: Option<Instant>,
    last_speech: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadState {
    Idle,
    Listening,
    SpeechDetected,
    SilenceAfterSpeech,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    None,
    SpeechStarted,
    SpeechContinuing,
    SpeechEnded,
}
```

Methods:
- `process_chunk(&mut self, samples: &[f32]) -> VadEvent` — called per audio callback
- RMS calculation: `sqrt(sum(s^2) / n)` — if above threshold for `min_speech_ms`, speech detected
- Silence: if below threshold for `silence_ms` after speech, speech ended
- **Owned by cpal callback closure** — no mutex needed for VadDetector itself

### 4.5 AudioStatus Enhancement

Add `mic_input_mode` and `vad_state` to `AudioStatus`:

```rust
pub struct AudioStatus {
    // ... existing fields ...
    #[serde(default)]
    pub mic_input_mode: String,  // "push_to_talk" or "voice_activity"
    #[serde(default)]
    pub vad_state: String,       // "idle", "listening", "speech", "transcribing"
}
```

Note: `vad_state` is a UI-level state tracked at the IPC layer. "transcribing" is set when speech ends and STT starts, cleared when transcription completes. It does NOT map 1:1 to `VadDetector::VadState`.

## 5. oneshim-audio Changes

### 5.1 New: vad.rs

VadDetector is a pure computation struct — no I/O, no async, no mutex.

```rust
impl VadDetector {
    pub fn new(threshold: f32, silence_ms: u32, min_speech_ms: u32) -> Self;
    pub fn process_chunk(&mut self, samples: &[f32]) -> VadEvent;
    pub fn reset(&mut self);
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}
```

### 5.2 Modify: AudioCapture

Add VAD support. New fields:

```rust
pub struct AudioCapture {
    // ... existing fields ...
    vad_active: Arc<AtomicBool>,
    speech_buffer: Arc<Mutex<Vec<f32>>>,
}
```

New methods:

```rust
impl AudioCapture {
    /// Start VAD listening mode.
    /// The `on_speech_signal` callback is invoked (on audio thread) when speech ends —
    /// it should be lightweight (send signal to channel, no buffer work).
    pub fn start_vad(
        &self,
        config: VadConfig,
        on_speech_signal: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), CoreError>;

    /// Stop VAD listening mode. Returns any accumulated speech as raw samples.
    pub fn stop_vad(&self) -> Result<(), CoreError>;

    /// Whether VAD listening is active.
    pub fn is_vad_active(&self) -> bool;

    /// Drain the speech buffer and resample to 16kHz. Called by receiver task.
    pub fn drain_speech_buffer(&self) -> Result<AudioBuffer, CoreError>;
}
```

**Mutual exclusion**: `start_vad()` checks `self.capturing` (PTT flag) and fails if true. `start()` checks `self.vad_active` and fails if true. Both use AtomicBool checks.

**cpal callback in VAD mode**:
1. Downmix to mono (same as PTT)
2. `vad.process_chunk(&mono)` — VadDetector owned by closure, no mutex
3. If SpeechStarted/SpeechContinuing → `speech_buffer.lock().extend_from_slice(&mono)`
4. If SpeechEnded → `on_speech_signal()` (just sends () to channel — no buffer work)
5. If None (silence, no speech) → do nothing

### 5.3 AudioCapturePort Enhancement

Add VAD methods with **default implementations** for backward compat:

```rust
pub trait AudioCapturePort: Send + Sync {
    fn start(&self) -> Result<(), CoreError>;
    fn stop(&self) -> Result<AudioBuffer, CoreError>;
    fn is_capturing(&self) -> bool;

    // VAD methods — default impls return "not supported"
    fn start_vad(
        &self,
        _config: VadConfig,
        _on_speech_signal: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), CoreError> {
        Err(CoreError::AudioCapture("VAD not supported".into()))
    }
    fn stop_vad(&self) -> Result<(), CoreError> {
        Err(CoreError::AudioCapture("VAD not supported".into()))
    }
    fn is_vad_active(&self) -> bool { false }
    fn drain_speech_buffer(&self) -> Result<AudioBuffer, CoreError> {
        Err(CoreError::AudioCapture("VAD not supported".into()))
    }
}
```

## 6. Integration

### 6.1 IPC Commands (src-tauri/commands/audio.rs)

New commands:

```rust
#[command]
pub async fn start_vad_listening(app: AppHandle, state: State<'_, AppState>) -> Result<(), String>

#[command]
pub async fn stop_vad_listening(state: State<'_, AppState>) -> Result<(), String>
```

**`start_vad_listening` flow**:
1. Create `mpsc::unbounded_channel()`
2. Create `on_speech_signal` closure that sends `()` to the channel
3. Call `capture.start_vad(config, on_speech_signal)`
4. Spawn tokio task: loop on channel receiver:
   - Set vad_state = "transcribing", emit `vad-state-changed`
   - Call `capture.drain_speech_buffer()` → get raw samples
   - Resample to 16kHz (reuse existing `resample()` logic)
   - Transcribe via `stt_engine`
   - Emit `vad-transcription-result` with text
   - Set vad_state = "listening", emit `vad-state-changed`

### 6.2 AudioContext Enhancement

Add VAD state tracking to `AudioContext`:

```rust
pub struct AudioContext {
    // ... existing fields ...
    pub vad_state: Arc<Mutex<String>>,  // "idle", "listening", "speech", "transcribing"
}
```

### 6.3 Frontend — Chat.tsx Enhancement

Mode-aware mic button:
- **PTT mode** (default): Same hold-to-speak behavior (unchanged)
- **VAD mode**: Click to toggle listening. Shows state via icon color:
  - idle → gray mic
  - listening → blue pulsing mic  
  - speech → red pulsing mic
  - transcribing → spinner

Listen for `vad-state-changed` and `vad-transcription-result` events.

### 6.4 Frontend — AudioTab Enhancement

Add to Settings AudioTab:
1. **Input Mode** — radio: "Push-to-Talk" / "Voice Activity" (form field)
2. **VAD Sensitivity** — range slider for `vad_threshold` (shown when VAD selected)
3. **Silence Duration** — number input for `vad_silence_ms` (ms)

### 6.5 Frontend Events

```typescript
// vad-state-changed
{ state: "idle" | "listening" | "speech" | "transcribing" }

// vad-transcription-result  
{ text: string, duration_secs: number, processing_secs: number }
```

## 7. File Change Summary

| File | Change |
|------|--------|
| `crates/oneshim-core/src/config/enums.rs` | Add `MicInputMode` enum |
| `crates/oneshim-core/src/config/sections/audio.rs` | Add `mic_input_mode`, `vad_threshold`, `vad_silence_ms`, `vad_min_speech_ms` |
| `crates/oneshim-core/src/models/audio.rs` | Add `VadConfig`, `mic_input_mode`/`vad_state` to `AudioStatus` |
| `crates/oneshim-core/src/ports/audio_capture.rs` | Add VAD methods with default impls |
| `crates/oneshim-audio/src/lib.rs` | Register `vad` module |
| `crates/oneshim-audio/src/vad.rs` | **NEW** — `VadDetector` + energy-based VAD |
| `crates/oneshim-audio/src/capture.rs` | Add `start_vad()`, `stop_vad()`, `is_vad_active()`, `drain_speech_buffer()`, mutual exclusion |
| `src-tauri/src/runtime_state.rs` | Add `vad_state` to `AudioContext` |
| `src-tauri/src/commands/audio.rs` | Add 2 IPC commands, modify `get_audio_status` |
| `src-tauri/src/main.rs` | Register new commands |
| `crates/oneshim-web/frontend/src/api/contracts.ts` | Add new fields to `AudioSettings` |
| `crates/oneshim-web/frontend/src/api/standalone.ts` | Add defaults |
| `crates/oneshim-web/frontend/src/pages/setting-tabs/stories-utils.ts` | Add defaults |
| `crates/oneshim-web/frontend/src/pages/setting-tabs/AudioTab.tsx` | Add input mode picker + VAD settings |
| `crates/oneshim-web/frontend/src/pages/Chat.tsx` | Mode-aware mic button + VAD event listeners |

## 8. Testing Strategy

| Layer | Tests | Count |
|-------|-------|-------|
| MicInputMode | Serde round-trip, Default | 2 |
| AudioConfig | New VAD fields backward-compat | 1 |
| VadDetector | Silent input → None event | 1 |
| VadDetector | Loud input → SpeechStarted | 1 |
| VadDetector | Speech then silence → SpeechEnded | 1 |
| VadDetector | Short speech (<min_speech_ms) → no SpeechEnded | 1 |
| VadDetector | RMS calculation correctness | 1 |
| VadDetector | reset() clears state | 1 |
| AudioCapture | start_vad fails if PTT capturing | 1 |
| AudioCapture | start fails if VAD active | 1 |
| **Total** | | ~11 |

## 9. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Energy-based VAD not accurate enough | Tunable threshold + sensitivity slider; can upgrade to ML later |
| Background noise triggers false positives | Configurable threshold; higher = less sensitive |
| Audio callback too heavy with VAD | RMS is O(n) single-pass; VadDetector owned by closure (no mutex) |
| VAD + PTT mode confusion | Clear UI: radio toggle, different button behavior per mode |
| Buffer drain on audio thread | Signal-only callback; drain + resample on receiver tokio task |
| Silence duration too short/long | Configurable `vad_silence_ms` with sensible default (800ms) |
| PTT/VAD concurrent activation | Mutual exclusion via AtomicBool flags |
| Resampling missing for VAD path | Explicit: receiver task calls drain_speech_buffer() + resample() |
