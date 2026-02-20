# Crate Implementation Docs

Detailed implementation reference for the ONESHIM Rust client's 10-crate workspace.

## Crate Dependency Graph

```
┌─────────────────────────────────────────────────────────────────┐
│                    oneshim-app (binary entry)                  │
│                 DI wiring, scheduler, lifecycle                │
└─────────────────────────────────────────────────────────────────┘
        │
        ├───────────┬───────────┬───────────┬───────────┬─────────┐
        ▼           ▼           ▼           ▼           ▼         ▼
┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌────────────┐
│  network  │ │suggestion │ │  storage  │ │  monitor  │ │   vision  │ │ automation │
│ HTTP/SSE  │ │ queueing  │ │  SQLite   │ │ telemetry │ │ edge proc │ │ policy exec│
│ gRPC/WS   │ │ feedback  │ │  WAL mode │ │ activity  │ │ PII filter│ │ sandbox    │
└───────────┘ └───────────┘ └───────────┘ └───────────┘ └───────────┘ └────────────┘
        │           │                                         │           │
        └─────┬─────┘                                         │           │
              ▼                                               ▼           │
       ┌───────────┐       ┌───────────┐                ┌───────────┐    │
       │    web    │       │    ui     │                │    ui     │    │
       │ REST API  │       │ desktop UI│◀───────────────│           │    │
       │ React FE  │       │ tray/menu │                └───────────┘    │
       └───────────┘       └───────────┘                                  │
              │                   │                                       │
              └───────┬──────────┘                                        │
                      ▼                                                   │
┌─────────────────────────────────────────────────────────────────────────┘
│                     oneshim-core (foundation)                           │
│          domain models, port interfaces, errors, configuration          │
└──────────────────────────────────────────────────────────────────────────┘
```

## Crate List

| Crate | Role | Key Implementations | Docs |
|-------|------|---------------------|------|
| **oneshim-core** | Foundation layer | Models, ports, errors, config | [Details](./oneshim-core.md) |
| **oneshim-network** | Network adapter | HTTP, SSE, WebSocket, compression, auth, gRPC, AI OCR/LLM clients | [Details](./oneshim-network.md) |
| **oneshim-vision** | Edge image processing | Capture, delta, WebP, OCR, privacy filter, Privacy Gateway | [Details](./oneshim-vision.md) |
| **oneshim-monitor** | System monitoring | CPU/memory/disk, active windows, idle detection, input activity | [Details](./oneshim-monitor.md) |
| **oneshim-storage** | Local storage | SQLite, migrations, retention policy, edge intelligence | [Details](./oneshim-storage.md) |
| **oneshim-suggestion** | Suggestion pipeline | Receive, priority queue, feedback, history | [Details](./oneshim-suggestion.md) |
| **oneshim-ui** | Desktop UI | System tray, notifications, main window, theme, automation toggle | [Details](./oneshim-ui.md) |
| **oneshim-web** | Local web dashboard | Axum REST API, React frontend, SSE | [Details](./oneshim-web.md) |
| **oneshim-automation** | Automation control | Policy-based execution, audit logging, OS sandbox, intent resolution | [Details](./oneshim-automation.md) |
| **oneshim-app** | Binary entry point | DI, 9-loop scheduler, FocusAnalyzer, auto-update | [Details](./oneshim-app.md) |

## Architecture Principles

### Hexagonal Architecture (Ports & Adapters)

- **Core**: `oneshim-core` defines all ports (traits) and domain models.
- **Adapters**: The other 9 crates implement those ports.
- **Dependency Rule**: Adapters depend on core; reverse dependencies are disallowed.

### Cross-Crate Communication Rules

1. No direct adapter-to-adapter imports.
2. All interfaces are expressed through `oneshim-core` traits.
3. Explicit exceptions: `suggestion -> network` (SSE intake), `ui -> suggestion` (display path).

### DI Pattern

- Constructor injection with `Arc<dyn T>`
- No DI framework; manual wiring
- Wiring is handled in `oneshim-app/src/main.rs`

### Two-Layer Automation Action Model

- **AutomationIntent** (server -> client): High-level intent (e.g., ClickElement, TypeIntoElement)
- **AutomationAction** (internal client): Low-level action (e.g., MouseMove, MouseClick, KeyType)
- **IntentResolver**: Converts intent into executable action sequence (with OCR + LLM assistance)

## Main Flows

### Monitoring Flow (1-second interval)

```
SystemMonitor -> ProcessMonitor -> ActivityMonitor
       │              │               │
       └──────────────┴───────────────┘
                      │
                      ▼
               ContextEvent
                      │
          ┌───────────┴───────────┐
          ▼                       ▼
    CaptureTrigger            Storage
          │                       │
          ▼                       │
    FrameProcessor                │
          │                       │
          └───────────┬───────────┘
                      ▼
               BatchUploader
                      │
                      ▼
                    Server
```

### Suggestion Reception Flow

```
Server (SSE) -> SseClient -> SuggestionReceiver -> PriorityQueue
                                    │                 │
                                    ▼                 ▼
                            DesktopNotifier    MainWindow (UI)
                                                      │
                                                      ▼
                                              FeedbackSender
                                                      │
                                                      ▼
                                               Server (REST)
```

### Automation Execution Flow

```
Server (AutomationIntent)
          │
          ▼
  AutomationController
          │
    ┌─────┴──────┐
    ▼            ▼
PolicyClient  AuditLogger
(validate)     (record)
    │
    ▼
IntentResolver
    │
    ├── ElementFinder (OCR)
    ├── LlmProvider
    └── PrivacyGateway
          │
          ▼
  AutomationAction[]
          │
          ▼
    ┌─────┴──────┐
    ▼            ▼
InputDriver   Sandbox
(execute)     (isolate)
```

## Test and Quality Status

To avoid drift across documents, mutable quality metrics are centralized in:

- [docs/STATUS.md](../STATUS.md)

This file intentionally avoids hard-coded totals for test counts, warning counts, and pass/fail status.

## References

- [ADR-001: Rust Client Architecture Patterns](../architecture/ADR-001-rust-client-architecture-patterns.md)
- [Migration Overview](../migration/README.md)
- [CLAUDE.md](../../CLAUDE.md) - Development guide
- [Contributing Guide](../../CONTRIBUTING.md)
- [Code of Conduct](../../CODE_OF_CONDUCT.md)
- [Security Policy](../../SECURITY.md)
