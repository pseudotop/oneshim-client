# CLAUDE.md — client-rust

ONESHIM Rust desktop client. 13-crate Cargo workspace, Hexagonal Architecture.

## Essential Commands

```bash
# Build/Verify
cargo check --workspace
cargo test --workspace
cargo clippy --workspace
cargo fmt --check

# Release build
cargo build --release -p oneshim-app

# Run binary
cargo run -p oneshim-app

# Test specific crate
cargo test -p oneshim-core
cargo test -p oneshim-vision

# Tauri 데스크탑 앱 빌드
cd src-tauri && cargo tauri build

# Tauri 개발 서버 (frontend HMR 포함)
cd src-tauri && cargo tauri dev
```

## Release Process

**RC 릴리스는 반드시 `release.sh`를 사용한다. 수동 태그 생성 금지.**

```bash
# RC 릴리스 (CHANGELOG 자동 생성 + 버전 동기화 + 커밋 + 태그)
./scripts/release.sh 0.4.1-rc.5

# Stable 승격 (검증된 RC → stable 릴리스)
./scripts/promote-stable.sh 0.4.1-rc.5
```

### 릴리스 플로우

1. feature 브랜치에서 작업 완료 → PR 생성 → CI 통과 → main 머지
2. `./scripts/release.sh <version>` 실행:
   - `[Unreleased]` 비어있으면 `git-cliff`로 CHANGELOG 자동 생성
   - `Cargo.toml` + `tauri.conf.json` 버전 동기화
   - 릴리스 커밋 생성 → PR용 브랜치 푸시
3. PR 머지 후 태그가 CI에 의해 빌드 → GitHub Releases에 인스톨러 업로드

### 주의사항

- **`git tag` 직접 사용 금지** — `release.sh`가 CHANGELOG, 버전 파일, 검증을 모두 처리
- **CHANGELOG.md는 `git-cliff`가 자동 생성** — 수동 편집 불필요
- **conventional commit 형식 필수** — `feat:`, `fix:`, `refactor:`, `docs:` 등 (`git-cliff`가 파싱)

## Workspace Structure

```
client-rust/
├── Cargo.toml              # Workspace root (resolver = "2")
├── src-tauri/              # Tauri v2 binary crate (active main binary, pkg "oneshim-app")
│   ├── src/
│   │   ├── main.rs         # Tauri app builder + DI wiring
│   │   ├── tray.rs         # System tray menu
│   │   ├── commands/       # Tauri IPC commands (directory module, ADR-003)
│   │   └── scheduler/      # 9-loop background scheduler
│   └── tauri.conf.json     # Tauri configuration
├── docs/
│   ├── architecture/   # ADR-001~ADR-004
│   ├── guides/         # Playbooks/runbooks/how-to docs
│   └── research/       # Exploratory notes
└── crates/
    ├── oneshim-core/       # Domain models + port traits + errors + config
    ├── oneshim-network/    # JWT auth, HTTP/SSE/WebSocket, gRPC, batch upload
    ├── oneshim-suggestion/ # Suggestion reception (SSE), priority queue, feedback, history
    ├── oneshim-storage/    # SQLite storage + schema migration
    ├── oneshim-monitor/    # System metrics (sysinfo), active window, activity tracking
    ├── oneshim-vision/     # Screen capture, delta encoding, WebP, thumbnail, PII filter
    ├── oneshim-web/        # Local web dashboard — Axum REST API + React frontend
    ├── oneshim-automation/ # Automation control — policy-based command execution, audit logging
    ├── oneshim-analysis/   # LLM analysis pipeline — segment summarization, vector RAG
    ├── oneshim-embedding/  # Vector embedding + compression — INT8 quantization, similarity search
    ├── oneshim-lint/       # Workspace lint tool (language-check binary)
    ├── oneshim-api-contracts/ # Shared API type contracts
    └── oneshim-app/        # ⚠️ DEPRECATED — removed from workspace (replaced by src-tauri)
```

## Core Architecture Rules

### Hexagonal Architecture (Ports & Adapters)

`oneshim-core` defines all traits (ports) and models. The other crates act as adapters (except `oneshim-lint`, a standalone workspace tool).

