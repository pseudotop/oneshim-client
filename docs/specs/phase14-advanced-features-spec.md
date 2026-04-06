# Phase 14: Advanced Features

## Status: DRAFT

**Branch**: `feature/phase14-advanced`
**Depends on**: Phase 15 complete (v0.4.29-rc.1)

---

## Scope Decision

| Feature | Feasibility | Decision |
|---------|------------|----------|
| X1: Workflow Builder | Medium (controller 80% done) | **In scope** — form-based preset CRUD |
| X2: LAN Multi-Device Sync | Small (transport 85% done) | **In scope** — sync UX + conflict display |
| X3: Playbook Library | Small (108 templates exist) | **In scope** — browsable library UI |
| X4: Privacy Vector Analysis | Large (requires new pipeline) | **Deferred** to Phase 16+ |
| X5: Screenshot Annotation | Medium-Large | **In scope** — data model + annotation API (no drawing UI) |

**Implementation order**: X3 → X1 → X2 → X5 (easiest to hardest)

---

## X3: Playbook Library (Browsable Template & Preset Catalog)

### Current State

- 108 coaching templates (54 EN + 54 KO) across 5 profiles, 6+ triggers, 3 tones
- `CoachingTemplateRegistry` with `select()` (3-tier fallback)
- 15 built-in `WorkflowPreset` definitions (Productivity/AppManagement/Workflow/Custom categories)
- No browsing UI — templates are internal, presets listed via IPC stub

### Goal

Expose coaching templates and automation presets as a unified "Playbook Library" page in the web dashboard, with categorized browsing and preview.

### Design

#### 1. Playbook IPC Commands

Add to `src-tauri/src/commands/coaching.rs`:

```rust
#[command]
pub fn list_coaching_templates() -> Vec<CoachingTemplateDto> {
    // Return all templates from CoachingTemplateRegistry (profile, trigger, tone, locale, text)
}

#[command]
pub fn list_coaching_profiles() -> Vec<CoachingProfileDto> {
    // Return available profiles with description and template count
}
```

`CoachingTemplateDto`:
```rust
pub struct CoachingTemplateDto {
    pub profile: String,
    pub trigger_type: String,
    pub tone: String,
    pub locale: String,
    pub text: String,
}
```

#### 2. Playbook Web API Endpoints

Add to `crates/oneshim-web/src/routes.rs`:
- `GET /api/playbooks/coaching` — list coaching templates (filtered by locale)
- `GET /api/playbooks/presets` — list automation presets (with step details)

#### 3. Frontend Playbook Page

Create `crates/oneshim-web/frontend/src/pages/Playbooks.tsx`:
- Tabbed view: "Coaching Templates" | "Automation Presets"
- Filter by: profile/category, trigger type, tone
- Template preview card: shows profile badge, trigger tag, sample text
- Preset card: shows name, category, step count, description
- "Run Preset" button for automation presets

### Affected Crates

| Crate | Changes |
|-------|---------|
| `oneshim-analysis` | Expose template listing from registry |
| `src-tauri` | New IPC commands for template/preset listing |
| `oneshim-web` | New API endpoints + Playbooks page |

### Estimated Tests: ~4

---

## X1: Workflow Preset Editor (Form-Based)

### Current State

- `WorkflowPreset` model: id, name, description, category, steps[]
- `WorkflowStep` model: name, intent (ClickElement/TypeIntoElement/ExecuteHotkey/WaitForText/ActivateApp/Raw), delay_ms, stop_on_failure
- `AutomationController.run_workflow()` executes presets with audit + timing
- `list_automation_presets` IPC command exists but returns empty (no user presets stored)
- `run_automation_preset` IPC command exists but incomplete
- No preset CRUD — presets are hardcoded (15 built-in)

### Goal

Allow users to create, edit, delete, and run custom workflow presets via a form-based editor in the web dashboard (not a visual graph builder).

### Design

#### 1. Preset Storage

Add preset CRUD to `oneshim-storage` (SQLite V29 migration):

