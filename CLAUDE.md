# CLAUDE.md — client-rust

ONESHIM Rust desktop client. 14-crate Cargo workspace, Hexagonal Architecture.

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
│   │   └── scheduler/      # 16-loop background scheduler (monitor, metrics, process, sync, heartbeat, aggregation, notification, focus, event_snapshot, oauth_refresh, analysis, cross_device_sync, coaching + conditional: health_check, suggestion_sse, suggestion_maintenance)
│   └── tauri.conf.json     # Tauri configuration
├── docs/
│   ├── architecture/   # ADR-001~ADR-019 (see docs/architecture/ADR-*.md)
│   ├── guides/         # Playbooks/runbooks/how-to docs
│   └── research/       # Exploratory notes
└── crates/
    ├── oneshim-core/       # Domain models + port traits + errors + config
    ├── oneshim-network/    # JWT auth, HTTP/SSE, gRPC, batch upload
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
    ├── oneshim-audio/      # Audio capture and speech-to-text — cpal + whisper-rs
    └── oneshim-sandbox-worker/ # Out-of-process sandboxed automation action executor (stdin JSON → stdout JSON under platform sandbox)
```

## Core Architecture Rules

### Hexagonal Architecture (Ports & Adapters)

`oneshim-core` defines all traits (ports) and models. The other crates act as adapters (except `oneshim-lint`, a standalone workspace tool).

```
oneshim-core  ←  oneshim-monitor
              ←  oneshim-vision
              ←  oneshim-network
              ←  oneshim-storage
              ←  oneshim-suggestion
              ←  oneshim-automation
              ←  oneshim-analysis    ←  oneshim-core
              ←  oneshim-embedding   ←  oneshim-core
              ←  oneshim-audio
              ←  oneshim-api-contracts
              ←  oneshim-sandbox-worker  (standalone binary: stdin JSON → stdout JSON)
              ←  src-tauri           ←  (all, Tauri v2 main binary)

oneshim-lint     (standalone — no oneshim-core dependency)
```

**Forbidden**: Direct dependency between adapter crates (e.g., monitor → storage). All cross-crate communication must go through `oneshim-core` traits.

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
- `models/`: 34+ domain types — suggestion, event, frame, context, session, system_metrics, batch (original Phase 1 set) plus ai_session, analysis, annotation, app_registry, audio, audit, automation, bug_report, coaching, coaching_template, daily_digest, embedding, focused_element, gui, gui_activity, gui_interaction, regime, and more (superpowers/phase-4/ADR-019 additions)
- `ports/`: 57 port files declaring 95 traits — ApiClient, SseClient, StorageService, SystemMonitor, ProcessMonitor, ActivityMonitor, CaptureTrigger, FrameProcessor, DesktopNotifier, Compressor (original set) plus AnalysisProvider, AudioCapture, AuditLog, CoachingStorage, ElementFinder, EmbeddingProvider, FeedbackSignalSink, FrameStorage, IntentPlanner, LlmProvider, ModelDownloader, OcrProvider, OverlayDriver, OverrideStore, PiiSanitizer, PresetStorage, and more (see `crates/oneshim-core/src/ports/`). All traits carry `# Errors` docs per iter-174 port-trait doc campaign.
- `error_codes/`: 18 typed code enums generated via single-source `define_code_enum!` macro per [ADR-019](docs/architecture/ADR-019-error-code-infrastructure.md)
- `error.rs`: `CoreError` (thiserror) — 38 variants with typed `code: XxxCode` field per struct-variant (ADR-019). Headline variants: Config, Network, RequestTimeout, RateLimit, ServiceUnavailable, Auth, OAuth, Validation, InvalidArguments, NotFound, Storage, Automation, Vision, Analysis, Suggestion, Monitor, Embedding, etc. Wire-format contract locked at **41 codes** in `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` (and enforced by `crates/oneshim-core/tests/wire_contract_snapshot.rs`).
- `config/`: `AppConfig` + section settings — directory module (ADR-003)
  - `mod.rs`: `AppConfig` struct + `Default` impl + helpers + re-exports
  - `enums.rs`: `PiiFilterLevel`, `Weekday`, `SandboxProfile`, `AiAccessMode`, `AiProviderType`, etc.
  - `sections/`: 37 config section structs in per-domain files (`NotificationConfig`, `TelemetryConfig`, `PrivacyConfig`, `ScheduleConfig`, `FileAccessConfig`, `IntegrationConfig`, `SyncConfig`, `SandboxConfig`, etc.) + `Default` impls. Directory module split from the old single `sections.rs` per ADR-003.