```
oneshim-core  ←  oneshim-monitor
              ←  oneshim-vision
              ←  oneshim-network
              ←  oneshim-storage
              ←  oneshim-suggestion  ←  oneshim-network
              ←  oneshim-automation
              ←  oneshim-analysis    ←  oneshim-core
              ←  oneshim-embedding   ←  oneshim-core
              ←  oneshim-api-contracts
              ←  src-tauri           ←  (all, Tauri v2 main binary)

oneshim-lint     (standalone — no oneshim-core dependency)
```

**Forbidden**: Direct dependency between adapter crates (e.g., monitor → storage). All cross-crate communication must go through `oneshim-core` traits.

**Exceptions**: `suggestion → network` (SSE reception).

**Accepted deviations**:
- `AppState.storage: Arc<SqliteStorage>` uses concrete type (not `Arc<dyn T>`) because `SqliteStorage` implements 10+ disjoint port traits (`StorageService`, `MetricsStorage`, `WebStorage`, `FocusStorage`, `VectorStore`, etc.) — a single trait object cannot represent this.
- `FocusStorage` and `WebStorage` traits are synchronous (no `#[async_trait]`) — called via `block_in_place` from sync SQLite operations.

### Error Strategy (ADR-001 §1)

- Library crates: `thiserror` — specific error enums
- Binary crate (`src-tauri`): `anyhow::Result`
- External crate errors are wrapped using `#[from]`

### Async Trait Pattern (ADR-001 §2)

Apply `#[async_trait]` to all port traits. Required for `Arc<dyn PortTrait>` DI. All port traits use `&self` (not `&mut self`) — implementations needing mutable state use interior mutability (`Mutex`, `RwLock`, atomic types).

```rust
#[async_trait]
pub trait ApiClient: Send + Sync {
    async fn upload_context(&self, context: &ContextPayload) -> Result<(), CoreError>;
}
```

### DI Pattern (ADR-001 §3)

Constructor injection + `Arc<dyn T>`. No DI framework is used. Wiring is manually performed in `src-tauri/src/main.rs`. All port implementations are wrapped directly in `Arc` — never `Arc<Mutex<Box<dyn T>>>`.

### Testing (ADR-001 §5)

Manual mock implementation (mockall is not used). Trait implementations inside `#[cfg(test)]` modules.

## Crate Summary

### oneshim-core (Foundation)
- `models/`: suggestion, event, frame, context, session, system_metrics, batch
- `ports/`: ApiClient, SseClient, StorageService, SystemMonitor, ProcessMonitor, ActivityMonitor, CaptureTrigger, FrameProcessor, DesktopNotifier, Compressor
- `error.rs`: `CoreError` (thiserror) — Network, RequestTimeout, RateLimit, ServiceUnavailable variants
- `config/`: `AppConfig` + section settings — directory module (ADR-003)
  - `mod.rs`: `AppConfig` struct + `Default` impl + helpers + re-exports
  - `enums.rs`: `PiiFilterLevel`, `Weekday`, `SandboxProfile`, `AiAccessMode`, `AiProviderType`, etc.
  - `sections.rs`: 20 config section structs (`NotificationConfig`, `TelemetryConfig`, `PrivacyConfig`, `ScheduleConfig`, `FileAccessConfig`, etc.) + `Default` impls
- `consent.rs`: `ConsentManager`, `ConsentPermissions`, `ConsentRecord` — GDPR Article 17/20 compliant
- `config_manager.rs`: JSON-based config file manager + platform-specific paths

### oneshim-network (Network Adapter)
- `auth.rs`: `TokenManager` — JWT login/refresh/logout, `RwLock<TokenState>`
- `http_client.rs`: `HttpApiClient` — REST API (impl ApiClient), timeout detection (`map_reqwest_error`)
- `sse_client.rs`: `SseStreamClient` — SSE stream + auto-reconnect (exponential backoff 1s→30s)
- `compression.rs`: `AdaptiveCompressor` — auto selection of gzip/zstd/lz4
- `batch_uploader.rs`: `BatchUploader` — Lock-free SegQueue + dynamic batch size + retry
- `ai_llm_client/`: `RemoteLlmProvider` — directory module (ADR-003)
  - `mod.rs`: `RemoteLlmProvider` struct + `LlmProvider` impl + re-exports
  - `request.rs`: request building helpers per provider type
  - `parsers.rs`: response parsing + extraction
  - `tests.rs`: unit tests
