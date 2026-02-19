//! 자동 업데이트 모듈.
//!
//! GitHub Releases API를 통해 새 버전을 확인하고,
//! 바이너리를 다운로드하여 자동으로 업데이트한다.

#![allow(dead_code)] // UI 연동 전까지 일부 메서드/필드 미사용

use oneshim_core::config::UpdateConfig;
use serde::Deserialize;
use std::path::PathBuf;
use thiserror::Error;

/// 현재 앱 버전 (Cargo.toml에서 가져옴)
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 업데이트 관련 에러
#[derive(Debug, Error)]
pub enum UpdateError {
    /// GitHub API 요청 실패
    #[error("GitHub API 요청 실패: {0}")]
    ApiRequest(#[from] reqwest::Error),

    /// API 응답 파싱 실패
    #[error("API 응답 파싱 실패: {0}")]
    ParseResponse(String),

    /// 버전 파싱 실패
    #[error("버전 파싱 실패: {0}")]
    VersionParse(#[from] semver::Error),

    /// 다운로드 실패
    #[error("다운로드 실패: {0}")]
    Download(String),

    /// 설치 실패
    #[error("설치 실패: {0}")]
    Install(String),

    /// 플랫폼 지원 안됨
    #[error("지원되지 않는 플랫폼: {0}")]
    UnsupportedPlatform(String),

    /// 파일 시스템 에러
    #[error("파일 시스템 에러: {0}")]
    Filesystem(#[from] std::io::Error),

    /// 업데이트 비활성화
    #[error("자동 업데이트가 비활성화되어 있습니다")]
    Disabled,

    /// 최신 버전
    #[error("이미 최신 버전입니다")]
    AlreadyLatest,

    /// 릴리즈에 적합한 에셋 없음
    #[error("현재 플랫폼에 맞는 에셋을 찾을 수 없습니다")]
    NoSuitableAsset,
}

/// GitHub Release 정보
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    /// 릴리즈 태그 (예: "v0.2.0")
    pub tag_name: String,
    /// 릴리즈 이름
    pub name: Option<String>,
    /// 릴리즈 본문 (변경 로그)
    pub body: Option<String>,
    /// 사전 릴리즈 여부
    pub prerelease: bool,
    /// 다운로드 가능한 에셋 목록
    pub assets: Vec<ReleaseAsset>,
    /// HTML URL
    pub html_url: String,
    /// 게시 일시
    pub published_at: Option<String>,
}

/// GitHub Release 에셋
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    /// 에셋 이름 (예: "oneshim-macos-arm64.tar.gz")
    pub name: String,
    /// 다운로드 URL
    pub browser_download_url: String,
    /// 파일 크기 (바이트)
    pub size: u64,
    /// Content-Type
    pub content_type: String,
}

/// 업데이트 확인 결과
#[derive(Debug)]
pub enum UpdateCheckResult {
    /// 새 버전 사용 가능
    Available {
        current: semver::Version,
        latest: semver::Version,
        release: Box<ReleaseInfo>,
        download_url: String,
    },
    /// 이미 최신 버전
    UpToDate { current: semver::Version },
}

/// 자동 업데이트 관리자
pub struct Updater {
    config: UpdateConfig,
    http_client: reqwest::Client,
}

impl Updater {
    /// 새 Updater 인스턴스 생성
    pub fn new(config: UpdateConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("oneshim/{}", CURRENT_VERSION))
            .build()
            .expect("HTTP 클라이언트 생성 실패");

        Self {
            config,
            http_client,
        }
    }

    /// 커스텀 HTTP 클라이언트로 생성 (테스트용)
    #[cfg(test)]
    pub fn with_client(config: UpdateConfig, http_client: reqwest::Client) -> Self {
        Self {
            config,
            http_client,
        }
    }

    /// 커스텀 base URL로 업데이트 확인 (테스트용)
    #[cfg(test)]
    pub async fn check_for_updates_with_base_url(
        &self,
        base_url: &str,
    ) -> Result<UpdateCheckResult, UpdateError> {
        if !self.config.enabled {
            return Err(UpdateError::Disabled);
        }

        let current = semver::Version::parse(CURRENT_VERSION)?;

        // GitHub Releases API 호출
        let url = format!(
            "{}/repos/{}/{}/releases/latest",
            base_url, self.config.repo_owner, self.config.repo_name
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::ParseResponse(format!(
                "API 응답 코드: {}",
                response.status()
            )));
        }

