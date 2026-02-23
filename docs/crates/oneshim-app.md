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

## Key Components

### main.rs

Application entry point and DI assembly:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // 2. Load configuration
    let config = AppConfig::load()?;

    // 3. Create components (DI wiring)
    let token_manager = Arc::new(TokenManager::new(
        &config.server.base_url,
        &std::env::var("ONESHIM_EMAIL")?,
        &std::env::var("ONESHIM_PASSWORD")?,
    ));

    let compressor = Arc::new(AdaptiveCompressor::default());
    let api_client = Arc::new(HttpApiClient::new(
        &config.server.base_url,
        token_manager.clone(),
        compressor.clone(),
    ));

    let sse_client = Arc::new(SseStreamClient::new(
        &config.server.base_url,
        token_manager.clone(),
        config.server.sse_max_retry_secs,
    ));

    let storage = Arc::new(SqliteStorage::new(&config.storage.db_path)?);
    let system_monitor = Arc::new(SysInfoMonitor::new());
    let process_monitor = Arc::new(ProcessTracker::new());
    let activity_monitor = Arc::new(ActivityTracker::new(300));

    let capture_trigger = Arc::new(SmartCaptureTrigger::new(
        config.vision.capture_throttle_ms,
    ));
    let frame_processor = Arc::new(EdgeFrameProcessor::new(/* ... */));

    let notifier = Arc::new(DesktopNotifierImpl);
    let suggestion_queue = Arc::new(PriorityQueue::new(50));
    let suggestion_history = Arc::new(SuggestionHistory::new(100));

    // 4. Create scheduler
    let scheduler = Scheduler::new(/* inject all components */);

    // 5. Set up lifecycle
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let lifecycle = Lifecycle::new(shutdown_tx);

    // 6. Start tasks
    let monitor_task = tokio::spawn(scheduler.run_monitor_loop(shutdown_rx.clone()));
    let sync_task = tokio::spawn(scheduler.run_sync_loop(shutdown_rx.clone()));
    let sse_task = tokio::spawn(/* SSE connection */);

    // 7. Wait for shutdown signal
    lifecycle.wait_for_shutdown().await;

    // 8. Cleanup
    monitor_task.abort();
    sync_task.abort();
    sse_task.abort();

    Ok(())
}
```

### Scheduler (scheduler.rs)

Three periodic task loops:

```rust
pub struct Scheduler {
    system_monitor: Arc<dyn SystemMonitor>,
    process_monitor: Arc<dyn ProcessMonitor>,
    activity_monitor: Arc<dyn ActivityMonitor>,
    capture_trigger: Arc<dyn CaptureTrigger>,
    frame_processor: Arc<dyn FrameProcessor>,
    api_client: Arc<dyn ApiClient>,
    storage: Arc<dyn StorageService>,
    batch_uploader: Arc<BatchUploader>,
    config: MonitorConfig,
}

impl Scheduler {
    /// Monitoring loop (1-second interval)
    pub async fn run_monitor_loop(&self, mut shutdown: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(
            Duration::from_millis(self.config.poll_interval_ms)
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.monitor_tick().await {
                        warn!("Monitoring error: {}", e);
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("Monitoring loop terminated");
                        break;
                    }
                }
            }
        }
    }

    async fn monitor_tick(&self) -> Result<(), CoreError> {
        // 1. Collect system metrics
        let metrics = self.system_monitor.get_metrics().await?;

        // 2. Check active window
        let window = self.process_monitor.get_active_window().await?;

        // 3. Create event
        let event = create_context_event(window, metrics, self.activity_monitor.is_idle().await?);

        // 4. Store locally
        self.storage.save_event(&event).await?;

        // 5. Capture decision
        if let CaptureDecision::Capture { importance } = self.capture_trigger.should_capture(&event).await? {
            let frame = ScreenCapture::capture_active_window()?;
            let processed = self.frame_processor.process(frame).await?;
            self.storage.save_frame(&processed).await?;
            self.batch_uploader.queue_frame(processed).await;
        }

        // 6. Add to batch queue
        self.batch_uploader.queue_event(event).await;

        Ok(())
    }

    /// Sync loop (10-second interval)
    pub async fn run_sync_loop(&self, mut shutdown: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(
            Duration::from_millis(self.config.sync_interval_ms)
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.batch_uploader.flush().await {
                        warn!("Sync error: {}", e);
                    }
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    /// Heartbeat loop (30-second interval)
    pub async fn run_heartbeat_loop(&self, mut shutdown: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(
            Duration::from_millis(self.config.heartbeat_interval_ms)
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Send server heartbeat
                }
                _ = shutdown.changed() => break,
            }
        }
    }
}
```

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

Auto update based on GitHub Releases:

```rust
pub struct Updater {
    config: UpdateConfig,
    http_client: reqwest::Client,
}