- `consent.rs`: `ConsentManager`, `ConsentPermissions`, `ConsentRecord` — GDPR Article 17/20 compliant
- `config_manager.rs`: JSON-based config file manager + platform-specific paths

### oneshim-network (Network Adapter)
- `auth.rs`: `TokenManager` — JWT login/refresh/logout, `RwLock<TokenState>`
- `http_client.rs`: `HttpApiClient` — REST API (impl ApiClient), timeout detection (`map_reqwest_error`)
- `sse_client.rs`: `SseStreamClient` — SSE stream + auto-reconnect (exponential backoff 1s→30s)
- `compression.rs`: `AdaptiveCompressor` — auto selection of gzip/zstd/lz4
- `batch_uploader.rs`: `BatchUploader` — Lock-free SegQueue + dynamic batch size + retry
- `circuit_breaker.rs`: per-endpoint `CircuitBreaker` + `CircuitBreakerRegistry` (D7 broadening 2026-04-20). Shared across 6 consumers: `BatchUploader` (original), `RemoteEmbeddingProvider`, `AnalysisClient`, `RemoteOcrProvider`, `RemoteLlmProvider`, `HttpApiSession`. Registry keyed by `scheme://host:port`. `serial_test` guards module-global state paths. See [docs/superpowers/specs/2026-04-20-d7-circuit-breaker-broadening-design.md](docs/superpowers/specs/2026-04-20-d7-circuit-breaker-broadening-design.md).
- `connectivity.rs`: connectivity detection + backoff helpers
- `resilience.rs`: shared resilience primitives — `jittered_backoff_delay`, `classify_for_breaker` (D7 classification: 5xx/401/429/transport = Failure, 2xx = Success, other 4xx = Neutral), `endpoint_authority` (breaker registry key derivation), `BreakerSignal` enum.
- `error.rs`: `NetworkError` enum (13 variants, typed-code per ADR-019)
- `http_api_session/`: HTTP-based `ApiSession` (stateful chat/tool-calling) — directory module with anthropic.rs, google.rs, openai.rs provider-specific request builders, mod.rs orchestrator, tests.rs
- `local_llm_session.rs`: local `ApiSession` via subprocess LLM (bridges via `subprocess_provider`)
- `analysis_client.rs`: analysis provider client
- `remote_embedding_client.rs`: remote embedding provider (`#[cfg(feature = "embedding-remote")]` or similar)
- `oauth/`: OAuth 2.0 flow helpers — directory module
- `proto/`: protobuf-generated types (tonic-build output)
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
  - `grpc/mod.rs`: module exports
  - `grpc/config.rs`: `GrpcConfig` — endpoints, fallback ports, TLS options
  - `grpc/auth_client.rs`: `GrpcAuthClient` — Login, Logout, RefreshToken, ValidateToken
  - `grpc/session_client.rs`: `GrpcSessionClient` — CreateSession, EndSession, Heartbeat
  - `grpc/context_client.rs`: `GrpcContextClient` — UploadBatch, SubscribeSuggestions, SendFeedback, ListSuggestions
  - `grpc/health_client.rs`: `GrpcHealthClient` — Consumer Contract `ClientHealth.Ping` RPC
  - `grpc/unified_client.rs`: `UnifiedClient` — gRPC + REST unified client, Feature Flag based switching
  - `grpc/api_adapter.rs`: `GrpcApiAdapter` — `impl ApiClient` bridging UnifiedClient + HttpApiClient REST fallback
  - `grpc/sse_adapter.rs`: `GrpcSseAdapter` — `impl SseClient` bridging gRPC streaming to SuggestionReceiver
  - `grpc/error_mapping.rs`: tonic `Status` → `NetworkError` conversion (maps gRPC codes → typed network variants)

