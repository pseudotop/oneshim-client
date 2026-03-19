# Priority 2: Accuracy Improvements — Design Spec

> Created: 2026-03-19
> Status: Implemented
> Depends on: Priority 1 UX Improvements, Layer 1 (Adaptive Tiered Memory), Layer 2 (Vector RAG)

## 1. Goal

Upgrade regime detection accuracy with HDBSCAN clustering, enable per-category/process parameter auto-tuning from usage data, and provide retroactive recalibration UI for user-driven noise exclusion and regime correction.

## 2. Design Decisions

| Item | Decision | Rationale |
|------|----------|-----------|
| Clustering algorithm | Config-selectable: HDBSCAN (default) / k-means (fallback) | HDBSCAN handles non-spherical clusters + automatic k + native noise detection |
| HDBSCAN dependency | Direct in `oneshim-analysis` (ADR-011 relaxed for pure-algorithm crates) | `hdbscan` is I/O-free math library, not an adapter |
| Auto-tuning | EMA per category/process (real-time) + periodic HDBSCAN re-clustering (daily) | Streaming-friendly O(1) + batch structural detection |
| Drift detection | EWMA error rate chart — flag when deviation exceeds threshold | Standard approach for concept drift in streaming data |
| Recalibration | Constraint-based semi-supervised: user overrides → must-link/cannot-link → re-clustering | Research-backed approach for user feedback in unsupervised learning |
| Recalibration UI | Inline (timeline segment) + bulk (date range management page) | Both granular and batch correction needed |
| Real-time classification | HDBSCAN approximate_predict (fast new-point assignment) | O(log n) per point without full re-clustering |

## 3. Architecture

### 3.1 Overview

```
CalibrationStore (existing, per-event data)
       │
       ▼
┌─────────────────────────────────────────────────┐
│  RegimeDetector (upgraded)                       │
│                                                  │
│  ┌──────────────┐  ┌─────────────────────────┐  │
│  │ HdbscanDetector│  │ KmeansDetector (existing)│ │
│  │ (default)     │  │ (fallback)              │  │
│  └───────┬───────┘  └────────────┬────────────┘  │
│          │                       │               │
│          ▼                       ▼               │
│  ClusteringStrategy trait (unified interface)    │
│          │                                       │
│          ▼                                       │
│  RegimeClassifier (existing, updated)            │
│  ├── approximate_predict for HDBSCAN             │
│  └── nearest-centroid for k-means                │
└──────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  AutoTuner (NEW)                                 │
│                                                  │
│  ├── EmaStatsTracker: per-category/process EMA   │
│  │   of event rate, importance, variance          │
│  │                                                │
│  ├── DriftDetector: EWMA error rate chart         │
│  │   flags regime shifts for re-clustering        │
│  │                                                │
│  └── ThresholdAdapter: auto-adjust trigger params │
│      from EMA statistics per category              │
└──────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  RecalibrationEngine (NEW)                       │
│                                                  │
│  ├── OverrideStore: user regime/noise overrides  │
│  ├── ConstraintBuilder: overrides → constraints  │
│  └── IncrementalRecluster: apply constraints     │
│      to next HDBSCAN run                          │
└──────────────────────────────────────────────────┘

UI:
  DashboardDay (existing) → inline segment recalibration
  RecalibrationPage (NEW) → bulk date range correction
```

### 3.2 ClusteringStrategy Trait

Unified interface for both HDBSCAN and k-means:

```rust
pub trait ClusteringStrategy: Send + Sync {
    /// Detect regimes from feature vectors. Returns Result for error handling.
    fn detect(&self, features: &[RegimeFeatures]) -> Result<ClusteringResult, CoreError>;

    /// Classify a single new point against existing clusters.
    fn classify(&self, point: &RegimeFeatures) -> Option<ClusterAssignment>;

    /// Re-detect with user override constraints applied.
    fn detect_with_constraints(
        &self,
        features: &[RegimeFeatures],
        constraints: &[ClusterConstraint],
    ) -> Result<ClusteringResult, CoreError>;

    /// Algorithm name for config/logging.
    fn algorithm_name(&self) -> &str;
}

pub struct ClusteringResult {
    pub labels: Vec<i32>,            // -1 = noise (HDBSCAN), others = cluster ID
    pub centroids: Vec<RegimeFeatures>,
    pub cluster_count: usize,
    pub noise_count: usize,
    pub probabilities: Option<Vec<f32>>,  // HDBSCAN soft membership
}

pub struct ClusterAssignment {
    pub cluster_id: i32,
    pub probability: f32,  // 1.0 for k-means, soft for HDBSCAN
}

pub enum ClusterConstraint {
    NoiseLabel(usize),            // Point must be labeled as noise (Phase 1)
    ForceCluster(usize, i32),     // Point must be in specific cluster (Phase 1)
    MustLink(usize, usize),       // Phase 2: Two points must be in same cluster
    CannotLink(usize, usize),     // Phase 2: Two points must be in different clusters
}
```

NOT a port trait — this is a pure algorithm interface within `oneshim-analysis`. Both `HdbscanDetector` and `KmeansDetector` implement it.

### 3.3 HdbscanDetector

**Important**: The `hdbscan` Rust crate (v0.12) provides ONLY `cluster() -> Result<Vec<i32>>` and `calc_centers()`. It does NOT provide `approximate_predict`, constraint support, or model persistence. These are custom implementations built on top of the crate's output.

```rust
pub struct HdbscanDetector {
    min_cluster_size: usize,   // default: 5
    min_samples: Option<usize>, // default: None (auto)
    // Custom: stored cluster state for fast new-point classification
    cluster_centroids: Mutex<Vec<RegimeFeatures>>,  // Computed from cluster() labels
    cluster_labels: Mutex<Vec<i32>>,                 // Last cluster() result
}
```

Uses the `hdbscan` Rust crate for the core `cluster()` call. All other functionality is custom.

**Feature normalization**: Continuous features (`avg_event_rate`, `avg_importance`, `context_activity_signal`, `communication_ratio`) are already [0,1] from the AdaptiveTrigger sigmoid/clamp. One-hot features are 0/1. Normalization is a no-op — verify at runtime and warn if values exceed [0,1].

**Noise handling**: HDBSCAN labels noise points as -1. These map to "unclassified" regime, using global default trigger params.

**Real-time classification (custom, NOT `approximate_predict`)**: After `detect()` runs `cluster()`, compute centroids per cluster label (weighted mean of cluster members). Store centroids in `cluster_centroids`. `classify()` does nearest-centroid matching against stored centroids — same O(k) approach as k-means. This reuses the existing `euclidean_distance` function. Not as theoretically precise as true `approximate_predict` (which traverses the condensed tree), but sufficient for 5-7 clusters with 7-dimensional features.

**Constraints (Phase 1 — preprocessing/postprocessing only)**:
- `NoiseLabel` points: excluded from input before `cluster()`, added back as noise labels after
- `ForceCluster` points: after `cluster()`, override their label to the specified cluster
- `MustLink`/`CannotLink`: **deferred to Phase 2** — requires distance matrix manipulation which the crate does not expose. Current user override actions (`MarkAsNoise`, `ReassignRegime`) only generate `NoiseLabel` and `ForceCluster` constraints, so this is not a functional gap for Phase 1.

**Error handling**: `hdbscan::cluster()` returns `Result<Vec<i32>, HdbscanError>`. On failure (insufficient data, all noise), return `ClusteringResult` with `cluster_count: 0` and all labels as -1. Fall back to k-means if HDBSCAN consistently fails.

### 3.4 KmeansDetector (existing, adapted)

Wrap the existing `RegimeDetector` to implement `ClusteringStrategy`:

```rust
pub struct KmeansDetector {
    existing: RegimeDetector,  // Existing hand-rolled k-means
}
```

`classify()` → nearest centroid (existing `euclidean_distance` logic).

`detect()` returns `ClusteringResult` with:
- `noise_count: 0` (k-means has no noise concept)
- All labels non-negative
- `probabilities: None` (k-means is hard assignment)

`detect_with_constraints()` → applies `NoiseLabel` (exclude points) and `ForceCluster` (post-assign) only. `MustLink`/`CannotLink` are logged as warnings and ignored.

### 3.5 Config

Extend `TieredMemoryConfig`:

```rust
pub struct TieredMemoryConfig {
    // ... existing fields ...

    #[serde(default = "default_clustering_algorithm")]
    pub clustering_algorithm: ClusteringAlgorithm,

    #[serde(default)]
    pub auto_tuning: AutoTuningConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClusteringAlgorithm {
    #[default]
    Hdbscan,
    Kmeans,
}

pub struct AutoTuningConfig {
    pub enabled: bool,                       // default: true
    pub ema_alpha: f32,                      // default: 0.05
    pub drift_threshold: f32,                // default: 2.0 (sigma)
    pub reclustering_ari_threshold: f32,     // default: 0.7
}
```

### 3.6 AutoTuner

Pure algorithm in `oneshim-analysis`. Tracks per-category/process signal statistics and adapts trigger thresholds.

#### EmaStatsTracker

```rust
pub struct EmaStatsTracker {
    category_stats: HashMap<String, CategoryStats>,
    process_stats: HashMap<String, ProcessStats>,
    alpha: f32,
}

pub struct CategoryStats {
    pub ema_event_rate: f32,
    pub ema_importance: f32,
    pub ema_variance: f32,  // Running variance via Welford's algorithm
    pub sample_count: u64,
}

impl EmaStatsTracker {
    /// Update statistics with a new observation.
    pub fn update(&mut self, category: &str, process: &str, event_rate: f32, importance: f32) {
        // EMA update for mean + Welford's for variance
    }

    /// Get adaptive threshold for a category.
    pub fn threshold(&self, category: &str, sigma_multiplier: f32) -> Option<f32> {
        // mean + sigma_multiplier * sqrt(variance)
    }

    /// Generate per-category TriggerParams overrides from learned statistics.
    /// Percentiles are approximated via normal distribution: percentile ≈ mean + z * sigma
    /// (z=0.674 for 75th, z=-0.674 for 25th). This is valid for activity data
    /// which tends to be roughly normal within a category.
    pub fn generate_overrides(&self) -> HashMap<String, TriggerParams> {
        // For each category with enough samples (>= 20):
        // t_high ≈ mean + 0.674 * sigma  (approx 75th percentile)
        // t_low  ≈ mean - 0.674 * sigma  (approx 25th percentile)
        // alpha_long based on observed event rate stability (inverse of variance)
    }
}
```

#### DriftDetector

```rust
pub struct DriftDetector {
    ewma: f32,
    ewma_variance: f32,
    alpha: f32,
    threshold_sigma: f32,
}

impl DriftDetector {
    /// Feed a new observation. Returns true if drift detected.
    pub fn observe(&mut self, value: f32) -> bool {
        // Update EWMA
        // Check if |current - ewma| > threshold_sigma * sqrt(variance)
    }

    /// Reset after acknowledged drift (e.g., after re-clustering).
    pub fn reset(&mut self) { ... }
}
```

#### Integration

AutoTuner runs per monitor loop tick:
1. `ema_tracker.update()` with current event's category/process stats
2. Every N ticks (configurable), `drift_detector.observe()` with regime classification error rate
3. If drift detected → flag for re-clustering
4. `generate_overrides()` produces per-category `TriggerParams` → feeds into `ParamResolver` category overrides

### 3.7 RecalibrationEngine

