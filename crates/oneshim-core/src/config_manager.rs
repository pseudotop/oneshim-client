//! 설정 파일 관리.
//!
//! 플랫폼별 설정 디렉토리에 JSON 파일로 설정을 저장/로드한다.

use crate::config::AppConfig;
use crate::error::CoreError;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// 설정 파일 이름
const CONFIG_FILE_NAME: &str = "config.json";

/// 앱 디렉토리 이름
const APP_DIR_NAME: &str = "oneshim";

/// 설정 관리자
///
/// 설정 파일의 로드/저장 및 런타임 설정 변경을 관리한다.
#[derive(Debug, Clone)]
pub struct ConfigManager {
    /// 현재 설정 (스레드 안전)
    config: Arc<RwLock<AppConfig>>,
    /// 설정 파일 경로
    config_path: PathBuf,
}

impl ConfigManager {
    /// 새 설정 관리자 생성 및 설정 로드
    ///
    /// 설정 파일이 없으면 기본 설정을 생성하고 저장한다.
    pub fn new() -> Result<Self, CoreError> {
        let config_path = Self::default_config_path()?;
        Self::with_path(config_path)
    }

    /// 지정된 경로로 설정 관리자 생성
    pub fn with_path(config_path: PathBuf) -> Result<Self, CoreError> {
        // 설정 디렉토리 생성
        if let Some(parent) = config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    CoreError::Config(format!(
                        "설정 디렉토리 생성 실패: {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
                info!("설정 디렉토리 생성: {}", parent.display());
            }
        }

        // 설정 파일 로드 또는 기본값 생성
        let config = if config_path.exists() {
            Self::load_from_file(&config_path)?
        } else {
            let default_config = AppConfig::default_config();
            Self::save_to_file(&config_path, &default_config)?;
            info!("기본 설정 파일 생성: {}", config_path.display());
            default_config
        };

        Ok(Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        })
    }

    /// 현재 설정 반환 (복제본)
    pub fn get(&self) -> AppConfig {
        self.config.read().unwrap().clone()
    }

    /// 설정 업데이트 및 파일 저장
    pub fn update(&self, new_config: AppConfig) -> Result<(), CoreError> {
        // 메모리 업데이트
        {
            let mut config = self.config.write().unwrap();
            *config = new_config.clone();
        }

        // 파일 저장
        Self::save_to_file(&self.config_path, &new_config)?;
        debug!("설정 저장 완료: {}", self.config_path.display());

        Ok(())
    }

    /// 특정 필드만 업데이트
    pub fn update_with<F>(&self, updater: F) -> Result<AppConfig, CoreError>
    where
        F: FnOnce(&mut AppConfig),
    {
        let mut config = self.get();
        updater(&mut config);
        self.update(config.clone())?;
        Ok(config)
    }

    /// 설정 파일 경로 반환
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    /// 설정 다시 로드
    pub fn reload(&self) -> Result<(), CoreError> {
        let config = Self::load_from_file(&self.config_path)?;
        let mut current = self.config.write().unwrap();
        *current = config;
        info!("설정 다시 로드 완료");
        Ok(())
    }

    /// 플랫폼별 기본 설정 파일 경로
    fn default_config_path() -> Result<PathBuf, CoreError> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join(CONFIG_FILE_NAME))
    }

    /// 플랫폼별 설정 디렉토리 경로
    pub fn config_dir() -> Result<PathBuf, CoreError> {
        #[cfg(target_os = "macos")]
        {
            // macOS: ~/Library/Application Support/oneshim/
            let home = std::env::var("HOME")
                .map_err(|_| CoreError::Config("HOME 환경 변수를 찾을 수 없습니다".to_string()))?;
            Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_DIR_NAME))
        }

        #[cfg(target_os = "windows")]
        {
            // Windows: %APPDATA%\oneshim\
            let appdata = std::env::var("APPDATA").map_err(|_| {
                CoreError::Config("APPDATA 환경 변수를 찾을 수 없습니다".to_string())
            })?;
            Ok(PathBuf::from(appdata).join(APP_DIR_NAME))
        }

        #[cfg(target_os = "linux")]
        {
            // Linux: ~/.config/oneshim/
            let home = std::env::var("HOME")
                .map_err(|_| CoreError::Config("HOME 환경 변수를 찾을 수 없습니다".to_string()))?;
            Ok(PathBuf::from(home).join(".config").join(APP_DIR_NAME))
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            // 기타 플랫폼: 현재 디렉토리
            warn!("지원되지 않는 플랫폼, 현재 디렉토리 사용");
            Ok(PathBuf::from(".").join(APP_DIR_NAME))
        }
    }

    /// 데이터 디렉토리 경로 (DB, 프레임 이미지 등)
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
                CoreError::Config("LOCALAPPDATA 환경 변수를 찾을 수 없습니다".to_string())
            })?;
            Ok(PathBuf::from(local_appdata).join(APP_DIR_NAME).join("data"))
        }

        #[cfg(target_os = "linux")]
        {
            // Linux: ~/.local/share/oneshim/
            let home = std::env::var("HOME")
                .map_err(|_| CoreError::Config("HOME 환경 변수를 찾을 수 없습니다".to_string()))?;
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

    /// 파일에서 설정 로드
    fn load_from_file(path: &PathBuf) -> Result<AppConfig, CoreError> {
        let content = fs::read_to_string(path).map_err(|e| {
            CoreError::Config(format!("설정 파일 읽기 실패: {}: {}", path.display(), e))
        })?;

        let config: AppConfig = serde_json::from_str(&content).map_err(|e| {
            CoreError::Config(format!("설정 파일 파싱 실패: {}: {}", path.display(), e))
        })?;

        debug!("설정 파일 로드 완료: {}", path.display());
        Ok(config)
    }

    /// 파일에 설정 저장
    fn save_to_file(path: &PathBuf, config: &AppConfig) -> Result<(), CoreError> {
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| CoreError::Config(format!("설정 직렬화 실패: {}", e)))?;

        fs::write(path, content).map_err(|e| {
            CoreError::Config(format!("설정 파일 저장 실패: {}: {}", path.display(), e))
        })?;

        Ok(())
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new().expect("기본 설정 관리자 생성 실패")
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

        // 새 관리자 생성 (기본 설정 파일 생성됨)
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

        // 설정 변경
        manager
            .update_with(|c| {
                c.web.port = 8080;
                c.storage.retention_days = 60;
            })
            .unwrap();

        // 새 관리자로 다시 로드
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

        // 파일 직접 수정
        let mut config = manager.get();
        config.web.port = 7777;
        let content = serde_json::to_string_pretty(&config).unwrap();
        fs::write(&config_path, content).unwrap();

        // 리로드
        manager.reload().unwrap();
        assert_eq!(manager.get().web.port, 7777);
    }

    #[test]
    fn config_dir_exists() {
        // 플랫폼별 디렉토리 경로가 유효한지 확인
        let config_dir = ConfigManager::config_dir();
        assert!(config_dir.is_ok());

        let data_dir = ConfigManager::data_dir();
        assert!(data_dir.is_ok());
    }
}