```sql
CREATE TABLE IF NOT EXISTS automation_presets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    category TEXT DEFAULT 'Custom',
    steps_json TEXT NOT NULL,  -- JSON array of WorkflowStep
    builtin INTEGER DEFAULT 0,
    platform TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

#### 2. PresetStorage Port

New sync port in `oneshim-core` (follows `FewShotStorage` pattern — sync, not async):

```rust
/// Synchronous storage port for automation presets.
/// Follows FewShotStorage/FocusStorage sync pattern (SQLite ops).
pub trait PresetStorage: Send + Sync {
    fn list_presets(&self) -> Result<Vec<WorkflowPreset>, CoreError>;
    fn get_preset(&self, id: &str) -> Result<Option<WorkflowPreset>, CoreError>;
    fn save_preset(&self, preset: &WorkflowPreset) -> Result<(), CoreError>;
    fn delete_preset(&self, id: &str) -> Result<(), CoreError>;
}
```

#### 3. IPC Commands

Update `src-tauri/src/commands/automation.rs`:

```rust
#[command]
pub fn list_presets() -> Vec<PresetDto>; // Built-in + user presets

#[command]
pub fn save_preset(preset: PresetInput) -> Result<PresetDto, String>;

#[command]
pub fn delete_preset(id: String) -> Result<(), String>;

#[command]
pub async fn run_preset(id: String) -> Result<PresetRunResultDto, String>;
```

#### 4. Web API + Frontend Editor

Add endpoints:
- `GET /api/automation/presets` — list all presets
- `POST /api/automation/presets` — create preset
- `PUT /api/automation/presets/{id}` — update preset
- `DELETE /api/automation/presets/{id}` — delete preset
- `POST /api/automation/presets/{id}/run` — execute preset

Frontend: `Automation.tsx` gains a "Presets" tab with:
- Preset list (built-in + custom, category filter)
- "New Preset" form: name, description, category dropdown
- Step editor: add/remove/reorder steps, each with intent type selector + parameters
- "Run" button with result display

### Affected Crates

| Crate | Changes |
|-------|---------|
| `oneshim-core` | PresetStorage port, WorkflowPreset serialization |
| `oneshim-storage` | V29 migration, PresetStorage impl |
| `src-tauri` | Updated automation IPC commands |
| `oneshim-web` | API endpoints + frontend preset editor |

### Estimated Tests: ~8

---

## X2: LAN Sync UX (Peer Management + Conflict Display)

### Current State

- Full LAN sync transport: mDNS discovery, HTTPS peer server, HMAC auth, push/pull
- `SyncTransport` trait with push/pull/discover_peers
- IPC: `get_sync_status`, `trigger_sync_cycle`, `discover_sync_peers`
- Sync result tracking: applied/skipped/tombstoned counts
- No user-facing peer management or conflict resolution UI

### Goal

Add peer management UI and sync activity display. Users can see peers, trigger sync, view results.

### Design

#### 1. Enhanced Sync Status

Note: `SyncConfig` has `device_name` but no `device_id` field. The `installation_id` from UpdateConfig (added in Phase 15 U4) can serve as device_id for sync. Reuse via `config.update.installation_id`.

Extend `SyncStatusDto` to include peer info and last sync result:

```rust
pub struct SyncStatusDto {
    pub enabled: bool,
    pub device_id: String,
    pub device_name: String,
    pub peers: Vec<SyncPeerDto>,
    pub last_sync: Option<SyncResultDto>,
    pub last_sync_at: Option<String>,
}
```

#### 2. Sync Settings IPC

Add to `src-tauri/src/commands/sync.rs`:

```rust
#[command]
pub fn set_sync_enabled(enabled: bool) -> Result<(), String>;

#[command]
pub fn set_sync_passphrase(passphrase: String) -> Result<(), String>;