- `ai_ocr_client/`: `RemoteOcrProvider` — directory module (ADR-003)
  - `mod.rs`: `RemoteOcrProvider` struct + `OcrProvider` impl + re-exports
  - `ollama.rs`: Ollama-specific request/response handling
  - `parsers.rs`: element extraction + JSON parsing
  - `strategy.rs`: provider strategy selection
  - `tests.rs`: unit tests
- **Sync** (`sync/`):
  - `lan_server/`: LAN peer discovery server — directory module (ADR-003)
    - `mod.rs`: `LanServer` struct + public API + re-exports
    - `handlers.rs`: request handler methods
    - `session.rs`: session management
    - `tls.rs`: TLS configuration
    - `tests.rs`: unit tests
  - `lan_transport/`: LAN transport client — directory module (ADR-003)
    - `mod.rs`: `LanTransport` struct + `SyncTransport` impl + re-exports
    - `auth.rs`: peer authentication
    - `operations.rs`: sync operations (push/pull/merge)
    - `tests.rs`: unit tests
- **Integration** (`integration/`):
  - `http_transport/`: HTTP remote transport — directory module (ADR-003)
    - `mod.rs`: `HttpTransport` struct + `SyncTransport` impl + re-exports
    - `connect.rs`: connection management
    - `egress.rs`: outbound data egress
    - `inbox.rs`: inbound data inbox
    - `tests.rs`: unit tests
  - `auth/`: Integration authentication — directory module (ADR-003)
    - `mod.rs`: re-exports
    - `oidc_device_flow.rs`: OIDC device authorization flow
    - `proof_factory.rs`: request proof generation
    - `static_auth.rs`: static token authentication
    - `tests.rs`: unit tests
- **gRPC Client** (`#[cfg(feature = "grpc")]`):
  - `grpc/mod.rs`: module exports + `GrpcConfig`
  - `grpc/auth_client.rs`: `GrpcAuthClient` — Login, Logout, RefreshToken, ValidateToken
  - `grpc/session_client.rs`: `GrpcSessionClient` — CreateSession, EndSession, Heartbeat
  - `grpc/context_client.rs`: `GrpcContextClient` — UploadBatch, SubscribeSuggestions, SendFeedback, ListSuggestions
  - `grpc/unified_client.rs`: `UnifiedClient` — gRPC + REST unified client, Feature Flag based switching

### oneshim-suggestion (Suggestion Pipeline)
- `receiver.rs`: SSE → `Suggestion` conversion + queue + notification
- `queue.rs`: `BTreeSet` priority queue (max 50, Critical > High > Medium > Low)
- `feedback.rs`: Accept/Reject → HTTP POST
- `presenter.rs`: `SuggestionView` — UI data mapping
- `history.rs`: FIFO history cache

### oneshim-storage (Local Storage)
- `sqlite.rs`: `SqliteStorage` (impl StorageService) — WAL mode + PRAGMA optimizations
- `migration.rs`: schema V1-V17 (events, frames, work_sessions, interruptions, focus_metrics, local_suggestions, activity_segments, embedding_vectors, regimes, FTS5, gui_interactions, sync, IVF index, coaching)
- `frame_storage.rs`: Frame image file storage + retention policy + buffer pool + parallel I/O
- Retention Policy: 30 days, 500MB
- Performance optimization: compound indexes, batch inserts, memory cache, ArrayQueue buffer pool

### oneshim-monitor (System Monitoring)
- `system.rs`: `SysInfoMonitor` — CPU/Memory/Disk/Network (sysinfo 0.38)
- `process.rs`: `ProcessTracker` — active process/window + `get_detailed_processes()`
- `macos.rs`: macOS specific (`#[cfg(target_os = "macos")]`) — osascript
- `windows.rs`: Windows specific (`#[cfg(target_os = "windows")]`) — Win32 GetForegroundWindow + sysinfo
- `linux.rs`: Linux specific (`#[cfg(target_os = "linux")]`) — xdotool/xprintidle (X11), Wayland XWayland fallback
- `activity.rs`: `ActivityTracker` — Idle detection
- `input_activity.rs`: `InputActivityCollector` — Mouse/Keyboard pattern collection (atomic counters)
- `window_layout.rs`: `WindowLayoutTracker` — window layout change tracking

