# SPEC: P3 — Cloud STT Fallback (OpenAI Whisper API)

> **Status**: v2 (post-review)
> **Depends on**: P1 Audio STT (PR #283), P2 Model Download UI (PR #284)
> **Scope**: `oneshim-core`, `oneshim-audio`, `src-tauri`, `oneshim-web/frontend`

---

## 1. Problem Statement

P1+P2 added local Whisper STT, but it requires downloading a model (75MB–1.5GB) and runs on CPU (5-10s latency for base model). Users with an OpenAI API key (BYOK) should be able to use the cloud Whisper API as a faster alternative without local model overhead.

## 2. Goals

| # | Goal | Measure |
|---|------|---------|
| G1 | Cloud STT via OpenAI Whisper API | User enters API key, audio transcribed via cloud |
| G2 | Provider selection in Settings | User picks Local or Cloud STT provider |
| G3 | Automatic fallback | If cloud fails, fall back to local (if available) |
| G4 | Seamless PTT UX | Same mic button UX regardless of provider |
| G5 | API key management via existing BYOK infra | Reuse existing `AiProviderSettings` key storage |

### Non-Goals (Phase 4+)
- Other cloud STT providers (Google, Azure, Deepgram)
- Streaming transcription (send audio as it's recorded)
- Per-request provider selection (global setting only)
- Token usage tracking for STT

## 3. Architecture

```
                   oneshim-core (SttProvider port — unchanged)
                  ╱              |              ╲
        oneshim-audio      src-tauri         oneshim-web/frontend
     WhisperSttProvider  (wiring + fallback)   (AudioTab provider picker)
     CloudSttProvider    (IPC commands)

Provider selection: AudioConfig.stt_provider → Local | Cloud
Fallback chain: Cloud → Local (if both available)
```

Key design decisions:
- **CloudSttProvider** implements the existing `SttProvider` trait — no new port needed
- **Placed in `oneshim-audio`** behind a new `cloud-stt` feature flag (needs `reqwest`)
- **Fallback logic** in `src-tauri` command layer — wraps both providers
- **API key** stored in `AudioConfig.cloud_api_key` (simple String field, BYOK)
- **Multipart form upload** to OpenAI `/v1/audio/transcriptions` endpoint — requires `reqwest` with `multipart` feature (verify workspace dep has this)
- **Audio format**: Convert `AudioBuffer` (f32 PCM 16kHz) → WAV in-memory → upload
- **Cloud timeout**: Configurable `cloud_timeout_secs` (default 10s) — timeout returns error immediately, does NOT trigger fallback. Only connection/auth/5xx errors trigger fallback.
- **REDACTED_PATHS** is `#[cfg(test)]` only in current code — no production redaction. API key stored in plaintext config (known limitation, consistent with existing BYOK)

## 4. Data Model

### 4.1 New Enum (oneshim-core/config/enums.rs)

```rust
/// STT provider selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SttProviderKind {
    #[default]
    Local,
    Cloud,
}
```

### 4.2 Config Changes (oneshim-core/config/sections/audio.rs)

Add fields to `AudioConfig`:

```rust
pub struct AudioConfig {
    // ... existing fields ...
    #[serde(default)]
    pub stt_provider: SttProviderKind,     // NEW — Local or Cloud
    #[serde(default)]
    pub cloud_api_key: String,             // NEW — OpenAI API key (BYOK)
    #[serde(default = "default_cloud_stt_endpoint")]
    pub cloud_stt_endpoint: String,        // NEW — defaults to OpenAI
    #[serde(default = "default_cloud_timeout_secs")]
    pub cloud_timeout_secs: u32,           // NEW — default 10
}

fn default_cloud_stt_endpoint() -> String {
    "https://api.openai.com/v1/audio/transcriptions".into()
}

fn default_cloud_timeout_secs() -> u32 {
    10
}
```

Update `Default` impl to include new fields.
Note: `#[serde(default)]` on all new fields for backward compatibility.

### 4.3 AudioBuffer → WAV Conversion (oneshim-core/models/audio.rs)

Add method to `AudioBuffer`:

```rust
impl AudioBuffer {
    /// Encode PCM samples as a WAV byte buffer (16-bit, 16kHz, mono).
    pub fn to_wav_bytes(&self) -> Vec<u8> { ... }
}
```

WAV is the simplest format OpenAI accepts. No external dependency — manually write the 44-byte WAV header + PCM16 data.

### 4.4 CloudSttProvider (oneshim-audio, feature `cloud-stt`)

```rust
pub struct CloudSttProvider {
    client: reqwest::Client,
    api_key: String,
    endpoint: String,
    language: SttLanguage,
    timeout_secs: u32,
}

#[async_trait]
impl SttProvider for CloudSttProvider {
    async fn transcribe(&self, audio: AudioBuffer) -> Result<TranscriptionResult, CoreError>;
    fn provider_name(&self) -> &str { "openai-whisper-cloud" }
}
```

### 4.5 FallbackSttProvider (src-tauri)

```rust
/// Tries primary provider, falls back to secondary on error.
pub struct FallbackSttProvider {
    primary: Arc<dyn SttProvider>,
    secondary: Option<Arc<dyn SttProvider>>,
}
```

Lives in `src-tauri` (binary crate) because it orchestrates two adapters — not a reusable port.

## 5. oneshim-audio Changes

### 5.1 New: cloud_stt.rs (behind `cloud-stt` feature)

Feature gate in `crates/oneshim-audio/Cargo.toml`:
```toml
[features]
cloud-stt = ["dep:reqwest"]
```

Note: `reqwest` is already an optional dep from the `download` feature. `cloud-stt` reuses it.
**Multipart requirement**: The workspace `reqwest` dep must include the `multipart` feature (reqwest 0.13). Verify the workspace Cargo.toml has `features = [..., "multipart"]` — if not, add it.

OpenAI Whisper API request:
- `POST /v1/audio/transcriptions`
- `Authorization: Bearer {api_key}`
- `Content-Type: multipart/form-data`
- Fields: `file` (WAV), `model` ("whisper-1"), `language` (optional)
- Response: `{ "text": "..." }`

### 5.2 WAV Encoding

In `oneshim-core/models/audio.rs`, `to_wav_bytes()`:
- 44-byte RIFF/WAV header
- 16-bit signed PCM data (f32 → i16 conversion: `(sample.clamp(-1.0, 1.0) * 32767.0) as i16`)
- No external crate needed

## 6. Integration

### 6.1 AppState/AudioContext Changes

`stt_engine` (already `Arc<RwLock<Option<Arc<dyn SttProvider>>>>`) is reused.
The `reload_stt_engine` command is extended to create the appropriate provider based on `stt_provider` config:
- `Local` → `WhisperSttProvider` (existing)
- `Cloud` → `CloudSttProvider` (new)
- Both available → `FallbackSttProvider` wrapping Cloud(primary) + Local(secondary)

### 6.2 IPC Command Changes

**Modify `reload_stt_engine`**: Build provider based on `config.audio.stt_provider`:
- `SttProviderKind::Local` → existing WhisperSttProvider path
- `SttProviderKind::Cloud` → CloudSttProvider with api_key + endpoint
- Fallback: if both local model exists AND cloud key provided, wrap in FallbackSttProvider

**Modify `get_audio_status`**: Add `stt_provider: SttProviderKind` to response.

### 6.3 Settings Allowlist

`"audio"` already in `ALLOWED_KEYS` (from P2). `cloud_api_key` field is sensitive — add to `REDACTED_PATHS` in settings.rs.

### 6.4 Frontend — AudioTab Enhancement

Add to existing AudioTab:
1. **STT Provider** — radio/select: "Local (Whisper)" / "Cloud (OpenAI)" (form field)
2. **API Key** — password input for `cloud_api_key` (form field, shown when Cloud selected)
3. **Endpoint** — text input for `cloud_stt_endpoint` (collapsed under "Advanced", form field)
4. **Provider Status** — badge showing which provider is active

### 6.5 Frontend — AudioStatus Enhancement

Add `stt_provider` field to:
- `AudioSettings` interface in `contracts.ts` (3 new fields: `stt_provider`, `cloud_api_key`, `cloud_stt_endpoint`, `cloud_timeout_secs`)
- `AudioStatusResponse` inline type in `AudioTab.tsx` (add `stt_provider` field)
- `get_audio_status` inline type in `Chat.tsx` (already minimal — no change needed)
- Standalone mock `standalone.ts` (add defaults for new fields)
- Stories utils `stories-utils.ts` (add defaults)

## 7. File Change Summary

| File | Change |
|------|--------|
| `crates/oneshim-core/src/config/enums.rs` | Add `SttProviderKind` enum |
| `crates/oneshim-core/src/config/sections/audio.rs` | Add `stt_provider`, `cloud_api_key`, `cloud_stt_endpoint` fields |
| `crates/oneshim-core/src/models/audio.rs` | Add `to_wav_bytes()` method + `AudioStatus.stt_provider` field |
| `crates/oneshim-audio/Cargo.toml` | Add `cloud-stt` feature |
| `crates/oneshim-audio/src/lib.rs` | Register `cloud_stt` module |
| `crates/oneshim-audio/src/cloud_stt.rs` | **NEW** — `CloudSttProvider` impl |
| `src-tauri/Cargo.toml` | Add `cloud-stt` feature forwarding |
| `src-tauri/src/commands/audio.rs` | Modify `reload_stt_engine` + `get_audio_status` |
| `src-tauri/src/commands/settings.rs` | Add `cloud_api_key` to `REDACTED_PATHS` |
| `src-tauri/src/app_runtime_launch.rs` | Wire cloud provider in initial setup |
| `src-tauri/src/fallback_stt.rs` | **NEW** — `FallbackSttProvider` |
| `crates/oneshim-web/frontend/src/api/contracts.ts` | Add `stt_provider` to `AudioSettings` + `AudioStatus` |
| `crates/oneshim-web/frontend/src/pages/setting-tabs/AudioTab.tsx` | Add provider picker + API key input + inline type update |
| `crates/oneshim-web/frontend/src/api/standalone.ts` | Add new audio field defaults |
| `crates/oneshim-web/frontend/src/pages/setting-tabs/stories-utils.ts` | Add new audio field defaults |

## 8. Testing Strategy

| Layer | Tests | Count |
|-------|-------|-------|
| SttProviderKind | Serde round-trip, Default | 2 |
| AudioConfig | New fields serde backward-compat | 1 |
| to_wav_bytes | Valid WAV header, correct PCM16 data, empty buffer | 3 |
| CloudSttProvider | provider_name, error on empty key | 2 |
| FallbackSttProvider | Primary success, primary fail → secondary, both fail | 3 |
| AudioStatus | stt_provider field serde | 1 |
| **Total** | | ~12 |

Note: `CloudSttProvider::transcribe` integration test deferred (requires real API key or mock server).

## 9. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| API key exposure in config file | Stored as plain text in config; `REDACTED_PATHS` prevents UI leaking |
| OpenAI API latency (1-3s) | Still faster than local CPU Whisper (5-10s); spinner UX already exists |
| OpenAI API cost | User provides own key (BYOK); no billing from us |
| Audio upload privacy | User explicitly opts into cloud STT; clear label in Settings |
| WAV encoding correctness | Unit tests verify header + PCM data |
| API endpoint changes | Configurable `cloud_stt_endpoint` field |
| Fallback creating double latency | `cloud_timeout_secs` (default 10s) — timeout returns error immediately, NO fallback. Only connection/auth/5xx errors trigger fallback. Max wait = cloud timeout alone. |
| API key in plaintext config | `REDACTED_PATHS` is test-only (pre-existing limitation). Key stored same as existing `ai_provider` keys. |
