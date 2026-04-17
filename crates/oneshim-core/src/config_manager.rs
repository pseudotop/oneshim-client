use crate::config::AppConfig;
use crate::error::CoreError;
use parking_lot::Mutex;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{debug, info, warn};

const CONFIG_FILE_NAME: &str = "config.json";

const APP_DIR_NAME: &str = "oneshim";

/// Configuration store with a `watch`-backed broadcast bus.
///
/// The source of truth is `inner.sender.borrow()`. Writers go through
/// `update`, `update_with`, or `reload`, each of which serialises on
/// `inner.writer_lock` and then calls `send_replace`. `subscribe()` /
/// `snapshot()` are zero-cost reads.
///
/// `Clone` is cheap: clones share `Arc<Inner>`. The `writer_lock` is therefore
/// process-wide (all clones contend on the same mutex), which matches the
/// previous `Arc<RwLock<AppConfig>>` semantics.
#[derive(Debug, Clone)]
pub struct ConfigManager {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// Broadcast + source of truth. `borrow()` is cheap.
    sender: watch::Sender<Arc<AppConfig>>,
    /// Linearises concurrent writers across the (non-atomic) compute-new →
    /// persist → send_replace sequence. Held briefly, never across `.await`.
    writer_lock: Mutex<()>,
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self, CoreError> {
        let config_path = Self::default_config_path()?;
        Self::with_path(config_path)
    }

    pub fn with_path(config_path: PathBuf) -> Result<Self, CoreError> {
        if let Some(parent) = config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    CoreError::Config(format!(
                        "Failed to create config directory: {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
                info!("settings create: {}", parent.display());
            }
        }

        let initial = if config_path.exists() {
            match Self::load_from_file(&config_path) {
                Ok(c) => c,
                Err(e) => {
                    warn!(
                        path = %config_path.display(),
                        error = %e,
                        "config file corrupted, falling back to defaults"
                    );
                    let default_config = AppConfig::default_config();
                    // Overwrite the corrupt file so the next launch is clean.
                    if let Err(e) = Self::save_to_file(&config_path, &default_config) {
                        debug!("save_to_file failed: {e}");
                    }
                    default_config
                }
            }
        } else {
            let default_config = AppConfig::default_config();
            Self::save_to_file(&config_path, &default_config)?;
            info!("default settings file create: {}", config_path.display());
            default_config
        };

        let (sender, _rx) = watch::channel(Arc::new(initial));
        // Dropping `_rx` is fine — `watch::Sender` does not require any receivers
        // to exist. `subscribe()` lazily creates them.

        Ok(Self {
            inner: Arc::new(Inner {
                sender,
                writer_lock: Mutex::new(()),
                config_path,
            }),
        })
    }

    pub fn get(&self) -> AppConfig {
        AppConfig::clone(&self.inner.sender.borrow())
    }

    /// Subscribe to whole-config change notifications.
    ///
    /// The receiver starts at the current config. `changed().await` resolves
    /// after the next `update` / `update_with` / `reload`. Dropping a receiver
    /// does not affect any other subscriber.
    ///
    /// `watch` has latest-wins semantics: rapid mutations may be coalesced and
    /// a subscriber that wakes late will see only the final value, not every
    /// intermediate transition. Consumers whose correctness depends on
    /// observing every transition (audit-log callers, counters) must either
    /// keep a tick-based poll structure OR run every `update` through their
    /// own side-effect channel. See ADR-016 for the audit-coalescing hazard.
    pub fn subscribe(&self) -> watch::Receiver<Arc<AppConfig>> {
        self.inner.sender.subscribe()
    }

    /// Cheap read-only snapshot of the current config.
    ///
    /// Equivalent to `subscribe().borrow().clone()` without registering a
    /// subscriber. Prefer this over `get()` when the caller is happy with an
    /// `Arc<AppConfig>` (no deep clone).
    pub fn snapshot(&self) -> Arc<AppConfig> {
        self.inner.sender.borrow().clone()
    }

