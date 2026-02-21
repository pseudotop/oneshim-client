[English](./oneshim-app.md) | [한국어](./oneshim-app.ko.md)

# oneshim-app

바이너리 진입점. DI 와이어링, 스케줄러, 라이프사이클 관리.

## 역할

- **진입점**: `main()` 함수, 애플리케이션 시작
- **DI 와이어링**: 모든 컴포넌트 조립 및 주입
- **스케줄링**: 주기적 태스크 실행
- **라이프사이클**: 시작/종료 처리
- **자동 업데이트**: GitHub Releases 기반 업데이트

## 디렉토리 구조

```
oneshim-app/src/
├── main.rs       # 진입점, DI 와이어링
├── scheduler.rs  # 스케줄러 - 주기적 태스크
├── lifecycle.rs  # 라이프사이클 - 시그널 처리
├── event_bus.rs  # 내부 이벤트 라우팅
├── autostart.rs  # 자동 시작 설정
└── updater.rs    # 자동 업데이트
```

## 주요 컴포넌트

### main.rs

애플리케이션 진입점 및 DI 조립:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 로깅 초기화
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // 2. 설정 로드
    let config = AppConfig::load()?;

    // 3. 컴포넌트 생성 (DI 와이어링)
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

    // 4. 스케줄러 생성
    let scheduler = Scheduler::new(/* 모든 컴포넌트 주입 */);

    // 5. 라이프사이클 설정
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let lifecycle = Lifecycle::new(shutdown_tx);

    // 6. 태스크 시작
    let monitor_task = tokio::spawn(scheduler.run_monitor_loop(shutdown_rx.clone()));
    let sync_task = tokio::spawn(scheduler.run_sync_loop(shutdown_rx.clone()));
    let sse_task = tokio::spawn(/* SSE 연결 */);

    // 7. 종료 시그널 대기
    lifecycle.wait_for_shutdown().await;

    // 8. 정리
    monitor_task.abort();
    sync_task.abort();
    sse_task.abort();

    Ok(())
}
```

### Scheduler (scheduler.rs)

3개의 주기적 태스크 루프:

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
    /// 모니터링 루프 (1초 간격)
    pub async fn run_monitor_loop(&self, mut shutdown: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(
            Duration::from_millis(self.config.poll_interval_ms)
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.monitor_tick().await {
                        warn!("모니터링 오류: {}", e);
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("모니터링 루프 종료");
                        break;
                    }
                }
            }
        }
    }

    async fn monitor_tick(&self) -> Result<(), CoreError> {
        // 1. 시스템 메트릭 수집
        let metrics = self.system_monitor.get_metrics().await?;

        // 2. 활성 창 확인
        let window = self.process_monitor.get_active_window().await?;

        // 3. 이벤트 생성
        let event = create_context_event(window, metrics, self.activity_monitor.is_idle().await?);

        // 4. 로컬 저장
        self.storage.save_event(&event).await?;

        // 5. 캡처 결정
        if let CaptureDecision::Capture { importance } = self.capture_trigger.should_capture(&event).await? {
            let frame = ScreenCapture::capture_active_window()?;
            let processed = self.frame_processor.process(frame).await?;
            self.storage.save_frame(&processed).await?;
            self.batch_uploader.queue_frame(processed).await;
        }

        // 6. 배치 큐에 추가
        self.batch_uploader.queue_event(event).await;

        Ok(())
    }

    /// 동기화 루프 (10초 간격)
    pub async fn run_sync_loop(&self, mut shutdown: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(
            Duration::from_millis(self.config.sync_interval_ms)
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.batch_uploader.flush().await {
                        warn!("동기화 오류: {}", e);
                    }
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    /// 하트비트 루프 (30초 간격)
    pub async fn run_heartbeat_loop(&self, mut shutdown: watch::Receiver<bool>) {
        let mut interval = tokio::time::interval(
            Duration::from_millis(self.config.heartbeat_interval_ms)
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // 서버 하트비트 전송
                }
                _ = shutdown.changed() => break,
            }
        }
    }
}
```

### Lifecycle (lifecycle.rs)