### oneshim-suggestion (Suggestion Pipeline)
- `receiver.rs`: SSE → `Suggestion` conversion + queue + notification
- `queue.rs`: `BTreeSet` priority queue (max 50, Critical > High > Medium > Low)
- `feedback.rs`: Accept/Reject → HTTP POST (+ fires `FeedbackSignalSink` per ADR-017 before network)
- `feedback_retry.rs`: `FeedbackRetryQueue` — persists failed feedback posts for scheduler-driven retry
- `deferred.rs`: deferred-suggestion handling (`snooze`, re-surface windows)
- `presenter.rs`: `SuggestionView` — UI data mapping
- `history.rs`: FIFO history cache
- `scorer.rs`: suggestion scoring helpers
- `error.rs`: `SuggestionError` (ADR-019 typed codes)

### oneshim-storage (Local Storage)
- `sqlite/`: `SqliteStorage` (impl StorageService + 10+ other port traits) — WAL mode + PRAGMA optimizations. Directory module per ADR-003; sub-modules: `metrics/`, `edge_intelligence/`, `annotation_storage_impl`, `coaching_storage`, `few_shot_storage_impl`, `focus_storage_impl`, `frames`, `fts_search_impl`, `habit_storage`, `integration_query_impl`, `lan_pin_store`, `override_store_impl`, `preset_storage_impl`, `port_contract_tests`, etc.
- `migration/`: schema V1–V31 as per-version files (`v01_v08.rs`, `v09_v18.rs`, `v19_v21.rs`, `v22_v23.rs`, `v23_v24.rs`, `v25.rs`, `v26.rs`, `v27.rs`, `v28.rs`, `v29.rs`, `v30.rs`, `v31_regime_manager_state.rs`) + `CURRENT_VERSION: u32 = 31` constant. Covers events, frames, work_sessions, interruptions, focus_metrics, local_suggestions, activity_segments, embedding_vectors, regimes, FTS5, gui_interactions, sync, IVF index, coaching, app_meta, session_audit, ai_sessions, type_confidence, regime_manager_state (v31).
- `frame_storage.rs`: Frame image file storage + retention policy + buffer pool + parallel I/O
- `integration_state_store/`, `regime_manager_state_store.rs`, `sync_extractor.rs`, `sync_merger.rs`, `device_identity.rs`, `keychain.rs`, `file_secret_store.rs`, `env_secret_store.rs`, `encryption.rs`, `maintenance.rs`, `process_env_projection.rs`, `file_transport.rs` — various orthogonal storage adapters.
- Retention Policy: 30 days, 500MB
- Performance optimization: compound indexes, batch inserts, memory cache, ArrayQueue buffer pool

### oneshim-monitor (System Monitoring)
- `system.rs`: `SysInfoMonitor` — CPU/Memory/Disk/Network (sysinfo 0.38)
- `system_info.rs`: system information wrappers
- `process.rs`: `ProcessTracker` — active process/window + `get_detailed_processes()`
- `macos.rs`: macOS specific (`#[cfg(target_os = "macos")]`) — osascript + `circuit_breaker_skips_when_tripped` serialized via `serial_test`
- `windows.rs`: Windows specific (`#[cfg(target_os = "windows")]`) — Win32 GetForegroundWindow + sysinfo
- `linux.rs`: Linux specific (`#[cfg(target_os = "linux")]`) — xdotool/xprintidle (X11), Wayland XWayland fallback
- `activity.rs`: `ActivityTracker` — Idle detection
- `idle.rs`: cross-platform idle detection helpers
- `input_activity.rs`: `InputActivityCollector` — Mouse/Keyboard pattern collection (atomic counters)
- `input_detail.rs`: richer input event details (superpowers-era addition)
- `keyboard_pattern.rs`: keyboard pattern matching
- `key_hook/`: low-level key-hook directory module (platform-branched)
- `clipboard.rs`: clipboard-change tracking
- `file_access.rs`: file-access telemetry
- `window_layout.rs`: `WindowLayoutTracker` — window layout change tracking
- `error.rs`: `MonitorError` (ADR-019 typed codes)

