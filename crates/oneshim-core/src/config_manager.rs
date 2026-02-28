use crate::config::AppConfig;
use crate::error::CoreError;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

const CONFIG_FILE_NAME: &str = "config.json";

const APP_DIR_NAME: &str = "oneshim";

#[derive(Debug, Clone)]
pub struct ConfigManager {
    config: Arc<RwLock<AppConfig>>,
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

        let config = if config_path.exists() {
            Self::load_from_file(&config_path)?
        } else {
            let default_config = AppConfig::default_config();
            Self::save_to_file(&config_path, &default_config)?;
            info!("default settings file create: {}", config_path.display());
            default_config
        };

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        })
    }

    pub fn get(&self) -> AppConfig {
        self.config.read().unwrap().clone()
    }

    pub fn update(&self, new_config: AppConfig) -> Result<(), CoreError> {
        {
            let mut config = self.config.write().unwrap();
            *config = new_config.clone();
        }

        Self::save_to_file(&self.config_path, &new_config)?;
        debug!("settings save complete: {}", self.config_path.display());

        Ok(())
    }

    pub fn update_with<F>(&self, updater: F) -> Result<AppConfig, CoreError>
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut config = self.get();
        updater(&mut config);
        self.update(config.clone())?;
        Ok(config)
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub fn reload(&self) -> Result<(), CoreError> {
        let config = Self::load_from_file(&self.config_path)?;
        let mut current = self.config.write().unwrap();
        *current = config;
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

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default config manager")
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
        assert_eq!(config.web.port, 9090);
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
}