Manages user overrides and translates them into clustering constraints.

#### OverrideStore (port)

```rust
#[async_trait]
pub trait OverrideStore: Send + Sync {
    async fn save_override(&self, override_entry: &RegimeOverride) -> Result<(), CoreError>;
    async fn list_overrides(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<RegimeOverride>, CoreError>;
    async fn delete_override(&self, override_id: &str) -> Result<(), CoreError>;
}
```

```rust
pub struct RegimeOverride {
    pub override_id: String,
    pub segment_id: String,
    pub original_regime_id: Option<String>,
    pub user_action: UserOverrideAction,
    pub created_at: DateTime<Utc>,
}

pub enum UserOverrideAction {
    MarkAsNoise,                          // "This was personal time"
    ReassignRegime { target_regime_id: String }, // "This was actually deep work"
    MarkAsPersonalTime { from: DateTime<Utc>, to: DateTime<Utc> }, // Bulk range
}
```

#### ConstraintBuilder

```rust
pub struct ConstraintBuilder;

impl ConstraintBuilder {
    /// Convert user overrides into clustering constraints.
    pub fn build_constraints(
        overrides: &[RegimeOverride],
        feature_indices: &HashMap<String, usize>, // segment_id → feature index
    ) -> Vec<ClusterConstraint> {
        // MarkAsNoise → ClusterConstraint::NoiseLabel
        // ReassignRegime → ClusterConstraint::ForceCluster
        // MarkAsPersonalTime → NoiseLabel for all segments in range
    }
}
```

#### Recalibration flow

1. User marks segments in UI (inline or bulk)
2. → `OverrideStore::save_override()`
3. Next daily re-clustering:
   - Load overrides for the clustering period
   - `ConstraintBuilder::build_constraints()`
   - `clustering_strategy.detect_with_constraints(features, constraints)`
4. New regime assignments respect user corrections
5. CalibrationStore entries for overridden segments: `is_noise = 1` (for noise overrides)

### 3.8 Reclustering Orchestration (async/sync bridge)

The daily re-clustering flow bridges async I/O (storage reads) with sync algorithms (clustering):

```rust
// In scheduler aggregation loop (async context):
async fn run_constrained_recluster(
    calibration_reader: &dyn CalibrationReader,
    override_store: &dyn OverrideStore,
    clustering_strategy: &dyn ClusteringStrategy,
    regime_manager: &mut RegimeManager,
) -> Result<(), CoreError> {
    // 1. Async: load calibration data
    let entries = calibration_reader.get_entries(...).await?;

    // 2. Async: load overrides
    let overrides = override_store.list_overrides(...).await?;

    // 3. Sync: build feature vectors + constraints
    let features = build_regime_features(&entries);
    let constraints = ConstraintBuilder::build_constraints(&overrides, &feature_index);

    // 4. Sync: run clustering with constraints
    let result = clustering_strategy.detect_with_constraints(&features, &constraints);

    // 5. Sync: update regime manager
    let regimes = build_regimes_from_result(&result, &features);
    regime_manager.update_from_detection(regimes);

    Ok(())
}
```

This pattern is used both in the scheduler's daily loop and the Tauri `trigger_recluster` command (which is async).

### 3.9 Storage

V12 migration:

```sql
CREATE TABLE regime_overrides (
    override_id TEXT PRIMARY KEY,
    segment_id TEXT NOT NULL,
    original_regime_id TEXT,
    action_type TEXT NOT NULL,  -- 'MARK_AS_NOISE', 'REASSIGN_REGIME', 'MARK_AS_PERSONAL_TIME'
    action_data TEXT,           -- JSON for ReassignRegime target or time range
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_override_segment ON regime_overrides(segment_id);
CREATE INDEX idx_override_created ON regime_overrides(created_at);
```

### 3.9 Recalibration UI

#### Inline (DashboardDay timeline)

