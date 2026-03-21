# Audio Capture + Whisper STT — Deferred Research

**Date:** 2026-03-21
**Status:** Deferred (customer demand-driven)
**Reason:** Privacy risk high, +350MB dependency budget, not core differentiator
**Trigger:** Implement when customer demand for meeting/call transcription is validated

> This document preserves deep research conducted 2026-03-21 for future use.
> The feature was originally §3 in the architecture improvements spec but was
> removed during scope review.

---

## Decision Record

**Why deferred:**

1. **Privacy risk** — Audio recording triggers significantly higher user resistance
   than screen/keyboard monitoring. Microphone + system audio capture requires
   explicit TCC permissions on macOS (screen recording dialog for audio-only).
2. **Dependency budget** — `ort` (ONNX Runtime) adds ~100MB, whisper model adds
   ~250MB. Total +350MB to binary/download size.
3. **Not a differentiator** — ONESHIM's core strengths (coaching engine, regime
   detection, 3-tier sync) don't depend on audio. Screenpipe has audio but it's
   not their differentiator either.
4. **Effort** — 12.5 days, half the total architecture improvement budget.

**When to reconsider:**
- Customer explicitly requests meeting transcription
- Competitive pressure from tools with integrated STT
- Whisper model sizes shrink significantly (<50MB)
- Platform APIs simplify (e.g., macOS adds audio-only ScreenCaptureKit permission)

---

## Research Summary

### STT Engine: whisper-rs (recommended)

| Model | Disk (Q5_1) | RAM | RTF (M4) | Meeting WER |
|---|---|---|---|---|
| tiny.en | ~40MB | ~120MB | ~0.04 | 12-15% (too low) |
| base.en | ~80MB | ~200MB | ~0.08 | 8-10% |
| **small.en** | **~250MB** | **~500MB** | **~0.18** | **5-7% (recommended)** |
| medium.en | ~750MB | ~1.5GB | ~0.55 | 4-5% |

Alternatives: `candle` (pure Rust, 2x slower), `ort` (ONNX, best on Windows),
`sherpa-onnx` (streaming Zipformer, <100ms latency).

### Audio Capture

| Platform | Microphone | System Audio |
|---|---|---|
| macOS | `cpal` (CoreAudio) | ScreenCaptureKit (macOS 13+) |
| Windows | `cpal` (WASAPI) | WASAPI loopback (native) |
| Linux | `cpal` (PulseAudio) | PulseAudio monitor source |

**Dual-stream** for meetings: mic (local) + loopback (remote) → mix → STT.

### VAD: Silero VAD (ONNX, 2MB)

ML-based, <1ms per 30ms frame, high accuracy. Uses `ort` crate.
Pipeline: Audio → Resample (16kHz) → Ring buffer → Silero VAD → Speech segments → whisper-rs.

### Port Traits (3 separate)

```rust
pub trait AudioCaptureSource: Send + Sync { ... }
pub trait VoiceActivityDetector: Send + Sync { ... }
pub trait SpeechRecognizer: Send + Sync { ... }
```

### Crate: `oneshim-stt`

```
oneshim-core  <--  oneshim-stt
                     ├── capture/ (mic + system audio + mixer)
                     ├── vad.rs (Silero VAD via ort)
                     ├── transcribe.rs (whisper-rs)
                     └── privacy.rs (PII masking)
```

### Resource Budget

| Component | Disk | RAM | CPU (active) |
|---|---|---|---|
| whisper-rs (small.en Q5_1) | 250MB | 500MB | 30-60% (1 core) |
| Silero VAD (ONNX) | 2MB | 10MB | <1% |
| **Total** | **~252MB** | **~512MB** | **30-60%** |

### Schema Impact

V18 migration: `audio_transcripts` table. FTS enrichment via `search_fts`.
`Event::Audio(AudioTranscriptEvent)` variant.

### Effort: ~12.5 days

See original spec v2 for detailed breakdown.

---

## Full Research Transcript

The complete research output (STT engines, audio capture APIs, VAD comparison,
model benchmarks, TCC permissions, meeting capture architecture) is available
in the agent output archive from the 2026-03-21 session.
