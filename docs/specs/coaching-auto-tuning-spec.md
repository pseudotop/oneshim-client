# Coaching Auto-Tuning Spec

**Date**: 2026-04-04
**Branch**: `feat/analysis-wiring`
**Scope**: `oneshim-analysis` (coaching_engine, feedback_tracker)

## 1. Problem Statement

8 coaching parameters are hardcoded constants. They should adapt to user behavior:

| Parameter | Current | Controls |
|-----------|---------|----------|
| `EXPLICIT_WEIGHT` | 3.0 | How much thumbs-up/down weighs vs implicit |
| `IMPLICIT_WINDOW_SECS` | 300 | Feedback evaluation window |
| `LOW_EFFECTIVENESS_THRESHOLD` | 0.2 | Below this → gate coaching |
| `MIN_SHOWN_FOR_GATING` | 5 | Min data before gating kicks in |
| `OVERSTAY_RATIO` | 1.2 | Trigger when duration > ratio × average |
| `EMA_ALPHA` | 0.2 | New data weight in dwell time EMA |
| `DEFAULT_AVG_REGIME_SECS` | 1800 | Fallback avg dwell time |
| Gating pattern | 1-in-3 | Allow rate when effectiveness low |

## 2. Goals

1. **Extract hardcoded constants into a `TunableParams` struct** passed to coaching engine
2. **Add lightweight EMA-based auto-tuner** that adjusts params based on accumulated feedback
3. **Zero new dependencies** — pure Rust, no ML framework needed

### Non-Goals

- ML model training/inference
- ONNX integration for coaching
- New storage tables (reuse existing `coaching_effectiveness`)
- Config file changes (tuned params are runtime-only, reset on restart)

## 3. Design

### 3.1 TunableParams Struct

```rust
pub struct TunableParams {
    pub explicit_weight: f32,              // default: 3.0, range: [1.0, 10.0]
    pub implicit_window_secs: i64,         // default: 300, range: [60, 900]
    pub low_effectiveness_threshold: f32,  // default: 0.2, range: [0.05, 0.5]
    pub min_shown_for_gating: u32,         // default: 5, range: [3, 20]
    pub overstay_ratio: f32,               // default: 1.2, range: [1.05, 2.0]
    pub ema_alpha: f32,                    // default: 0.2, range: [0.05, 0.5]
    pub gate_allow_ratio: f32,             // default: 0.33, range: [0.1, 0.5]
}
```

Each param has a safe range. The auto-tuner cannot push values outside these bounds.

### 3.2 Auto-Tuning Strategy: Feedback-Driven EMA Adjustment

**Concept**: After each feedback event, nudge the relevant parameter toward a better value.

**Algorithm per parameter**:
```
on feedback(positive: bool):
    if positive:
        // current settings worked → reinforce (nudge toward current)
        param = param  // no change needed, settings are good
    else:
        // current settings didn't work → adjust
        adjustment = param * TUNE_STEP  // 5% step
        param = param ± adjustment      // direction depends on parameter semantics
    
    param = clamp(param, min, max)
```

**Specific adjustments on negative feedback**:
- `effectiveness_threshold` → increase by 5% (be more selective)
- `overstay_ratio` → increase by 5% (trigger later, less annoying)
- `explicit_weight` → no auto-tune (user preference, keep at 3.0)
- `gate_allow_ratio` → decrease by 5% (show fewer when low effectiveness)
- `implicit_window_secs` → increase by 5% (give user more time to react)

**On positive feedback**: No adjustment (current values are working).

**Decay**: Every 100 feedback events, nudge all params 2% toward defaults (prevents runaway drift).

### 3.3 Integration

`CoachingEngine` already holds mutable state. Add `TunableParams` as a field:

```rust
pub struct CoachingEngine {
    // existing fields...
    params: TunableParams,
}
```

Replace const references in:
- `feedback_tracker.rs` → use `params.explicit_weight`, `params.implicit_window_secs`, etc.
- `triggers.rs` → use `params.overstay_ratio`, `params.ema_alpha`
- `guards.rs` → use `params.low_effectiveness_threshold`, `params.gate_allow_ratio`

The `FeedbackTracker` needs access to `TunableParams` (pass by reference or store Arc).

### 3.4 Tuning Entry Point

In `FeedbackTracker::record_feedback()` and `FeedbackTracker::evaluate_implicit()`, after recording feedback:

```rust
self.params.adjust_on_feedback(positive);
```

This is called naturally during the existing feedback flow — no new scheduler loops needed.

## 4. Files Changed

| File | Change | Lines (~) |
|------|--------|-----------|
| `crates/oneshim-analysis/src/coaching_engine/tunable_params.rs` | **NEW** — TunableParams struct + adjust logic | +80 |
| `crates/oneshim-analysis/src/coaching_engine/mod.rs` | Hold TunableParams, pass to subsystems | +10 |
| `crates/oneshim-analysis/src/feedback_tracker.rs` | Replace consts with params references | +15, -8 |
| `crates/oneshim-analysis/src/coaching_engine/triggers.rs` | Replace overstay/EMA consts | +5, -3 |
| `crates/oneshim-analysis/src/coaching_engine/guards.rs` | Replace gating consts | +5, -3 |

**Estimated total**: ~115 lines new + ~30 lines modified

## 5. Test Strategy

| Test | Type |
|------|------|
| TunableParams defaults match current hardcoded values | unit |
| Negative feedback increases overstay_ratio | unit |
| Params stay within safe ranges after repeated adjustments | unit |
| Decay nudges toward defaults | unit |
| Existing coaching behavior unchanged with default params | regression |

## 6. Key Constraint: Lightweight

- **No new dependencies**
- **No storage changes** — params are volatile (reset on restart)
- **No new scheduler loops** — tuning piggybacks on existing feedback flow
- **O(1) per adjustment** — single multiply + clamp per param