### oneshim-vision (Edge Image Processing) — 22 top-level entries
- `capture.rs`: `ScreenCapture` — multi-monitor capture using xcap
- `trigger.rs`: `SmartCaptureTrigger` (impl CaptureTrigger) — event classification + importance + throttle, interior mutability (`Mutex<TriggerState>`)
- `delta.rs`: 16x16 tile comparison → changed region extraction (pointer-based fast pixel access)
- `encoder.rs`: WebP encoding (Low/Medium/High quality) + stat-based quality prediction
- `thumbnail.rs`: fast_image_resize + LRU caching (100 entries, FNV-1a hash)
- `ring_buffer.rs`: bounded frame ring buffer
- `processor.rs`: `EdgeFrameProcessor` (impl FrameProcessor) — branches by importance, interior mutability (`Mutex<Option<DynamicImage>>` for prev_frame)
  - >= 0.8: Full + OCR
  - >= 0.5: Delta
  - >= 0.3: Thumbnail
  - < 0.3: Metadata only
- `ocr.rs`: `OcrExtractor` — leptess(Tesseract) OCR (`#[cfg(feature = "ocr")]`), async support
- `local_ocr_provider.rs`: local OCR provider impl
- `privacy.rs`: PII filter levels (Off/Basic/Standard/Strict cascaded inheritance), sensitive app auto-detection, phone/API key/IP/email/credit card/SSN/file path masking
- `privacy_gateway.rs`: centralized privacy gateway wrapping filter + sensitive-app detection
- `timeline.rs`: In-memory frame timeline + filters
- `element_finder.rs`: `ElementFinder` impl — combines GUI detection + AX + spatial query (R-tree via `rstar`)
- `work_classifier.rs`: activity classification from frame features
- `gui_detector/`: GUI element detection — directory module (ADR-003)
  - correlation, inference, tests
- `contour_classifier/`: OpenCV-like contour-based classifier — directory module
- `ml_classifier/`: ML-based classifier (inference pipeline) — directory module
- `native_detect/`: native platform GUI detection — directory module
- `native_ocr/`: native platform OCR — directory module
- `accessibility/`: platform accessibility adapters — `macos/` (AX extractor + observer + tests), `windows/` (UIA CacheRequest), `linux/` (AT-SPI via atspi 0.29)
- `error.rs`: `VisionError` (ADR-019 typed codes)

### oneshim-web (Local Web Dashboard)
- `lib.rs`: `WebServer` — Axum 0.8 HTTP server + graceful shutdown
- `routes.rs`: 118 REST route definitions registered via `.route(...)` (368 LoC) — covers metrics, processes, idle, sessions, frames, events, stats, tags, focus, ai_models, ai_provider_surfaces, ai_session, annotations, automation, automation_gui, backup, bug_report, coaching, daily_digest, dashboard, data, digests, export, integration, etc. Contract-frozen via `docs/contracts/oneshim-web.v1.openapi.yaml` + `http-interface-manifest.v1.json`.
- `handlers/`: 44 handler files across domain-grouped subdirectories + flat files. Originally 9 handlers at Phase-1; grew with superpowers/phase-4 features.
- `embedded.rs`: static file serving for React frontend using rust-embed
- `error.rs`: `ApiError` — JSON error responses
- `frontend/`: React 18.3 + Vite + Tailwind CSS + Recharts + FocusWidget + i18n (en/ko) + Biome lint + Vitest tests + Playwright e2e + Storybook review catalog

### oneshim-automation (Automation Control) — 29 source files
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
- `action_dispatcher.rs`: maps `AutomationCommand` → action execution
- `input_driver.rs`: cross-platform input driver (mouse/keyboard synthesis)
- `intent_planner.rs`, `intent_resolver.rs`, `resolver.rs`: intent parsing + resolution pipeline
- `local_llm.rs`: local LLM wiring for intent planning
- `overlay.rs`: overlay driver interface (surfaced via `MagicOverlayDriver` in src-tauri)
- `gui_interaction/`: GUI V2 session state + nonce replay protection + TTL enforcement (ADR-002 M3)
- `presets.rs`: preset loading + validation
- `sandbox/`: per-platform sandbox enforcement (Windows Job Objects, Linux seccomp+Landlock, macOS App Sandbox) — invoked out-of-process via `oneshim-sandbox-worker`
- `error.rs`: `AutomationError` (ADR-019 typed codes)

