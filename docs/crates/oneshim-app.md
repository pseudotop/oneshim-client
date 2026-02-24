[English](./oneshim-app.md) | [한국어](./oneshim-app.ko.md)

# oneshim-app

The binary entry point. DI wiring, scheduler, and lifecycle management.

## Role

- **Entry Point**: `main()` function, application startup
- **DI Wiring**: Assembly and injection of all components
- **Scheduling**: Periodic task execution
- **Lifecycle**: Startup/shutdown handling
- **Auto Update**: Updates based on GitHub Releases

## Directory Structure

```
oneshim-app/src/
├── main.rs       # Entry point, DI wiring
├── gui_runner.rs # GUI + Agent integrated runtime
├── automation_runtime.rs # AI provider runtime wiring
├── provider_adapters.rs  # AI provider adapter resolution
├── cli_subscription_bridge.rs # CLI subscription bridge artifact sync
├── scheduler.rs  # Scheduler - periodic tasks
├── lifecycle.rs  # Lifecycle - signal handling
├── event_bus.rs  # Internal event routing
├── autostart.rs  # Auto-start configuration
└── updater.rs    # Auto update
```

## CLI Subscription Bridge

When `ai_provider.access_mode` is `ProviderSubscriptionCli`, ONESHIM can sync bridge command files for external AI CLIs.

- Enable auto-install: `ONESHIM_CLI_BRIDGE_AUTOINSTALL=1`
- Include user-scope directories (`~/.codex`, `~/.claude`, `~/.gemini`): `ONESHIM_CLI_BRIDGE_INCLUDE_USER_SCOPE=1`
- Default context export reference path in generated bridge files: `<data_dir>/exports/oneshim-context.json`

Reference: `docs/architecture/cli-subscription-bridge-research.md`

## AI Provider Adapters (`provider_adapters.rs`)

`resolve_ai_provider_adapters()` resolves OCR/LLM providers from `AiProviderConfig` and returns source metadata:

- Access modes:
  - `LocalModel`
  - `ProviderApiKey`
  - `ProviderSubscriptionCli`
  - `PlatformConnected`
- Fallback behavior:
  - Remote init failure can fall back to local providers when `fallback_to_local=true`
- OCR privacy gate:
  - Remote OCR calls are wrapped by `GuardedOcrProvider`
  - Pre-flight sanitization through `PrivacyGateway::sanitize_image_for_external_policy()`
  - Optional opt-out for raw remote OCR via `allow_unredacted_external_ocr`
  - Post-parse calibration validation (`OcrValidationConfig`)

## Key Components

### main.rs

Application entry point and DI assembly:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. parse CLI args + init logging + load AppConfig
    // 2. run integrity preflight, evaluate AI access mode
    // 3. build storage/monitor/network/vision/automation components
    // 4. resolve provider adapters + optional CLI bridge sync
    // 5. start scheduler (9-loop orchestration)
    // 6. start web server + optional update coordinator
    // 7. wait shutdown signal and finalize session
    Ok(())
}
```

### Scheduler (scheduler.rs)

Current scheduler is a **9-loop orchestrator** (`Scheduler::run()` → `run_scheduler_loops()`), not a 3-loop model.

| Loop | Interval | Responsibility |
|------|----------|----------------|
| Monitor | 1s | Context collection, idle transitions, capture trigger, frame processing, event persistence, optional upload enqueue |
| Metrics | 5s | System metrics persistence + dashboard realtime broadcast + notification high-usage checks |
| Process Snapshot | 10s | Top process snapshot persistence |
| Sync | 10s | Batched upload (platform-connected mode only) + retention enforcement |
| Heartbeat | 30s | Server heartbeat in connected mode |
| Aggregation | 1h | Hourly aggregation + metrics/process/idle cleanup |
| Notification | 1m | Long-session notification checks |
| Focus | 1m | `FocusAnalyzer::analyze_periodic()` and suggestion generation |
| Event Snapshot | 30s | Detailed process snapshot + `InputActivityCollector` snapshot events |

Key scheduler boundaries:

- Storage boundary: `SchedulerStorage` port (extends metrics storage with frame metadata write path)
- Privacy/upload boundary: `PlatformEgressPolicy` decides sanitization and upload eligibility
- Frame retention: `FrameFileStorage` retention + storage-limit enforcement in sync loop
- Optional subsystems: `NotificationManager` and `FocusAnalyzer` are injected and run conditionally

### Lifecycle (lifecycle.rs)

Signal handling and graceful shutdown:

```rust
pub struct Lifecycle {
    shutdown_tx: watch::Sender<bool>,
}

impl Lifecycle {
    pub fn new(shutdown_tx: watch::Sender<bool>) -> Self {
        Self { shutdown_tx }
    }

    pub async fn wait_for_shutdown(&self) {
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => info!("Ctrl+C received"),
            _ = terminate => info!("SIGTERM received"),
        }