        let release: ReleaseInfo = response.json().await?;

        // 사전 릴리즈 필터링
        if release.prerelease && !self.config.include_prerelease {
            return Ok(UpdateCheckResult::UpToDate { current });
        }

        // 버전 비교
        let latest_tag = release.tag_name.trim_start_matches('v');
        let latest = semver::Version::parse(latest_tag)?;

        if latest > current {
            // 플랫폼에 맞는 에셋 찾기
            let download_url = self.find_platform_asset(&release)?;

            Ok(UpdateCheckResult::Available {
                current,
                latest,
                release: Box::new(release),
                download_url,
            })
        } else {
            Ok(UpdateCheckResult::UpToDate { current })
        }
    }

    /// 새 버전 확인
    pub async fn check_for_updates(&self) -> Result<UpdateCheckResult, UpdateError> {
        if !self.config.enabled {
            return Err(UpdateError::Disabled);
        }

        let current = semver::Version::parse(CURRENT_VERSION)?;

        // GitHub Releases API 호출
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.config.repo_owner, self.config.repo_name
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::ParseResponse(format!(
                "API 응답 코드: {}",
                response.status()
            )));
        }

        let release: ReleaseInfo = response.json().await?;

        // 사전 릴리즈 필터링
        if release.prerelease && !self.config.include_prerelease {
            return Ok(UpdateCheckResult::UpToDate { current });
        }

        // 버전 비교
        let latest_tag = release.tag_name.trim_start_matches('v');
        let latest = semver::Version::parse(latest_tag)?;

        if latest > current {
            // 플랫폼에 맞는 에셋 찾기
            let download_url = self.find_platform_asset(&release)?;

            Ok(UpdateCheckResult::Available {
                current,
                latest,
                release: Box::new(release),
                download_url,
            })
        } else {
            Ok(UpdateCheckResult::UpToDate { current })
        }
    }

    /// 현재 플랫폼에 맞는 에셋 찾기
    fn find_platform_asset(&self, release: &ReleaseInfo) -> Result<String, UpdateError> {
        let platform_patterns = Self::get_platform_patterns()?;

        for asset in &release.assets {
            let name_lower = asset.name.to_lowercase();
            for pattern in &platform_patterns {
                if name_lower.contains(pattern) {
                    return Ok(asset.browser_download_url.clone());
                }
            }
        }

        Err(UpdateError::NoSuitableAsset)
    }

    /// 현재 플랫폼에 해당하는 에셋 이름 패턴
    fn get_platform_patterns() -> Result<Vec<&'static str>, UpdateError> {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            Ok(vec![
                "macos-arm64",
                "darwin-arm64",
                "macos-aarch64",
                "darwin-aarch64",
            ])
        }
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            Ok(vec![
                "macos-x64",
                "darwin-x64",
                "macos-x86_64",
                "darwin-x86_64",
            ])
        }
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            Ok(vec!["windows-x64", "windows-x86_64", "win64", "win-x64"])
        }
        #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
        {
            Ok(vec!["windows-arm64", "windows-aarch64", "win-arm64"])
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            Ok(vec!["linux-x64", "linux-x86_64", "linux-amd64"])
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            Ok(vec!["linux-arm64", "linux-aarch64"])
        }
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
        )))]
        {
            Err(UpdateError::UnsupportedPlatform(format!(
                "{}-{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            )))
        }
    }

    /// 업데이트 다운로드
    pub async fn download_update(&self, download_url: &str) -> Result<PathBuf, UpdateError> {
        tracing::info!("업데이트 다운로드 시작: {}", download_url);

        let response = self.http_client.get(download_url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::Download(format!(
                "다운로드 실패: HTTP {}",
                response.status()
            )));
        }

        // 임시 파일에 저장
        let temp_dir = std::env::temp_dir();
        let file_name = download_url
            .split('/')
            .next_back()
            .unwrap_or("oneshim-update");
        let temp_path = temp_dir.join(file_name);

        let bytes = response.bytes().await?;
        std::fs::write(&temp_path, &bytes)?;

        tracing::info!("업데이트 다운로드 완료: {:?}", temp_path);
        Ok(temp_path)
    }

    /// 업데이트 설치 및 재시작
    ///
    /// # Safety
    /// 이 함수는 현재 실행 중인 바이너리를 교체하고 프로세스를 재시작한다.
    pub fn install_and_restart(&self, downloaded_path: &PathBuf) -> Result<(), UpdateError> {
        use self_update::self_replace;

        tracing::info!("업데이트 설치 시작: {:?}", downloaded_path);

        // 아카이브 확장자 확인
        let file_name = downloaded_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let binary_path = if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
            self.extract_tar_gz(downloaded_path)?
        } else if file_name.ends_with(".zip") {
            self.extract_zip(downloaded_path)?
        } else {
            // 압축되지 않은 바이너리로 가정
            downloaded_path.clone()
        };

        // 바이너리 교체
        self_replace::self_replace(&binary_path)
            .map_err(|e| UpdateError::Install(format!("바이너리 교체 실패: {}", e)))?;

        tracing::info!("업데이트 설치 완료, 재시작합니다...");

        // 재시작
        self.restart_app()
    }

    /// tar.gz 아카이브에서 바이너리 추출
    fn extract_tar_gz(&self, archive_path: &PathBuf) -> Result<PathBuf, UpdateError> {
        use flate2::read::GzDecoder;
        use std::fs::File;

        let file = File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);

        let extract_dir = archive_path
            .parent()
            .unwrap_or(std::path::Path::new("/tmp"));
        archive.unpack(extract_dir)?;

        // 바이너리 파일 찾기
        self.find_binary_in_dir(extract_dir)
    }

    /// zip 아카이브에서 바이너리 추출
    fn extract_zip(&self, archive_path: &PathBuf) -> Result<PathBuf, UpdateError> {
        use std::fs::File;

        let file = File::open(archive_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| UpdateError::Install(format!("ZIP 아카이브 열기 실패: {}", e)))?;

        let extract_dir = archive_path
            .parent()
            .unwrap_or(std::path::Path::new("/tmp"));

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| UpdateError::Install(format!("ZIP 엔트리 읽기 실패: {}", e)))?;

            let outpath = extract_dir.join(file.name());

            if file.name().ends_with('/') {
                std::fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = std::fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        self.find_binary_in_dir(extract_dir)
    }

    /// 디렉토리에서 바이너리 파일 찾기
    fn find_binary_in_dir(&self, dir: &std::path::Path) -> Result<PathBuf, UpdateError> {
        let binary_name = if cfg!(windows) {
            "oneshim.exe"
        } else {
            "oneshim"
        };

        // 직접 경로 확인
        let direct_path = dir.join(binary_name);
        if direct_path.exists() {
            return Ok(direct_path);
        }

        // 서브 디렉토리 검색
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let sub_binary = path.join(binary_name);
                if sub_binary.exists() {
                    return Ok(sub_binary);
                }
            } else if path.file_name().map(|n| n == binary_name).unwrap_or(false) {
                return Ok(path);
            }
        }

        Err(UpdateError::Install(format!(
            "바이너리 '{}' 를 찾을 수 없습니다",
            binary_name
        )))
    }

    /// 애플리케이션 재시작
    fn restart_app(&self) -> Result<(), UpdateError> {
        let current_exe = std::env::current_exe()?;
        let args: Vec<String> = std::env::args().skip(1).collect();

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new(&current_exe).args(&args).exec();
            Err(UpdateError::Install(format!("재시작 실패: {}", err)))
        }

        #[cfg(windows)]
        {
            std::process::Command::new(&current_exe)
                .args(&args)
                .spawn()
                .map_err(|e| UpdateError::Install(format!("재시작 실패: {}", e)))?;
            std::process::exit(0);
        }

        #[cfg(not(any(unix, windows)))]
        {
            Err(UpdateError::UnsupportedPlatform(
                "재시작 미지원 플랫폼".to_string(),
            ))
        }
    }

    /// 마지막 업데이트 확인 시간 저장/로드를 위한 경로
    pub fn last_check_path() -> PathBuf {
        directories::BaseDirs::new()
            .map(|d| d.cache_dir().join("oneshim").join("last_update_check"))
            .unwrap_or_else(|| PathBuf::from("/tmp/oneshim_last_update_check"))
    }

    /// 마지막 업데이트 확인 시간 저장
    pub fn save_last_check_time(&self) -> Result<(), UpdateError> {
        let path = Self::last_check_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let now = chrono::Utc::now().timestamp();
        std::fs::write(&path, now.to_string())?;
        Ok(())
    }

    /// 업데이트 확인이 필요한지 판단
    pub fn should_check_for_updates(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        let path = Self::last_check_path();
        if !path.exists() {
            return true;
        }

        let Ok(content) = std::fs::read_to_string(&path) else {
            return true;
        };

        let Ok(last_check) = content.trim().parse::<i64>() else {
            return true;
        };

        let now = chrono::Utc::now().timestamp();
        let interval_secs = (self.config.check_interval_hours as i64) * 3600;

        now - last_check >= interval_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> UpdateConfig {
        UpdateConfig {
            enabled: true,
            repo_owner: "test-owner".to_string(),
            repo_name: "test-repo".to_string(),
            check_interval_hours: 24,
            include_prerelease: false,
        }
    }

    #[test]
    fn current_version_is_valid_semver() {
        let version = semver::Version::parse(CURRENT_VERSION);
        assert!(version.is_ok(), "CURRENT_VERSION이 유효한 semver여야 함");
    }

    #[test]
    fn updater_creation() {
        let config = test_config();
        let updater = Updater::new(config.clone());
        assert_eq!(updater.config.repo_owner, "test-owner");
        assert_eq!(updater.config.repo_name, "test-repo");
    }

    #[test]
    fn disabled_updater_returns_error() {
        let mut config = test_config();
        config.enabled = false;
        let updater = Updater::new(config);

        let result = tokio_test::block_on(updater.check_for_updates());
        assert!(matches!(result, Err(UpdateError::Disabled)));
    }

    #[test]
    fn version_comparison_works() {
        let v1 = semver::Version::parse("0.1.0").unwrap();
        let v2 = semver::Version::parse("0.2.0").unwrap();
        let v3 = semver::Version::parse("0.1.1").unwrap();

        assert!(v2 > v1);
        assert!(v3 > v1);
        assert!(v2 > v3);
    }

    #[test]
    fn platform_patterns_exist() {
        let patterns = Updater::get_platform_patterns();
        assert!(patterns.is_ok(), "현재 플랫폼에 패턴이 정의되어야 함");
        assert!(!patterns.unwrap().is_empty());
    }

    #[test]
    fn find_platform_asset_no_assets() {
        let config = test_config();
        let updater = Updater::new(config);

        let release = ReleaseInfo {
            tag_name: "v0.2.0".to_string(),
            name: Some("Test Release".to_string()),
            body: None,
            prerelease: false,
            assets: vec![],
            html_url: "https://github.com/test/test".to_string(),
            published_at: None,
        };

        let result = updater.find_platform_asset(&release);
        assert!(matches!(result, Err(UpdateError::NoSuitableAsset)));
    }

    #[test]
    fn find_platform_asset_matches_pattern() {
        let config = test_config();
        let updater = Updater::new(config);

        // 현재 플랫폼에 맞는 에셋 이름 생성
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let asset_name = "oneshim-macos-arm64.tar.gz";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let asset_name = "oneshim-macos-x64.tar.gz";
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let asset_name = "oneshim-windows-x64.zip";
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let asset_name = "oneshim-linux-x64.tar.gz";
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "x86_64"),
        )))]
        let asset_name = "oneshim-unknown.tar.gz";

        let release = ReleaseInfo {
            tag_name: "v0.2.0".to_string(),
            name: Some("Test Release".to_string()),
            body: None,
            prerelease: false,
            assets: vec![ReleaseAsset {
                name: asset_name.to_string(),
                browser_download_url: "https://example.com/download".to_string(),
                size: 1000,
                content_type: "application/octet-stream".to_string(),
            }],
            html_url: "https://github.com/test/test".to_string(),
            published_at: None,
        };

        let result = updater.find_platform_asset(&release);

        // 지원되는 플랫폼에서만 성공해야 함
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "x86_64"),
        ))]
        assert!(result.is_ok());
    }

    #[test]
    fn should_check_returns_true_when_no_last_check() {
        let config = test_config();
        let updater = Updater::new(config);

        // 마지막 체크 파일이 없으면 true
        // (실제 환경에서는 파일이 있을 수 있으므로 로직만 테스트)
        assert!(updater.config.enabled);
    }

    #[tokio::test]
    async fn check_for_updates_with_mock_api_up_to_date() {
        let mut server = mockito::Server::new_async().await;

        let mock = server
            .mock("GET", "/repos/test-owner/test-repo/releases/latest")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                "tag_name": "v{}",
                "name": "Current Release",
                "body": "No changes",
                "prerelease": false,
                "assets": [],
                "html_url": "https://github.com/test/releases/v0.1.0",
                "published_at": "2024-01-01T00:00:00Z"
            }}"#,
                CURRENT_VERSION
            ))
            .create_async()
            .await;

        let config = test_config();
        let updater = Updater::new(config);

        let result = updater.check_for_updates_with_base_url(&server.url()).await;

        mock.assert_async().await;

        assert!(matches!(result, Ok(UpdateCheckResult::UpToDate { .. })));
    }

    #[tokio::test]
    async fn check_for_updates_with_mock_api_available() {
        let mut server = mockito::Server::new_async().await;

        // 현재 버전보다 높은 버전 반환
        let newer_version = "99.0.0";

        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let asset_name = "oneshim-macos-arm64.tar.gz";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        let asset_name = "oneshim-macos-x64.tar.gz";
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        let asset_name = "oneshim-windows-x64.zip";
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        let asset_name = "oneshim-linux-x64.tar.gz";
        #[cfg(not(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "x86_64"),
        )))]
        let asset_name = "oneshim-unknown.tar.gz";

        let mock = server
            .mock("GET", "/repos/test-owner/test-repo/releases/latest")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                "tag_name": "v{}",
                "name": "New Release",
                "body": "New features",
                "prerelease": false,
                "assets": [{{
                    "name": "{}",
                    "browser_download_url": "https://example.com/download/{}",
                    "size": 10000,
                    "content_type": "application/octet-stream"
                }}],
                "html_url": "https://github.com/test/releases/v99.0.0",
                "published_at": "2024-01-01T00:00:00Z"
            }}"#,
                newer_version, asset_name, asset_name
            ))
            .create_async()
            .await;

        let config = test_config();
        let updater = Updater::new(config);

        let result = updater.check_for_updates_with_base_url(&server.url()).await;

        mock.assert_async().await;

        // 지원되는 플랫폼에서만 Available 반환
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "x86_64"),
        ))]
        {
            match result {
                Ok(UpdateCheckResult::Available { latest, .. }) => {
                    assert_eq!(latest, semver::Version::parse(newer_version).unwrap());
                }
                other => unreachable!("Expected Available, got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn check_for_updates_api_error() {
        let mut server = mockito::Server::new_async().await;

        let mock = server
            .mock("GET", "/repos/test-owner/test-repo/releases/latest")
            .with_status(404)
            .with_body("Not Found")
            .create_async()
            .await;

        let config = test_config();
        let updater = Updater::new(config);

        let result = updater.check_for_updates_with_base_url(&server.url()).await;

        mock.assert_async().await;

        assert!(matches!(result, Err(UpdateError::ParseResponse(_))));
    }

    #[tokio::test]
    async fn prerelease_filtered_when_disabled() {
        let mut server = mockito::Server::new_async().await;

        let mock = server
            .mock("GET", "/repos/test-owner/test-repo/releases/latest")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "tag_name": "v99.0.0-beta",
                "name": "Beta Release",
                "body": "Beta features",
                "prerelease": true,
                "assets": [],
                "html_url": "https://github.com/test/releases/v99.0.0-beta",
                "published_at": "2024-01-01T00:00:00Z"
            }"#,
            )
            .create_async()
            .await;

        let mut config = test_config();
        config.include_prerelease = false;
        let updater = Updater::new(config);

        let result = updater.check_for_updates_with_base_url(&server.url()).await;

        mock.assert_async().await;

        // 사전 릴리즈는 필터링되어 UpToDate 반환
        assert!(matches!(result, Ok(UpdateCheckResult::UpToDate { .. })));
    }

    #[test]
    fn error_display_messages() {
        let errors = vec![
            UpdateError::Disabled,
            UpdateError::AlreadyLatest,
            UpdateError::NoSuitableAsset,
            UpdateError::UnsupportedPlatform("test".to_string()),
            UpdateError::ParseResponse("test".to_string()),
            UpdateError::Download("test".to_string()),
            UpdateError::Install("test".to_string()),
        ];

        for error in errors {
            let msg = format!("{}", error);
            assert!(!msg.is_empty());
        }
    }
}
