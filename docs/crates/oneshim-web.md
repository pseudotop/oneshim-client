[English](./oneshim-web.md) | [한국어](./oneshim-web.ko.md)

# oneshim-web

The local web dashboard crate. Provides a REST API based on Axum 0.7 and a React 18 frontend.

## Features

- **REST API**: 60+ endpoints (metrics, processes, frames, events, tags, reports, automation, etc.)
- **Real-time SSE**: Server-Sent Events stream (metrics, frames, idle state)
- **React Frontend**: Embedded in the binary via rust-embed
- **Auto Port Finding**: Automatically tries the next port on conflict
- **Automation Dashboard**: Automation status, audit logs, workflow presets, execution statistics

## Structure

```
oneshim-web/
├── src/
│   ├── lib.rs          # WebServer + AppState (includes audit_logger)
│   ├── routes.rs       # Route definitions (60+ endpoints)
│   ├── error.rs        # ApiError type
│   ├── embedded.rs     # Static file serving
│   └── handlers/       # API handlers
│       ├── metrics.rs
│       ├── processes.rs
│       ├── idle.rs
│       ├── sessions.rs
│       ├── frames.rs
│       ├── events.rs
│       ├── stats.rs
│       ├── tags.rs
│       ├── search.rs
│       ├── reports.rs
│       ├── timeline.rs
│       ├── focus.rs
│       ├── backup.rs
│       ├── export.rs
│       ├── settings.rs    # AppSettings DTO + automation/sandbox/AI settings
│       └── automation.rs  # Automation API (10 endpoints)
└── frontend/           # React frontend
    ├── src/
    │   ├── pages/      # Page components (Dashboard, Automation, Settings, etc.)
    │   ├── components/ # UI components
    │   ├── api/        # API client
    │   ├── hooks/      # React hooks
    │   ├── i18n/       # Internationalization translations (ko/en)
    │   └── styles/     # Design tokens
    └── e2e/            # Playwright E2E tests
```

## AppState

```rust
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<SqliteStorage>,
    pub frames_dir: Option<PathBuf>,
    pub event_tx: broadcast::Sender<RealtimeEvent>,
    pub config_manager: Option<ConfigManager>,
    pub audit_logger: Option<Arc<RwLock<AuditLogger>>>,
}
```

### WebServer Builder

```rust
let server = WebServer::new(storage, web_config)
    .with_config_manager(config_manager)
    .with_audit_logger(audit_logger)
    .with_event_tx(event_tx)
    .with_frames_dir(frames_dir);

server.run(shutdown_rx).await?;
```

## API Endpoints

### Metrics
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/metrics` | Latest system metrics |
| GET | `/api/metrics/history` | Metrics history |
| GET | `/api/stats/heatmap` | Activity heatmap |

### Processes/Sessions
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/processes` | Current process list |
| GET | `/api/idle` | Idle period list |
| GET | `/api/sessions` | Session statistics |

### Frames/Events
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/frames` | Frame list (paginated) |
| GET | `/api/frames/:id` | Frame details |
| GET | `/api/frames/:id/image` | Frame image |
| GET | `/api/events` | Event list |

### Tags
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/tags` | All tags |
| POST | `/api/tags` | Create tag |
| PUT | `/api/tags/:id` | Update tag |
| DELETE | `/api/tags/:id` | Delete tag |
| POST | `/api/frames/:id/tags/:tag_id` | Add tag to frame |
| DELETE | `/api/frames/:id/tags/:tag_id` | Remove tag from frame |

### Search/Reports
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/search` | Unified search |
| GET | `/api/reports` | Activity reports |
| GET | `/api/timeline` | Unified timeline |

### Focus Analytics
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/focus/metrics` | Focus metrics |
| GET | `/api/focus/sessions` | Work sessions |
| GET | `/api/focus/interruptions` | Interruption events |
| GET | `/api/focus/suggestions` | Local suggestions |
| POST | `/api/focus/suggestions/:id/feedback` | Suggestion feedback |

### Settings/Backup
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/settings` | Get settings (includes automation/sandbox/AI) |
| POST | `/api/settings` | Update settings |
| GET | `/api/backup` | Create backup |
| POST | `/api/backup/restore` | Restore backup |
| GET | `/api/export/:type` | Export data |

### Automation
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/automation/status` | Automation system status |
| GET | `/api/automation/audit` | Query audit logs (limit, status filter) |
| GET | `/api/automation/policies` | Active policy summary |
| GET | `/api/automation/stats` | Execution statistics (success/failure/denied/timeout) |
| GET | `/api/automation/presets` | Preset list (builtin + user) |
| POST | `/api/automation/presets` | Create user preset |
| PUT | `/api/automation/presets/:id` | Update user preset |
| DELETE | `/api/automation/presets/:id` | Delete user preset |
| POST | `/api/automation/presets/:id/run` | Run preset |

### Real-time Stream
| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/stream` | SSE event stream |

## Automation API Details

### DTO Types

```rust
/// Automation system status
pub struct AutomationStatusDto {
    pub enabled: bool,
    pub sandbox_enabled: bool,
    pub sandbox_profile: String,
    pub ocr_provider: String,
    pub llm_provider: String,
    pub external_data_policy: String,
    pub pending_audit_entries: usize,
}