        info!("Starting shutdown...");
        let _ = self.shutdown_tx.send(true);
    }
}
```

### EventBus (event_bus.rs)

Internal event routing:

```rust
pub struct EventBus {
    sender: broadcast::Sender<InternalEvent>,
}

#[derive(Clone, Debug)]
pub enum InternalEvent {
    NewSuggestion(Suggestion),
    ConnectionStatusChanged(ConnectionStatus),
    SyncCompleted { events: usize, frames: usize },
    ErrorOccurred(String),
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(100);
        Self { sender }
    }

    pub fn publish(&self, event: InternalEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<InternalEvent> {
        self.sender.subscribe()
    }
}
```

### Autostart (autostart.rs)

Auto-start at login configuration:

```rust
pub struct Autostart;

impl Autostart {
    #[cfg(target_os = "macos")]
    pub fn enable() -> Result<(), CoreError> {
        let plist = Self::generate_launchagent_plist()?;
        let path = dirs::home_dir()
            .unwrap()
            .join("Library/LaunchAgents/com.oneshim.client.plist");
        std::fs::write(&path, plist)?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub fn disable() -> Result<(), CoreError> {
        let path = dirs::home_dir()
            .unwrap()
            .join("Library/LaunchAgents/com.oneshim.client.plist");
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub fn enable() -> Result<(), CoreError> {
        use windows_sys::Win32::System::Registry::*;

        let exe_path = std::env::current_exe()?;
        let key_path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

        unsafe {
            let mut hkey: HKEY = std::ptr::null_mut();
            RegOpenKeyExW(HKEY_CURRENT_USER, /* ... */)?;
            RegSetValueExW(hkey, "ONESHIM", /* exe_path */)?;
            RegCloseKey(hkey);
        }
        Ok(())
    }
}
```

### Updater (updater.rs)

Auto update flow is driven by `Updater` + `update_coordinator`:

- Source of truth: `https://api.github.com/repos/{owner}/{repo}/releases/latest`
- Version policy:
  - compares semver against `CURRENT_VERSION`
  - supports prerelease filtering (`include_prerelease`)
  - enforces minimum version floor
- Asset policy:
  - selects platform-specific asset by OS/arch pattern
  - validates download host allowlist before fetch
- Integrity policy:
  - always verifies `SHA-256` checksum (`.sha256`)
  - optionally enforces Ed25519 signature verification (`require_signature_verification=true`, `update.signature_public_key`)
- Install:
  - archive extraction with path traversal guards
  - binary replacement via `self_update::self_replace`

### Installer/Release Packaging Link

Release artifacts are packaged by `.github/workflows/release.yml`, then consumed by:

- App updater (`crates/oneshim-app/src/updater.rs`) for in-app updates
- Cross-platform terminal installers:
  - `scripts/install.sh`
  - `scripts/install.ps1`
  - `scripts/uninstall.sh`
  - `scripts/uninstall.ps1`

The installers and updater share the same release assets/checksum/signature sidecars, so integrity behavior is consistent across install and in-app upgrade paths.

## Execution Flow

1. `main.rs` / `gui_runner.rs` loads config and wires DI (storage, monitor, network, vision, automation, web).
2. Access mode is evaluated (`LocalModel`, `ProviderApiKey`, `ProviderSubscriptionCli`, `PlatformConnected`) and AI adapters are resolved.
3. Optional CLI subscription bridge artifacts are synced in subscription mode.
4. Scheduler starts 9 loops and optional subsystems (notification/focus/realtime).
5. Web server serves API + embedded frontend and consumes shared realtime events.
6. Update coordinator checks release channel and handles gated install actions.
7. Shutdown signal triggers graceful stop and session finalization.

## Dependencies

- `anyhow`: Binary error handling
- `tokio`: Async runtime
- `tracing-subscriber`: Logging
- `config`: Config file parsing
- `directories`: Platform-specific directories
- `self_update`: Binary update
- `semver`: Version comparison

## Build

```bash
# Development build
cargo build -p oneshim-app

# Release build
cargo build --release -p oneshim-app

# Run
cargo run -p oneshim-app
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ONESHIM_EMAIL` | Connected mode only ✅ | Login email (optional in standalone mode) |
| `ONESHIM_PASSWORD` | Connected mode only ✅ | Login password (optional in standalone mode) |
| `RUST_LOG` | ❌ | Log level (default: `info`) |
| `ONESHIM_CONFIG` | ❌ | Config file path |

## Tests

```rust
#[test]
fn updater_rejects_unknown_download_host() {
    let updater = Updater::new(test_config());
    let result = updater.validate_download_url("https://evil.example.com/file.tar.gz");
    assert!(result.is_err());
}

#[test]
fn verify_signature_accepts_valid_ed25519_signature() {
    // updater signature verification happy-path is covered in updater.rs tests
}
```