    pub fn update(&self, new_config: AppConfig) -> Result<(), CoreError> {
        let _guard = self.inner.writer_lock.lock();
        Self::save_to_file(&self.inner.config_path, &new_config)?;
        self.inner.sender.send_replace(Arc::new(new_config));
        debug!(
            "settings save complete: {}",
            self.inner.config_path.display()
        );
        Ok(())
    }

    /// Atomically read-modify-write the config while holding the writer lock
    /// throughout, preventing TOCTOU races between concurrent callers.
    pub fn update_with<F>(&self, updater: F) -> Result<AppConfig, CoreError>
    where
        F: FnOnce(&mut AppConfig) -> Result<(), String>,
    {
        let _guard = self.inner.writer_lock.lock();
        let mut new_cfg = (**self.inner.sender.borrow()).clone();
        updater(&mut new_cfg).map_err(CoreError::Config)?;
        Self::save_to_file(&self.inner.config_path, &new_cfg)?;
        let snapshot = new_cfg.clone();
        self.inner.sender.send_replace(Arc::new(new_cfg));
        debug!(
            "settings save complete: {}",
            self.inner.config_path.display()
        );
        Ok(snapshot)
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.inner.config_path
    }

    pub fn reload(&self) -> Result<(), CoreError> {
        let _guard = self.inner.writer_lock.lock();
        let reloaded = Self::load_from_file(&self.inner.config_path)?;
        self.inner.sender.send_replace(Arc::new(reloaded));
        info!("settings load complete");
        Ok(())
    }