impl Updater {
    pub async fn check_for_update(&self) -> Result<Option<Release>, CoreError> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.config.repo_owner,
            self.config.repo_name,
        );

        let response: GitHubRelease = self.http_client
            .get(&url)
            .header("User-Agent", "ONESHIM-Client")
            .send()
            .await?
            .json()
            .await?;

        let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
        let latest = semver::Version::parse(&response.tag_name.trim_start_matches('v'))?;

        if latest > current {
            Ok(Some(Release {
                version: latest,
                download_url: Self::find_asset_url(&response)?,
                release_notes: response.body,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn download_and_install(&self, release: &Release) -> Result<(), CoreError> {
        info!("Downloading update: {}", release.version);

        // 1. Download asset
        let bytes = self.http_client
            .get(&release.download_url)
            .send()
            .await?
            .bytes()
            .await?;

        // 2. Extract to temporary directory
        let temp_dir = tempfile::tempdir()?;
        Self::extract_archive(&bytes, temp_dir.path())?;

        // 3. Replace binary (using self_update)
        self_update::Move::from_source(temp_dir.path().join("oneshim"))
            .to_dest(&std::env::current_exe()?)?;

        info!("Update complete. Restart required.");
        Ok(())
    }

    fn find_asset_url(release: &GitHubRelease) -> Result<String, CoreError> {
        let pattern = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "macos-arm64",
            ("macos", "x86_64") => "macos-x64",
            ("windows", _) => "windows-x64",
            ("linux", _) => "linux-x64",
            _ => return Err(CoreError::Internal("Unsupported platform".into())),
        };

        release.assets
            .iter()
            .find(|a| a.name.contains(pattern))
            .map(|a| a.browser_download_url.clone())
            .ok_or_else(|| CoreError::Internal("Asset not found".into()))
    }
}
```

## Execution Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                         main.rs                                  │
│                                                                  │
│  1. Initialize logging                                           │
│  2. Load configuration                                           │
│  3. Create components (DI)                                       │
│  4. Start scheduler                                              │
│  5. SSE connection                                               │
│  6. Wait for shutdown signal                                     │
└─────────────────────────────────────────────────────────────────┘
        │
        ├─────────────────┬─────────────────┬─────────────────┐
        ▼                 ▼                 ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐ ┌───────────┐
│ Monitor Loop  │ │  Sync Loop    │ │ Heartbeat Loop│ │ SSE Task  │
│   (1s)        │ │   (10s)       │ │   (30s)       │ │           │
└───────────────┘ └───────────────┘ └───────────────┘ └───────────┘
        │                 │                 │                 │
        │                 │                 │                 │
        └─────────────────┴─────────────────┴─────────────────┘
                                    │
                                    ▼
                          ┌─────────────────┐
                          │ Graceful Shutdown│
                          │ (Ctrl+C/SIGTERM) │
                          └─────────────────┘
```

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
#[tokio::test]
async fn test_lifecycle_shutdown() {
    let (tx, rx) = watch::channel(false);
    let lifecycle = Lifecycle::new(tx);

    // Send shutdown signal
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        // In practice, simulates Ctrl+C
    });

    // wait_for_shutdown test requires signal mocking
}

#[test]
fn test_platform_detection() {
    let pattern = Updater::get_platform_pattern();
    #[cfg(target_os = "macos")]
    assert!(pattern.contains("macos"));
    #[cfg(target_os = "windows")]
    assert!(pattern.contains("windows"));
}
```
