# Regime Detection Strengthening Spec

**Date**: 2026-04-04
**Branch**: `feat/analysis-wiring`
**Scope**: `src-tauri` (scheduler/analysis_pipeline)

## 1. Problem Statement

The regime detection system is 90% wired. The remaining gaps:

| Component | Current State | Gap |
|-----------|--------------|-----|
| `run_periodic_regime_detection()` | Runs in analysis pipeline | 24-hour interval too long; no minimum sample threshold |
| `RegimeAnalysisFacade` | **Already uses HDBSCAN** (`ClusteringAlgorithm::Hdbscan` is `#[default]`, feature ON) | No gap ✅ |
| Drift → recluster | **Already wired** (`analysis_pipeline/mod.rs:294-299`) | No gap — drift sets `recluster_requested` ✅ |
| `recluster_requested` consumption | **Already consumed** (`regime.rs:20-22`) | No gap — atomically read-and-cleared ✅ |
| IPC trigger_recluster | **Already wired** (`commands/dashboard.rs:218-226`) | No gap ✅ |

**What the user sees**: Regimes re-learn every 24 hours (too slow). HDBSCAN code exists but k-means is always used. No quality gate on detection (runs even with 3 samples).

## 2. Goals

1. **Reduce detection interval**: 24h → 2h (configurable), so regimes adapt faster to behavior shifts
2. **Add minimum sample threshold**: Skip detection if calibration data has < 50 feature vectors

Note: HDBSCAN is already activated — `ClusteringAlgorithm::Hdbscan` is `#[default]` and the `hdbscan` Cargo feature is ON by default.

### Non-Goals

- New scheduler loop (existing pipeline sufficient)
- Drift detection wiring (already done)
- `recluster_requested` wiring (already done)
- HNSW changes (already wired in search coordinator)
- Storage schema changes

## 3. Design

### 3.1 Reduce Detection Interval

**Current** (`analysis_pipeline/regime.rs:24-28`):
```rust
let elapsed = now - last;
if elapsed.num_hours() >= 24 { should_detect = true; }
```

**Change**: Replace hardcoded 24h with configurable interval:
```rust
let interval_hours = config.analysis.regime_detection_interval_hours; // default: 2
if elapsed.num_hours() >= interval_hours { should_detect = true; }
```

**Config**: Add `regime_detection_interval_hours: u64` to `AnalysisConfig` with default `2`.

### 3.2 Minimum Sample Threshold

**Current**: Detection runs with any number of features (even 3).

**Change** (`analysis_pipeline/regime.rs`, after feature extraction):
```rust
if features.len() < 50 {
    debug!(count = features.len(), "regime detection skipped — insufficient samples");
    return;
}
```

This prevents poor-quality clustering from noisy small datasets. The threshold matches `RegimeDetector`'s internal minimum (50 samples).

### 3.3 Integration Points

```
run_periodic_regime_detection() [EXISTING]
  │
  ├─ recluster_requested.swap(false)     [EXISTING: manual/drift trigger]
  ├─ elapsed >= 2h? (was 24h)            [CHANGED: configurable interval]
  │
  ├─ Load 7-day calibration data         [EXISTING]
  ├─ Build feature vectors               [EXISTING]
  │
  ├─ features.len() < 50? → skip         [NEW: quality gate]
  │
  ├─ regime_analysis (facade) present?
  │   ├─ YES → recluster_with_constraints() [EXISTING: uses HDBSCAN if activated]
  │   └─ NO → regime_detector.detect()      [EXISTING: legacy k-means]
  │
  ├─ regime_manager.update_from_detection() [EXISTING]
  ├─ regime_classifier.update_regimes()     [EXISTING]
  └─ drift_detector.reset()                 [EXISTING]
```

## 4. Files Changed

| File | Change | Lines (~) |
|------|--------|-----------|
| `src-tauri/src/scheduler/analysis_pipeline/regime.rs` | Configurable interval + sample threshold | +10, -2 |
| `crates/oneshim-core/src/config/sections/analysis.rs` | Add `regime_detection_interval_hours` field | +5 |

**Estimated total**: ~15 lines changed

## 5. Test Strategy

| Test | Type | Notes |
|------|------|-------|
| Detection skipped when < 50 samples | unit | Mock calibration reader returning few entries |
| Detection runs at 2h interval | unit | Verify interval check logic |
| HDBSCAN facade constructed when feature enabled | compile-time | Feature flag verification |

## 6. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| 2h interval too aggressive for low-activity users | Low | Configurable; 50-sample threshold prevents premature detection |
| HDBSCAN produces fewer clusters than k-means | Low | Facade already falls back to k-means on HDBSCAN error |

## 7. Config

| Field | Default | Description |
|-------|---------|-------------|
| `analysis.regime_detection_interval_hours` | 2 | Hours between automatic regime re-detection |

Existing config unchanged:
- `hdbscan` feature flag (default ON in Cargo.toml)
- `analysis.tiered_memory.buffer_capacity` — calibration buffer size