### oneshim-vision (Edge Image Processing)
- `capture.rs`: `ScreenCapture` — multi-monitor capture using xcap
- `trigger.rs`: `SmartCaptureTrigger` (impl CaptureTrigger) — event classification + importance + throttle, interior mutability (`Mutex<TriggerState>`)
- `delta.rs`: 16x16 tile comparison → changed region extraction (pointer-based fast pixel access)
- `encoder.rs`: WebP encoding (Low/Medium/High quality) + stat-based quality prediction
- `thumbnail.rs`: fast_image_resize + LRU caching (100 entries, FNV-1a hash)
- `processor.rs`: `EdgeFrameProcessor` (impl FrameProcessor) — branches by importance, interior mutability (`Mutex<Option<DynamicImage>>` for prev_frame)
  - >= 0.8: Full + OCR
  - >= 0.5: Delta
  - >= 0.3: Thumbnail
  - < 0.3: Metadata only
- `ocr.rs`: `OcrExtractor` — leptess(Tesseract) OCR (`#[cfg(feature = "ocr")]`), async support
- `privacy.rs`: PII filter levels (Off/Basic/Standard/Strict cascaded inheritance), sensitive app auto-detection, phone/API key/IP/email/credit card/SSN/file path masking
- `timeline.rs`: In-memory frame timeline + filters
- `gui_detector/`: GUI element detection — directory module (ADR-003)
  - `mod.rs`: `GuiDetector` struct + public API + re-exports
  - `correlation.rs`: GUI correlation logic
  - `inference.rs`: element inference
  - `tests.rs`: unit tests
- `accessibility/macos/`: macOS accessibility adapter — directory module (ADR-003)
  - `mod.rs`: re-exports
  - `extractor.rs`: AX tree element extraction
  - `observer.rs`: AX notification observer
  - `tests.rs`: unit tests

### oneshim-web (Local Web Dashboard)
- `lib.rs`: `WebServer` — Axum 0.8 HTTP server + graceful shutdown
- `routes.rs`: REST API route definitions (16+ endpoints)
- `handlers/`: metrics, processes, idle, sessions, frames, events, stats, tags, focus
- `embedded.rs`: static file serving for React frontend using rust-embed
- `error.rs`: `ApiError` — JSON error responses
- `frontend/`: React 18 + Vite + Tailwind CSS + Recharts + FocusWidget

### oneshim-automation (Automation Control)
- `controller/`: `AutomationController` — directory module (ADR-003)
  - `mod.rs`: struct + builders + validators + re-exports
  - `types.rs`: `AutomationCommand`, `CommandResult`, `WorkflowResult`, etc.
  - `intent.rs`: intent execution + scene analysis methods
  - `preset.rs`: workflow/preset execution methods
- `policy/`: `PolicyClient` — directory module (ADR-003)
  - `mod.rs`: public API + re-exports
  - `models.rs`: `AuditLevel`, `ExecutionPolicy`, `PolicyCache`, `ProcessOutput`
  - `token.rs`: token generation, parsing, signature verification
- `audit.rs`: `AuditLogger` — local VecDeque buffer + batched audit logs transmission, buffer overflow management

### oneshim-analysis (LLM Analysis Pipeline)
- `analyzer.rs`: `ContextAnalyzer` — segment summarization via LLM, regime classification
- `embedding_pipeline.rs`: `EmbeddingPipeline` — content activity + LLM summary embedding with optional INT8 quantization
- `vector_retriever.rs`: `VectorRetriever` — vector similarity search with quantized + adaptive strategy support
- `regime_classifier.rs`: `RegimeClassifier` — behavioral regime detection and labeling
- `regime_manager.rs`: `RegimeManager` — regime lifecycle (create, merge, split, mark_seen)
- `auto_tuner.rs`: `EmaStatsTracker`, `DriftDetector` — exponential moving average baselines and behavioral drift detection
- `coaching_engine/`: `CoachingEngine` — directory module (ADR-003)
  - `mod.rs`: `CoachingEngine` struct + public API + re-exports
  - `guards.rs`: coaching guard conditions and eligibility checks
  - `triggers.rs`: coaching trigger evaluation and event matching
- `coaching_template/`: coaching template system — directory module (ADR-003)
  - `mod.rs`: template registry + public API + re-exports
  - `templates.rs`: built-in coaching template definitions
- `adaptive_search.rs`: `AdaptiveSearchCoordinator` — auto strategy selection (brute-force / IVF / IVF+binary)

### oneshim-embedding (Vector Embedding + Compression)
- `lib.rs`: `EmbeddingService` — vector embedding generation, INT8 scalar quantization, similarity search
- Compression: 4x storage reduction via INT8 quantization with configurable float32 retention