### oneshim-analysis (LLM Analysis Pipeline) — 56 source files grouped by theme
- **Context + segmentation**: `analyzer.rs` (ContextAnalyzer — LLM segment summarization + regime classification), `assembler.rs`, `segment_buffer.rs`, `segment_summarizer.rs`, `llm_segment_summarizer.rs`, `content_tracker.rs`
- **Regime pipeline**: `regime_classifier.rs`, `regime_detector.rs`, `regime_manager.rs`, `regime_analysis_facade.rs`, `regime_goal_tracker.rs` — behavioral regime lifecycle (create, merge, split, mark_seen) + facade for external consumption
- **Embeddings + vector search**: `embedding_pipeline.rs` (INT8 quantization), `vector_retriever.rs`, `adaptive_search.rs` (auto strategy: brute-force / IVF / IVF+binary), `hnsw_adapter.rs`, `hybrid_search_service.rs`, `query_expander.rs`, `few_shot_selector.rs`
- **Clustering**: `kmeans_adapter.rs`, `gmm_detector.rs`, `hdbscan_detector.rs`, `clustering_strategy.rs`
- **Work classification**: `work_type_classifier.rs`, `llm_work_type_refiner.rs`, `gui_work_type_refiner.rs`, `gui_aggregator.rs`, `terminal_detector.rs`, `title_bar_parser/`, `document_heading.rs`
- **Tuning + feedback**: `auto_tuner.rs` (EmaStatsTracker + DriftDetector), `adaptive_trigger.rs`, `calibration_buffer.rs`, `feedback_tracker.rs`, `param_resolver.rs`, `constraint_builder.rs`
- **Digests + insights**: `daily_digest_generator.rs`, `weekly_digest_generator.rs`, `daily_insight_generator.rs`, `digest_exporter.rs`
- **Coaching** (ADR-003 directory modules):
  - `coaching_engine/`: `CoachingEngine` struct + guards (eligibility checks) + triggers (event matching)
  - `coaching_template/`: template registry + built-in coaching templates
- **Pattern mining**: `pattern_miner/` — directory module
- **Suggestion filter + misc**: `suggestion_filter.rs`, `prompts.rs`, `fallback_analysis_provider.rs`, `focus_shared.rs`
- `error.rs`: `AnalysisError` (ADR-019 typed codes)

### oneshim-embedding (Vector Embedding + Compression)
- `lib.rs`: `EmbeddingService` — vector embedding generation, INT8 scalar quantization, similarity search
- Compression: 4x storage reduction via INT8 quantization with configurable float32 retention

### oneshim-api-contracts (Shared API Type Contracts) — 44 domain-grouped files
- Shared request/response types between `src-tauri` (Tauri commands) + `oneshim-web` (REST handlers) + frontend
- Ensures API contract consistency across the workspace; contract-frozen via `docs/contracts/oneshim-web.v1.openapi.yaml` + `http-interface-manifest.v1.json`
- **Per-domain contract files**: `ai_providers`, `ai_session`, `annotations`, `automation`, `automation_gui`, `backup`, `bug_report`, `coaching`, `common`, `dashboard`, `data`, `digests`, `events`, `export`, `focus`, `frames`, `idle`, `integration`, `metrics`, `onboarding`, `playbooks`, `pomodoro`, `processes`, `recalibration`, `reports`, `search`, `sessions`, `settings`, `stats`, `stream`, `suggestions`, `support`, `tags`, `timeline`, `update` — each carries the request/response DTOs for its surface.
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
- `error.rs`: shared error/response types

### oneshim-audio (Audio Capture + STT)
- `capture.rs`: Cross-platform microphone capture via cpal, auto-resampling to 16kHz mono
- `vad.rs`: `VadDetector` — energy-based voice activity detection with configurable threshold
- `whisper.rs`: `WhisperSttProvider` — local speech-to-text via Whisper model (`#[cfg(feature = "whisper")]`)
- `cloud_stt.rs`: `CloudSttProvider` — cloud-based STT fallback (`#[cfg(feature = "cloud-stt")]`)
- `model_downloader.rs`: Whisper model download support (`#[cfg(feature = "download")]`)

### oneshim-sandbox-worker (Sandboxed Automation Executor)
- `main.rs`: Out-of-process action executor. Spawned by the parent `src-tauri` with platform sandbox constraints (Job Object on Windows, seccomp+Landlock on Linux, App Sandbox on macOS) already applied. Reads a `SandboxRequest` JSON from stdin, runs the `AutomationAction` via `oneshim-core` models, writes a `SandboxResponse` JSON to stdout. Keeps the main process isolated from action-side crashes and containment failures. Binary target: `oneshim-sandbox-worker`.