On each timeline segment block, add a context menu (click gear icon or right-click):
- "Mark as personal time" → `UserOverrideAction::MarkAsNoise`
- "Change regime to..." → dropdown of active regimes → `UserOverrideAction::ReassignRegime`
- Visual indicator: overridden segments show a small badge/strikethrough

#### Bulk (RecalibrationPage)

New page `/recalibration`:
- Date range picker (from/to)
- List of segments in range with current regime assignment
- "Mark range as personal time" button
- Per-segment override controls
- "Trigger re-clustering now" button (calls re-clustering on demand)
- Override history list with undo capability

API endpoints:
```
POST /api/recalibration/override   — create override
DELETE /api/recalibration/override/:id — delete override
GET /api/recalibration/overrides?from=...&to=... — list overrides
POST /api/recalibration/recluster   — trigger on-demand re-clustering
```

Tauri commands: `create_override`, `delete_override`, `list_overrides`, `trigger_recluster`

## 4. Crate Placement

| Component | Location | Rationale |
|-----------|----------|-----------|
| `ClusteringStrategy` trait | `oneshim-analysis/src/` | Internal algorithm interface (not a port) |
| `HdbscanDetector` | `oneshim-analysis/src/` | Pure algorithm, `hdbscan` crate dep |
| `KmeansDetector` | `oneshim-analysis/src/` | Wraps existing RegimeDetector |
| `ClusteringAlgorithm` enum | `oneshim-core/src/config/` | Config type |
| `AutoTuningConfig` | `oneshim-core/src/config/` | Config type |
| `EmaStatsTracker` | `oneshim-analysis/src/` | Pure algorithm |
| `DriftDetector` | `oneshim-analysis/src/` | Pure algorithm |
| `OverrideStore` port | `oneshim-core/src/ports/` | I/O boundary |
| `RegimeOverride` model | `oneshim-core/src/models/` | Domain model |
| `ConstraintBuilder` | `oneshim-analysis/src/` | Pure algorithm |
| `OverrideStore` impl | `oneshim-storage/src/sqlite/` | SQLite adapter |
| Recalibration REST handlers | `oneshim-web/src/handlers/` | API |
| RecalibrationPage | `oneshim-web/frontend/src/pages/` | React |
| Tauri commands | `src-tauri/src/commands.rs` | IPC |

## 5. Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `hdbscan` | latest | HDBSCAN clustering (pure Rust, no I/O) |

Added to `oneshim-analysis/Cargo.toml` as optional:
```toml
hdbscan = { version = "0.10", optional = true }

[features]
default = ["hdbscan"]
```

When `hdbscan` feature is disabled, `HdbscanDetector::new()` returns error, system falls back to k-means.

## 6. SQLite Migrations

**V12**: `regime_overrides` table

## 7. Phase Scope

### This implementation

1. `ClusteringStrategy` trait + `HdbscanDetector` + `KmeansDetector` adapter
2. `ClusteringAlgorithm` config enum (HDBSCAN default, k-means fallback)
3. Feature normalization for HDBSCAN input
4. `approximate_predict` for real-time classification
5. `EmaStatsTracker` — per-category/process running statistics
6. `DriftDetector` — EWMA-based regime shift detection
7. `ThresholdAdapter` — auto-generate TriggerParams overrides from EMA
8. `OverrideStore` port + SQLite implementation (V12 migration)
9. `RegimeOverride` model + `ConstraintBuilder`
10. `detect_with_constraints()` for constrained re-clustering
11. Integration: AutoTuner in monitor loop, constrained re-clustering in daily loop
12. Recalibration REST endpoints + Tauri commands
13. Inline recalibration UI (DashboardDay segment context menu)
14. RecalibrationPage (bulk date range correction)

### Out of scope
- Bayesian GMM (future option, more complex)
- Cross-session constraint propagation
- Automated constraint generation (without user input)
- Regime merge/split UI
