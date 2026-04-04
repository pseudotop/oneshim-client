# GUI ML Phase 3 — Training Pipeline Spec

**Date**: 2026-04-04
**Branch**: `feat/analysis-wiring`
**Scope**: `oneshim-web` (export endpoint), `oneshim-vision` (hot-reload), `scripts/` (Python training)

## 1. Problem Statement

Phase 2 built the ONNX classifier infrastructure (`OnnxGuiClassifier`), but no model exists. Three things are needed:

| Component | Purpose |
|-----------|---------|
| **Training Data Exporter** | Extract crop images + labels from existing frames + gui_interactions |
| **Python Training Script** | Train RepVGG-Nano/MobileNet-v3 → export ONNX |
| **Model Hot-Reload** | Detect new `.onnx` file → reload classifier without restart |

## 2. Design

### 2.1 Training Data Exporter

**Approach**: REST API endpoint that joins `frames` + `gui_interactions` by timestamp proximity, crops the element region from the frame image, and exports as a labeled dataset.

**Endpoint**: `GET /api/export/training-data?from=...&to=...&min_confidence=0.8`

**Algorithm**:
1. Query `gui_interactions` in the date range → list of (timestamp, bbox_json, element_type, app_name)
2. For each interaction, find the nearest `frame` by timestamp (within 5s window)
3. Load the frame's WebP file, decode to RGBA
4. Crop the bbox region, resize to 64×64
5. Package as: `{label}/{uuid}.png` directory structure (ImageNet-style)

**Output format**: ZIP file containing:
```
training-data/
├── Button/
│   ├── a1b2c3.png
│   └── d4e5f6.png
├── TextInput/
│   └── ...
├── Link/
│   └── ...
└── labels.csv   (uuid, label, app_name, confidence, timestamp)
```

**Filter**: `min_confidence` parameter filters by `type_confidence` from the heuristic scorer (Phase 1). Higher confidence = cleaner weak labels.

### 2.2 Python Training Script

**Location**: `scripts/train_gui_classifier.py`

**Two model options** (user selects via CLI arg):
- `--model repvgg` (default): Custom RepVGG-Nano [48,96,192,384] channels, ~0.8M params
- `--model mobilenet`: MobileNet-v3-Small from timm, ~2.5M params

**Pipeline**:
```
1. Load training-data/ directory (ImageNet-style)
2. Split: 80% train, 20% val
3. Augmentations: RandomHorizontalFlip, ColorJitter, RandomRotation(5)
4. Train: CrossEntropyLoss, AdamW, CosineAnnealing, 30 epochs
5. Evaluate: accuracy, per-class F1
6. Export: torch.onnx.export() → gui-classifier.onnx
7. Verify: onnxruntime.InferenceSession() sanity check
```

**Dependencies**: `torch`, `torchvision`, `timm`, `onnx`, `onnxruntime`, `Pillow`

### 2.3 Model Hot-Reload

**Current**: `OnnxGuiClassifier::load(path)` is called once at startup.

**Change**: Add `reload_if_changed()` method that:
1. Check file modification time vs cached mtime
2. If changed: load new session, swap atomically
3. Log the reload event

**Call site**: Scheduler aggregation loop (every 5 min), or on-demand via IPC.

**No file watcher dependency** — simple mtime polling (lightweight, cross-platform).

## 3. Files Changed

| File | Change | Lines (~) |
|------|--------|-----------|
| `crates/oneshim-web/src/handlers/export.rs` | Add `/api/export/training-data` endpoint | +80 |
| `crates/oneshim-web/src/services/training_data_export.rs` | **NEW** — crop extraction + zip packaging | +150 |
| `crates/oneshim-vision/src/ml_classifier/mod.rs` | Add `reload_if_changed()` to OnnxGuiClassifier | +30 |
| `scripts/train_gui_classifier.py` | **NEW** — RepVGG-Nano + MobileNet-v3 training | +200 |
| `scripts/requirements-training.txt` | **NEW** — Python dependencies | +6 |

**Estimated total**: ~460 lines (Rust ~260 + Python ~200)

## 4. Key Decisions

- **ZIP not raw directory**: Endpoint returns ZIP for easy download/transfer
- **Weak supervision**: Heuristic labels (type_confidence > 0.8) as initial labels. Can be refined later via API/manual review.
- **No new Rust dependencies**: ZIP created with existing `zip` crate (already in workspace for auto-updater)
- **Python script is standalone**: Not integrated into Rust build. User runs separately.
- **mtime polling not inotify**: Simpler, cross-platform, no new deps. 5-min interval is fine.