### oneshim-api-contracts (Shared API Type Contracts)
- Shared request/response types between client crates
- Ensures API contract consistency across the workspace
- `provider_specs/`: AI provider specifications — directory module (ADR-003)
  - `mod.rs`: re-exports + public API
  - `enums.rs`: provider type enums
  - `models.rs`: provider spec data models
  - `helpers.rs`: utility functions
  - `parsers.rs`: spec parsing logic
  - `queries.rs`: provider query types
  - `resolvers.rs`: provider resolution logic
  - `validation.rs`: spec validation rules
  - `tests.rs`: unit tests

### oneshim-app (crates/oneshim-app/) — DEPRECATED
> **Removed from workspace.** Replaced by `src-tauri/` which is the active binary crate
> (also named `oneshim-app` as its package). Do not add new code here.
>
> Legacy modules (scheduler, focus_analyzer, updater, lifecycle, etc.) have been
> migrated into `src-tauri/src/`.

## Key Dependencies

| Category | Crate | Version |
|----------|-------|---------|
| Runtime | tokio | 1 (full) |
| HTTP | reqwest | 0.13 |
| Web Server | axum + tower-http | 0.8 / 0.6 |
| SSE | eventsource-stream | 0.2 |
| WebSocket | tokio-tungstenite | 0.28 |
| **gRPC** | tonic + tonic-prost + prost | 0.14 / 0.14 / 0.14 |
| DB | rusqlite | 0.38 (bundled, fallible_uint) |
| Monitoring | sysinfo | 0.38 |
| Image | image + fast_image_resize + webp + xcap | 0.25 / 6 / 0.3 / 0.8 |
| **Desktop Shell** | tauri | 2 |
| Windows API | windows-sys | 0.61 |
| Error | thiserror / anyhow | 2 / 1 |
| Serialization| serde + serde_json | 1 |
| Concurrency | crossbeam + parking_lot | 0.8 / 0.12 |
| Caching | lru | 0.16 |
| Auto Update | self_update + semver | 0.42 / 1 |
| Decompression| tar + zip | 0.4 / 2 |

## Coding Conventions

- Comments/Documentation: **English-first** (public docs require Korean companion docs for key guides)
- Rust edition: **2021**, Minimum version: **1.77.1**
- Code Formatting: `cargo fmt` (rustfmt default settings)
- Linting: `cargo clippy` — `dead_code` warnings are allowed only for variants intended for future use
- Frontend Linting: `pnpm lint` (Biome) — `useExhaustiveDependencies` enabled
- Testing: Write in `#[cfg(test)] mod tests` at the bottom of each module
- Logging: `tracing` macros (`debug!`, `info!`, `warn!`, `error!`)
- Serialization: `serde` derive — `Serialize, Deserialize` for all models

## Architecture Guardrails

Rules to prevent known issues from recurring. Enforced by code review.

### AppState Sub-Structs

`AppState` fields are grouped into sub-structs by concern. When adding new fields:
- Capture-related → `CaptureContext`
- Connection status → `ConnectionStatus`
- Create new sub-struct if 3+ related fields would otherwise be top-level

### Monitor Loop Complexity

`spawn_monitor_loop` in `scheduler/loops/monitor.rs` must stay under 500 lines. When adding functionality, extract into a helper function in the `loops/` directory (like `coaching_helper.rs`).

### Port Instance Sharing

Ports (Arc<dyn T>) created for the Scheduler should be shared with AppState, not duplicated. If a separate instance is intentionally needed (e.g., stateless API), add a comment explaining why.

### Overlay Frontend Patterns

- **State**: All overlay state flows through the `useOverlayEvents` reducer. No local `useState` for data that comes from Rust events.
- **Event listeners**: Register in `useOverlayEvents.ts` only, with proper cleanup. Never in individual components.
- **IPC invoke**: Use dynamic `await import('@tauri-apps/api/core')` (not static import) for graceful degradation outside Tauri.
- **IPC param names**: Tauri v2 auto-converts camelCase JS → snake_case Rust. Use camelCase in JS invoke calls.
- **Reducer completeness**: Every new Tauri event → needs OverlayAction type + reducer case + OverlayState field. Missing any one causes silent failures.

### Concurrency

- Bounded collections only: use `LruCache` or `VecDeque` with max capacity for any runtime-growing collection.
- Lock acquisition order: if multiple locks needed, acquire in a single scope or document the ordering.