시그널 처리 및 graceful shutdown:

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
                .expect("Ctrl+C 핸들러 설치 실패");
        };

        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM 핸들러 설치 실패")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => info!("Ctrl+C 수신"),
            _ = terminate => info!("SIGTERM 수신"),
        }

        info!("종료 시작...");
        let _ = self.shutdown_tx.send(true);
    }
}
```

### EventBus (event_bus.rs)

내부 이벤트 라우팅:

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

로그인 시 자동 시작 설정:

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

GitHub Releases 기반 자동 업데이트:

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
        info!("업데이트 다운로드 중: {}", release.version);

        // 1. 에셋 다운로드
        let bytes = self.http_client
            .get(&release.download_url)
            .send()
            .await?
            .bytes()
            .await?;

        // 2. 임시 디렉토리에 압축 해제
        let temp_dir = tempfile::tempdir()?;
        Self::extract_archive(&bytes, temp_dir.path())?;

        // 3. 바이너리 교체 (self_update 사용)
        self_update::Move::from_source(temp_dir.path().join("oneshim"))
            .to_dest(&std::env::current_exe()?)?;

        info!("업데이트 완료. 재시작 필요.");
        Ok(())
    }

    fn find_asset_url(release: &GitHubRelease) -> Result<String, CoreError> {
        let pattern = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "macos-arm64",
            ("macos", "x86_64") => "macos-x64",
            ("windows", _) => "windows-x64",
            ("linux", _) => "linux-x64",
            _ => return Err(CoreError::Internal("지원하지 않는 플랫폼".into())),
        };

        release.assets
            .iter()
            .find(|a| a.name.contains(pattern))
            .map(|a| a.browser_download_url.clone())
            .ok_or_else(|| CoreError::Internal("에셋을 찾을 수 없음".into()))
    }
}
```

## 실행 흐름

```
┌─────────────────────────────────────────────────────────────────┐
│                         main.rs                                  │
│                                                                  │
│  1. 로깅 초기화                                                   │
│  2. 설정 로드                                                     │
│  3. 컴포넌트 생성 (DI)                                            │
│  4. 스케줄러 시작                                                  │
│  5. SSE 연결                                                      │
│  6. 종료 시그널 대기                                               │
└─────────────────────────────────────────────────────────────────┘
        │
        ├─────────────────┬─────────────────┬─────────────────┐
        ▼                 ▼                 ▼                 ▼
┌───────────────┐ ┌───────────────┐ ┌───────────────┐ ┌───────────┐
│ Monitor Loop  │ │  Sync Loop    │ │ Heartbeat Loop│ │ SSE Task  │
│   (1초)       │ │   (10초)      │ │   (30초)      │ │           │
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

## 의존성

- `anyhow`: 바이너리 에러 처리
- `tokio`: 비동기 런타임
- `tracing-subscriber`: 로깅
- `config`: 설정 파일 파싱
- `directories`: 플랫폼별 디렉토리
- `self_update`: 바이너리 업데이트
- `semver`: 버전 비교

## 빌드

```bash
# 개발 빌드
cargo build -p oneshim-app

# 릴리즈 빌드
cargo build --release -p oneshim-app

# 실행
cargo run -p oneshim-app
```

## 환경 변수

| 변수 | 필수 | 설명 |
|------|------|------|
| `ONESHIM_EMAIL` | 연결 모드에서만 ✅ | 로그인 이메일 (standalone 모드에서는 선택) |
| `ONESHIM_PASSWORD` | 연결 모드에서만 ✅ | 로그인 비밀번호 (standalone 모드에서는 선택) |
| `RUST_LOG` | ❌ | 로그 레벨 (기본: `info`) |
| `ONESHIM_CONFIG` | ❌ | 설정 파일 경로 |

## 테스트

```rust
#[tokio::test]
async fn test_lifecycle_shutdown() {
    let (tx, rx) = watch::channel(false);
    let lifecycle = Lifecycle::new(tx);

    // 종료 시그널 전송
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        // 실제로는 Ctrl+C 시뮬레이션
    });

    // wait_for_shutdown 테스트는 시그널 모킹 필요
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