    fn default_config_path() -> Result<PathBuf, CoreError> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join(CONFIG_FILE_NAME))
    }

    pub fn config_dir() -> Result<PathBuf, CoreError> {
        #[cfg(target_os = "macos")]
        {
            // macOS: ~/Library/Application Support/oneshim/
            let home = std::env::var("HOME").map_err(|_| {
                CoreError::Config("HOME environment variable not found".to_string())
            })?;
            Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_DIR_NAME))
        }

        #[cfg(target_os = "windows")]
        {
            // Windows: %APPDATA%\oneshim\
            let appdata = std::env::var("APPDATA").map_err(|_| {
                CoreError::Config("APPDATA environment variable not found".to_string())
            })?;
            Ok(PathBuf::from(appdata).join(APP_DIR_NAME))
        }

        #[cfg(target_os = "linux")]
        {
            // Linux: ~/.config/oneshim/
            let home = std::env::var("HOME").map_err(|_| {
                CoreError::Config("HOME environment variable not found".to_string())
            })?;
            Ok(PathBuf::from(home).join(".config").join(APP_DIR_NAME))
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            tracing::warn!("Unsupported platform; using current directory as config base");
            Ok(PathBuf::from(".").join(APP_DIR_NAME))
        }
    }

    pub fn data_dir() -> Result<PathBuf, CoreError> {
        #[cfg(target_os = "macos")]
        {
            // macOS: ~/Library/Application Support/oneshim/data/
            Self::config_dir().map(|p| p.join("data"))
        }

        #[cfg(target_os = "windows")]
        {
            // Windows: %LOCALAPPDATA%\oneshim\data\
            let local_appdata = std::env::var("LOCALAPPDATA").map_err(|_| {
                CoreError::Config("LOCALAPPDATA environment variable not found".to_string())
            })?;
            Ok(PathBuf::from(local_appdata).join(APP_DIR_NAME).join("data"))
        }

        #[cfg(target_os = "linux")]
        {
            // Linux: ~/.local/share/oneshim/
            let home = std::env::var("HOME").map_err(|_| {
                CoreError::Config("HOME environment variable not found".to_string())
            })?;
            Ok(PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(APP_DIR_NAME))
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            Ok(PathBuf::from(".").join(APP_DIR_NAME).join("data"))
        }
    }

    fn load_from_file(path: &PathBuf) -> Result<AppConfig, CoreError> {
        let content = fs::read_to_string(path).map_err(|e| {
            CoreError::Config(format!(
                "Failed to read config file: {}: {}",
                path.display(),
                e
            ))
        })?;

        let config: AppConfig = serde_json::from_str(&content).map_err(|e| {
            CoreError::Config(format!(
                "Failed to parse config file: {}: {}",
                path.display(),
                e
            ))
        })?;

        debug!("settings file load complete: {}", path.display());
        Ok(config)
    }

    fn save_to_file(path: &PathBuf, config: &AppConfig) -> Result<(), CoreError> {
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| CoreError::Config(format!("Failed to serialize config: {}", e)))?;

        fs::write(path, content).map_err(|e| {
            CoreError::Config(format!(
                "Failed to write config file: {}: {}",
                path.display(),
                e
            ))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let manager = ConfigManager::with_path(config_path.clone()).unwrap();
        assert!(config_path.exists());

        let config = manager.get();
        assert_eq!(config.web.port, crate::config::DEFAULT_WEB_PORT);
    }

    #[test]
    fn update_and_persist_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let manager = ConfigManager::with_path(config_path.clone()).unwrap();

        manager
            .update_with(|c| {
                c.web.port = 8080;
                c.storage.retention_days = 60;
                Ok(())
            })
            .unwrap();

        let manager2 = ConfigManager::with_path(config_path).unwrap();
        let config = manager2.get();

        assert_eq!(config.web.port, 8080);
        assert_eq!(config.storage.retention_days, 60);
    }

    #[test]
    fn reload_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let manager = ConfigManager::with_path(config_path.clone()).unwrap();

        let mut config = manager.get();
        config.web.port = 7777;
        let content = serde_json::to_string_pretty(&config).unwrap();
        fs::write(&config_path, content).unwrap();

        manager.reload().unwrap();
        assert_eq!(manager.get().web.port, 7777);
    }

    #[test]
    fn config_dir_exists() {
        let config_dir = ConfigManager::config_dir();
        assert!(config_dir.is_ok());

        let data_dir = ConfigManager::data_dir();
        assert!(data_dir.is_ok());
    }

    #[test]
    fn reload_with_corrupted_json_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Create a valid config so the manager initialises successfully.
        let manager = ConfigManager::with_path(config_path.clone()).unwrap();
        assert!(config_path.exists());

        // Overwrite the file with invalid JSON.
        fs::write(&config_path, r#"{"invalid": }"#).unwrap();

        // reload() must propagate the parse error as a CoreError::Config variant.
        let result = manager.reload();
        assert!(
            result.is_err(),
            "reload() should return Err when the config file contains invalid JSON"
        );
        match result.unwrap_err() {
            CoreError::Config(msg) => {
                assert!(
                    msg.contains("Failed to parse config file"),
                    "error message should describe the parse failure, got: {msg}"
                );
            }
            other => panic!("expected CoreError::Config, got {other:?}"),
        }

        // The in-memory config must remain unchanged (still the original defaults).
        let config = manager.get();
        assert_eq!(
            config.web.port,
            crate::config::DEFAULT_WEB_PORT,
            "in-memory config should not be mutated after a failed reload"
        );
    }

    // ── Task 27b: ConfigManager file I/O tests ─────────────────────

    #[test]
    fn load_creates_default_when_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent_dir").join("config.json");

        // The file (and parent dir) do not exist yet.
        assert!(!config_path.exists());

        let manager = ConfigManager::with_path(config_path.clone()).unwrap();

        // File should have been created with defaults.
        assert!(
            config_path.exists(),
            "config file should be created on init"
        );

        let config = manager.get();
        assert_eq!(
            config.web.port,
            crate::config::DEFAULT_WEB_PORT,
            "missing file should produce default config"
        );
        assert_eq!(
            config.storage.retention_days, 30,
            "default retention_days should be 30"
        );
    }

    #[test]
    fn save_then_load_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let manager = ConfigManager::with_path(config_path.clone()).unwrap();

        // Mutate several fields across different sections.
        manager
            .update_with(|c| {
                c.web.port = 9999;
                c.storage.retention_days = 90;
                c.server.base_url = "https://prod.example.com".to_string();
                c.monitor.idle_threshold_secs = 600;
                c.vision.ocr_enabled = true;
                Ok(())
            })
            .unwrap();

        // Create a fresh manager pointing at the same file.
        let manager2 = ConfigManager::with_path(config_path).unwrap();
        let loaded = manager2.get();

        assert_eq!(loaded.web.port, 9999);
        assert_eq!(loaded.storage.retention_days, 90);
        assert_eq!(loaded.server.base_url, "https://prod.example.com");
        assert_eq!(loaded.monitor.idle_threshold_secs, 600);
        assert!(loaded.vision.ocr_enabled);
    }

    #[test]
    fn load_corrupt_file_falls_back_to_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Write garbage content before the manager tries to load.
        fs::write(&config_path, "<<<NOT JSON>>>").unwrap();

        // Should NOT crash — falls back to default config.
        let manager = ConfigManager::with_path(config_path.clone()).unwrap();
        let config = manager.get();
        assert_eq!(
            config.web.port,
            crate::config::DEFAULT_WEB_PORT,
            "corrupt config should fall back to defaults"
        );

        // The corrupt file should have been overwritten with valid defaults.
        let reloaded = ConfigManager::with_path(config_path).unwrap();
        assert_eq!(
            reloaded.get().web.port,
            crate::config::DEFAULT_WEB_PORT,
            "overwritten file should contain valid defaults"
        );
    }

    // ── X1 ConfigChangeBus tests ───────────────────────────────────────
    // See docs/reviews/2026-04-17-phase2-config-telemetry-spec.md §2.8.

    /// T-X1-1
    #[test]
    fn subscribe_sees_initial_value() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("config.json");
        let mgr = ConfigManager::with_path(cfg_path).unwrap();

        let rx = mgr.subscribe();
        let via_rx = rx.borrow();
        let via_get = mgr.get();
        // Value equivalence is what matters. The Arc held by rx and the value
        // returned by get() should agree on every field.
        assert_eq!(via_rx.web.port, via_get.web.port);
    }

    /// T-X1-5
    #[tokio::test]
    async fn dropped_receiver_does_not_block_sender() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("config.json");
        let mgr = ConfigManager::with_path(cfg_path).unwrap();

        let rx_a = mgr.subscribe();
        let mut rx_b = mgr.subscribe();
        drop(rx_a);

        // Flip a scalar field so update_with produces a visibly different snapshot.
        mgr.update_with(|c| {
            c.web.port = c.web.port.wrapping_add(1);
            Ok(())
        })
        .expect("update_with must not fail after a receiver drops");

        // The survivor observes the update without deadlocking.
        let _ = rx_b.borrow_and_update();
    }

    /// T-X1-6
    #[test]
    fn snapshot_matches_latest_update() {
        let tmp = TempDir::new().unwrap();
        let cfg_path = tmp.path().join("config.json");
        let mgr = ConfigManager::with_path(cfg_path).unwrap();

        let baseline_port = mgr.get().web.port;
        mgr.update_with(|c| {
            c.web.port = baseline_port.wrapping_add(1);
            Ok(())
        })
        .unwrap();

        let via_snapshot = mgr.snapshot();
        let rx = mgr.subscribe();
        let via_rx = rx.borrow();

        assert_eq!(via_snapshot.web.port, via_rx.web.port);
        assert_eq!(via_snapshot.web.port, baseline_port.wrapping_add(1));
    }
}