## Reference Documents

- [Docs Index](docs/README.md) — Document map by intent
- [ADR-001: Rust Client Architecture Patterns](docs/architecture/ADR-001-rust-client-architecture-patterns.md)
- [ADR-002: OS GUI Interaction Boundary and Runtime Split](docs/architecture/ADR-002-os-gui-interaction-boundary.md)
- [ADR-003: Directory Module Pattern for Large Source Files](docs/architecture/ADR-003-directory-module-pattern.md)
- [ADR-004: Tauri v2 Migration (iced → Tauri v2 + WebView)](docs/architecture/ADR-004-tauri-v2-migration.md) ([한국어](docs/architecture/ADR-004-tauri-v2-migration.ko.md))
- [Documentation Policy](docs/DOCUMENTATION_POLICY.md) — English-primary + Korean companion docs + metrics consistency rules
- [Project Status](docs/STATUS.md) — single source of truth for mutable quality metrics
- [Migration Overview](docs/migration/README.md) — Migration plans and history
- [Server API](docs/migration/04-server-api.md) — 29 REST endpoints + gRPC RPCs
- [Migration Phases](docs/migration/05-migration-phases.md) — Phase 0-36 plans
- [Edge Vision](docs/migration/legacy/08-edge-vision.md) — Image processing details
- [gRPC Client Guide](docs/guides/grpc-client.md) — Rust gRPC client usage
- [Contributing Guide](CONTRIBUTING.md) — Rust development guide
- [Code of Conduct](CODE_OF_CONDUCT.md) — Contributor Covenant v2.1
- [Security Policy](SECURITY.md) — Vulnerability reporting process

## Current Status

- Phase 0-35 + Privacy & Permission Control System + Superpowers-era features completed (13 crates)
- Current quality metrics (test counts, pass/fail, lint/build status) are maintained in `docs/STATUS.md` as the single source of truth
- Actual adapters connected to all ports: `SmartCaptureTrigger`, `EdgeFrameProcessor`, `DesktopNotifierImpl`
- Windows active window detection implemented (`windows-sys` + `sysinfo`)
- Cross-crate integration tests (`src-tauri/tests/`)
- `cargo check/test/clippy/fmt` pass status is tracked in `docs/STATUS.md`
- **GA Ready**: CI/CD, installers, documentation completed
- **Web Dashboard**: Available at http://localhost:10090
- **Notification System**: Desktop notifications (idle, long session, high usage)

### Implementation Phases

Phase 4.5–37 + Privacy & Permission Control System completed. For detailed phase-by-phase changelog, see [docs/PHASE-HISTORY.md](docs/PHASE-HISTORY.md).

Key capabilities by phase:
- **Web Dashboard** (P9-13): Axum REST API, React frontend, SSE real-time, heatmap
- **Config & Notifications** (P14-15): JSON config persistence, cooldown notifications
- **Data Export & Theme** (P16-17): JSON/CSV export, dark/light theme
- **Frontend Polish** (P18-21): rust-embed SPA, i18n (ko/en), keyboard shortcuts, design system
- **Tags & Search** (P22-23): Tag CRUD, tag-based search
- **Reports & Backup** (P24-25): Activity reports, JSON backup/restore
- **E2E & Replay** (P26-27): 72 Playwright tests, session replay
- **Edge Intelligence** (P28, P30-33): Focus analyzer, SQLite perf, LRU cache, lock-free queue
- **Server Integration** (P34-37): HTTP retry, gRPC client, REST standardization, port fallback
- **Privacy** (Tier 1-3): Telemetry control, PII filter, GDPR consent, automation crate
- **Superpowers** (S1-S5): GUI Intelligence (accessibility + text extraction), Text Intelligence (LLM analysis pipeline + regime classification), Vector Compression (INT8/2-bit quantization + IVF index), Cross-Device Sync (device identity + LAN peer discovery), Coaching Engine (proactive productivity coaching + MagicOverlay)
- **ADR-002 M3** (Native Adapters): macOS AX tree traversal (batch), Windows UIA CacheRequest, Linux AT-SPI (atspi 0.29), MagicOverlayDriver (Tauri WebView bridge), dashcam accessibility tagging, permission gating, R-tree spatial index (rstar), app-specific element type overrides, ContextAssembler GUI section, 13 failure scenario tests, 6 E2E smoke tests, ops docs (runbook + contract examples + security review + audit logger)