#[command]
pub fn forget_peer(device_id: String) -> Result<(), String>;
```

#### 3. Frontend Sync Page

Create a "Sync" section in Settings (or dedicated page):
- Device identity: show device_id, device_name (editable)
- Passphrase setup for peer authentication
- Peer list: device name, last sync time, connection status
- "Sync Now" button with result display (applied/skipped/tombstoned)
- "Forget Peer" button per peer
- Enable/disable toggle

### Affected Crates

| Crate | Changes |
|-------|---------|
| `src-tauri` | Enhanced sync IPC commands |
| `oneshim-web` | Sync settings UI |
| `oneshim-core` | SyncConfig additions (if needed) |

### Estimated Tests: ~4

---

## X5: Screenshot Annotation (Data Model + API)

### Current State

- Frame capture + persistence: `FrameFileStorage` saves WebP files to `frames/YYYY-MM-DD/`
- Ring buffer for dashcam capture: `CaptureRingBuffer` with RingFrame (timestamp, thumbnail, app, window, accessibility)
- Frame endpoints: `GET /api/frames`, `GET /api/frames/{id}/image`
- Timeline page with frame playback
- No annotation data model or API

### Goal

Add annotation data model and REST API for attaching highlights and memos to captured frames. Phase 14 scope: backend only (annotation drawing UI deferred).

### Design

#### 1. Annotation Model

In `oneshim-core/src/models/`:

```rust
pub struct FrameAnnotation {
    pub annotation_id: String,
    pub frame_id: i64,  // matches frames table INTEGER PRIMARY KEY
    pub annotation_type: AnnotationType,
    pub x: f32,        // normalized 0.0-1.0 relative to frame width
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Option<String>,  // hex color
    pub text: Option<String>,   // memo text
    pub created_at: DateTime<Utc>,
}

pub enum AnnotationType {
    Highlight,  // rectangular highlight region
    Memo,       // text note pinned to a coordinate
    Arrow,      // arrow annotation
}
```

#### 2. Storage (V30 migration)

```sql
CREATE TABLE IF NOT EXISTS frame_annotations (
    annotation_id TEXT PRIMARY KEY,
    frame_id INTEGER NOT NULL,
    annotation_type TEXT NOT NULL,
    x REAL NOT NULL,
    y REAL NOT NULL,
    width REAL DEFAULT 0,
    height REAL DEFAULT 0,
    color TEXT,
    text TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_annotations_frame ON frame_annotations(frame_id);
```

#### 3. AnnotationStorage Port

```rust
pub trait AnnotationStorage: Send + Sync {
    fn list_annotations(&self, frame_id: i64) -> Result<Vec<FrameAnnotation>, CoreError>;
    fn save_annotation(&self, annotation: &FrameAnnotation) -> Result<(), CoreError>;
    fn delete_annotation(&self, annotation_id: &str) -> Result<(), CoreError>;
}
```

#### 4. REST API

Add endpoints:
- `GET /api/frames/{frame_id}/annotations` — list annotations
- `POST /api/frames/{frame_id}/annotations` — create annotation
- `DELETE /api/frames/{frame_id}/annotations/{id}` — delete annotation

### Affected Crates

| Crate | Changes |
|-------|---------|
| `oneshim-core` | FrameAnnotation model, AnnotationStorage port |
| `oneshim-storage` | V30 migration, AnnotationStorage impl |
| `oneshim-web` | Annotation REST endpoints |

### Estimated Tests: ~6

---

## Cross-Cutting Concerns

### Migration Versions

- V29: `automation_presets` table (X1)
- V30: `frame_annotations` table (X5)

### Backward Compatibility

All new features are additive:
- New IPC commands don't affect existing ones
- New API endpoints don't conflict with existing routes
- New SQLite tables don't alter existing schema
- Frontend pages are new routes (no existing page changes)

### Implementation Order

1. **X3 first** (smallest — just expose existing data)
2. **X1 second** (preset CRUD + storage + form editor)
3. **X2 third** (sync UX — mostly frontend)
4. **X5 last** (annotation model + storage + API)

## Estimated Impact

- **New files**: ~8 (pages, API handlers, ports, migration files)
- **Modified files**: ~12
- **New tests**: ~22
- **Lines added**: ~800-1000
- **Migrations**: V29, V30