/// Audit log entry
pub struct AuditEntryDto {
    pub entry_id: String,
    pub timestamp: String,
    pub session_id: String,
    pub command_id: String,
    pub action_type: String,
    pub status: String,  // Started | Completed | Failed | Denied | Timeout
    pub details: Option<String>,
    pub elapsed_ms: Option<u64>,
}

/// Execution statistics
pub struct AutomationStatsDto {
    pub total_executions: usize,
    pub successful: usize,
    pub failed: usize,
    pub denied: usize,
    pub timeout: usize,
    pub avg_elapsed_ms: f64,
}

/// Policy summary
pub struct PoliciesDto {
    pub automation_enabled: bool,
    pub sandbox_profile: String,
    pub sandbox_enabled: bool,
    pub allow_network: bool,
    pub external_data_policy: String,
}

/// Preset run result
pub struct PresetRunResult {
    pub preset_id: String,
    pub success: bool,
    pub message: String,
}
```

## Settings DTO (Automation Related)

Three automation sections added to `AppSettings`:

```rust
pub struct AppSettings {
    // ... existing monitor/vision/notification/privacy settings ...
    pub automation: AutomationSettings,
    pub sandbox: SandboxSettings,
    pub ai_provider: AiProviderSettings,
}

pub struct AutomationSettings { pub enabled: bool }

pub struct SandboxSettings {
    pub enabled: bool,
    pub profile: String,              // "Permissive" | "Standard" | "Strict"
    pub allowed_read_paths: Vec<String>,
    pub allowed_write_paths: Vec<String>,
    pub allow_network: bool,
    pub max_memory_bytes: u64,
    pub max_cpu_time_ms: u64,
}

pub struct AiProviderSettings {
    pub ocr_provider: String,          // "Local" | "Remote"
    pub llm_provider: String,          // "Local" | "Remote"
    pub external_data_policy: String,  // "PiiFilterStrict" | "PiiFilterStandard" | "AllowFiltered"
    pub fallback_to_local: bool,
    pub ocr_api: Option<ExternalApiSettings>,
    pub llm_api: Option<ExternalApiSettings>,
}

pub struct ExternalApiSettings {
    pub endpoint: String,
    pub api_key_masked: String,        // GET: masked / POST: full key
    pub model: Option<String>,
    pub timeout_secs: u64,
}
```

### API Key Masking

- **GET**: `mask_api_key("sk-1234567890abcdef")` → `"sk...cdef"` (first 2 chars + `...` + last 4 chars)
- **POST**: Stores full key when received, retains existing key when masked value (`is_masked_key()`) is sent

## Frontend Pages

| Path | Page | Shortcut | Description |
|------|------|----------|-------------|
| `/` | Dashboard | `D` | System summary, CPU/Memory charts, focus |
| `/timeline` | Timeline | `T` | Screenshot thumbnail grid |
| `/search` | Search | — | Unified search + tag filters |
| `/reports` | Reports | `R` | Activity reports + statistics |
| `/replay` | Session Replay | — | Session replay |
| `/focus` | Focus Analytics | — | Focus analysis |
| `/automation` | **Automation** | `A` | Automation dashboard |
| `/settings` | Settings | `S` | Settings (automation/sandbox/AI included) |
| `/privacy` | Privacy | `P` | Privacy management |

### Automation Page

Composed of 5 panels (React Query based):

1. **Status Card** — Enabled status, sandbox profile, OCR/LLM provider, pending audit entries
2. **Workflow Presets** — Category tabs (Productivity/App Management/Workflow/Custom), preset card grid, run/CRUD
3. **Execution Statistics** — Success/failure/denied/timeout counts + average elapsed time
4. **Audit Log** — Table (timestamp, command ID, action, status badge, elapsed time), status filter, 30-second auto-refresh
5. **Policy Info** — Current applied policy summary

### Settings Page (Automation Section)

Three sections added to existing settings:

1. **Automation** — Enable toggle
2. **Sandbox** — Enable, profile dropdown, network allow toggle
3. **AI Provider** — OCR/LLM type selection, data policy, fallback toggle, external API settings (`type="password"`)

## i18n Support

220+ Korean/English translation keys:
- `automation.*` — Automation UI translations (40+)
- `settingsAutomation.*` — Automation settings translations (26+)
- Existing translations retained (dashboard, timeline, settings, privacy, search, reports, etc.)

## Usage

### Basic Execution

```rust
use oneshim_web::WebServer;

let server = WebServer::new(storage, web_config)
    .with_config_manager(config_manager)
    .with_audit_logger(audit_logger)
    .with_event_tx(event_tx);

server.run(shutdown_rx).await?;
```

### Configuration

```toml
[web]
enabled = true
port = 9090
allow_external = false
```

## Frontend Development

```bash
cd crates/oneshim-web/frontend

# Install dependencies
pnpm install

# Development server
pnpm dev

# Build
pnpm build

# E2E tests
pnpm test:e2e
```

## Tests

- **Rust tests**: 78 — API handlers, routes, error handling, automation DTO serialization, settings mapping
- **E2E tests**: 72 Playwright-based tests
  - Navigation, dashboard, timeline
  - Settings, privacy, search, reports