### oneshim-app (formerly crates/oneshim-app/) — REMOVED
> The `crates/oneshim-app/` directory no longer exists in the workspace.
> Replaced by `src-tauri/` which is the active main binary crate (its package
> name is still `oneshim-app` for external build-script compatibility).
>
> Legacy modules (scheduler, focus_analyzer, updater, lifecycle, etc.) have been
> migrated into `src-tauri/src/`. Any reference to `crates/oneshim-app/*` in older
> docs or commit messages is historical.

## Key Dependencies

| Category | Crate | Version |
|----------|-------|---------|
| Runtime | tokio | 1 (full) |
| HTTP | reqwest | 0.13 |
| Web Server | axum + tower-http | 0.8 / 0.6 |
| SSE | eventsource-stream | 0.2 |
| Integration Transport | tokio-tungstenite | 0.28 |
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
- Logging: `tracing` macros (`debug!`, `info!`, `warn!`, `error!`). When logging a `CoreError`, include the wire code as a structured field so Loki/Grafana/OTel can group by `err.code` without regex-matching the Display body: `warn!(err.code = %e.code(), "failed: {e}")`. For adapter errors without a `code()` method, convert first: `let core: CoreError = e.into(); warn!(err.code = %core.code(), ...)`.  See ADR-019 Follow-up #2 for the observability rationale.
- Serialization: `serde` derive — `Serialize, Deserialize` for all models

## Architecture Guardrails

Rules to prevent known issues from recurring. Enforced by code review.

### AppState Sub-Structs

`AppState` fields are grouped into sub-structs by concern. When adding new fields:
- Capture-related → `CaptureContext`
- Connection status → `ConnectionStatus`
- Create new sub-struct if 3+ related fields would otherwise be top-level

### Monitor Loop Complexity

`spawn_monitor_loop` in `scheduler/loops/monitor.rs` must stay under 500 lines. When adding functionality, extract into a helper function in the `loops/` directory (like `coaching_helper.rs`, `tracking_schedule_helper.rs`).

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
- [ADR-019: Error Code Infrastructure + AWS Bedrock Intentional Non-Support](docs/architecture/ADR-019-error-code-infrastructure.md) ([한국어](docs/architecture/ADR-019-error-code-infrastructure.ko.md)) — typed `code` field on every struct-variant of `CoreError`/`GuiInteractionError` (`#[from]` variants derive code via `impl code()` per §7); Bedrock deleted from catalog; re-introduction requires §5 8-step checklist
- [HTTP Status Error Mapping Pattern](docs/guides/http-status-error-mapping.md) ([한국어](docs/guides/http-status-error-mapping.ko.md)) — canonical 401/403/404/408/429/502/503/504 → wire code table applied across 16 HTTP dispatchers (original 14; iter-98 added `auth::refresh`; iter-194 added `sync/lan_transport::authenticate_with_peer` per Follow-up #5; `oneshim-web::services::ai_model_catalog_web_service` is the 16th); follow this pattern when adding new HTTP call sites
- [Documentation Policy](docs/DOCUMENTATION_POLICY.md) — English-primary + Korean companion docs + metrics consistency rules
- [Project Status](docs/STATUS.md) — single source of truth for mutable quality metrics
- [Migration Overview](docs/migration/README.md) — Migration plans and history
- [Server API](docs/migration/04-server-api.md) — ~30 REST endpoints + gRPC RPCs (auth×5, sessions×6, messages×3, suggestions×6, context×4, telemetry/sync×2, health×4 per section headers)
- [Migration Phases](docs/migration/05-migration-phases.md) — Phase 0-36 plans
- [Edge Vision](docs/migration/legacy/08-edge-vision.md) — Image processing details
- [gRPC Client Guide](docs/guides/grpc-client.md) — Rust gRPC client usage
- [Contributing Guide](CONTRIBUTING.md) — Rust development guide
- [Code of Conduct](CODE_OF_CONDUCT.md) — Contributor Covenant v2.1
- [Security Policy](SECURITY.md) — Vulnerability reporting process

## Current Status

- Phase 0-35 + Privacy & Permission Control System + Superpowers-era features completed (14 crates)
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
