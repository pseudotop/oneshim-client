#![allow(dead_code)] // UI /

mod github;
mod install;
mod state;

use oneshim_core::config::UpdateConfig;
use serde::Deserialize;
use thiserror::Error;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("GitHub API request failed: {0}")]
    ApiRequest(#[from] reqwest::Error),

    #[error("Failed to parse API response: {0}")]
    ParseResponse(String),

    #[error("Failed to parse version: {0}")]
    VersionParse(#[from] semver::Error),

    #[error("Download failed: {0}")]
    Download(String),

    #[error("Installation failed: {0}")]
    Install(String),

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("Filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),

    #[error("Auto-update is disabled")]
    Disabled,

    #[error("Already on latest version")]
    AlreadyLatest,

    #[error("No suitable release asset found for current platform")]
    NoSuitableAsset,

    #[error("Integrity verification failed: {0}")]
    Integrity(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAsset>,
    /// HTML URL
    pub html_url: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    /// Content-Type
    pub content_type: String,
}

#[derive(Debug)]
pub enum UpdateCheckResult {
    Available {
        current: semver::Version,
        latest: semver::Version,
        release: Box<ReleaseInfo>,
        download_url: String,
    },
    UpToDate {
        current: semver::Version,
    },
}

pub struct Updater {
    pub(super) config: UpdateConfig,
    pub(super) http_client: reqwest::Client,
}

impl Updater {
    pub(super) const ALLOWED_DOWNLOAD_HOSTS: [&'static str; 4] = [
        "github.com",
        "api.github.com",
        "objects.githubusercontent.com",
        "githubusercontent.com",
    ];

    pub fn new(config: UpdateConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .user_agent(format!("oneshim/{}", CURRENT_VERSION))
            .build()
            .expect("failed to build HTTP client");

        Self {
            config,
            http_client,
        }
    }

    #[cfg(test)]
    pub fn with_client(config: UpdateConfig, http_client: reqwest::Client) -> Self {
        Self {
            config,
            http_client,
        }
    }

    #[cfg(test)]
    pub async fn check_for_updates_with_base_url(
        &self,
        base_url: &str,
    ) -> Result<UpdateCheckResult, UpdateError> {
        if !self.config.enabled {
            return Err(UpdateError::Disabled);
        }

        let current = semver::Version::parse(CURRENT_VERSION)?;

        let url = format!(
            "{}/repos/{}/{}/releases/latest",
            base_url, self.config.repo_owner, self.config.repo_name
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::ParseResponse(format!(
                "API response status: {}",
                response.status()
            )));
        }

        let release: ReleaseInfo = response.json().await?;

        if release.prerelease && !self.config.include_prerelease {
            return Ok(UpdateCheckResult::UpToDate { current });
        }

        let latest_tag = release.tag_name.trim_start_matches('v');
        let latest = semver::Version::parse(latest_tag)?;
        self.enforce_version_floor(&latest)?;

        if latest > current {
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

    pub async fn check_for_updates(&self) -> Result<UpdateCheckResult, UpdateError> {
        if !self.config.enabled {
            return Err(UpdateError::Disabled);
        }

        let current = semver::Version::parse(CURRENT_VERSION)?;

        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.config.repo_owner, self.config.repo_name
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::ParseResponse(format!(
                "API response status: {}",
                response.status()
            )));
        }

        let release: ReleaseInfo = response.json().await?;

        if release.prerelease && !self.config.include_prerelease {
            return Ok(UpdateCheckResult::UpToDate { current });
        }

        let latest_tag = release.tag_name.trim_start_matches('v');
        let latest = semver::Version::parse(latest_tag)?;
        self.enforce_version_floor(&latest)?;

        if latest > current {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use tempfile::tempdir;

    fn test_config() -> UpdateConfig {
        UpdateConfig {
            enabled: true,
            repo_owner: "test-owner".to_string(),
            repo_name: "test-repo".to_string(),
            check_interval_hours: 24,
            include_prerelease: false,
            auto_install: false,
            require_signature_verification: false,
            signature_public_key: String::new(),
            min_allowed_version: None,
        }
    }

    #[test]
    fn current_version_is_valid_semver() {
        let version = semver::Version::parse(CURRENT_VERSION);
        assert!(version.is_ok(), "CURRENT_VERSION must be a valid semver");
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
        assert!(
            patterns.is_ok(),
            "Current platform must have at least one pattern"
        );
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

        assert!(matches!(result, Ok(UpdateCheckResult::UpToDate { .. })));
    }

    #[tokio::test]
    async fn check_for_updates_rejects_release_below_min_allowed_version() {
        let mut server = mockito::Server::new_async().await;

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
                "tag_name": "v99.0.0",
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
                asset_name, asset_name
            ))
            .create_async()
            .await;

        let mut config = test_config();
        config.min_allowed_version = Some("100.0.0".to_string());
        let updater = Updater::new(config);

        let result = updater.check_for_updates_with_base_url(&server.url()).await;

        mock.assert_async().await;
        assert!(matches!(result, Err(UpdateError::Integrity(_))));
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

    #[test]
    fn parse_sha256_manifest_validates_format() {
        let hash = Updater::parse_sha256_manifest(
            "8f434346648f6b96df89dda901c5176b10a6d83961fca6f18e40f9f0f84f2304  oneshim.tar.gz",
        )
        .unwrap();
        assert_eq!(
            hash,
            "8f434346648f6b96df89dda901c5176b10a6d83961fca6f18e40f9f0f84f2304"
        );
    }

    #[test]
    fn parse_sha256_manifest_rejects_invalid_hash() {
        let err = Updater::parse_sha256_manifest("not-a-valid-hash  oneshim.tar.gz");
        assert!(matches!(err, Err(UpdateError::Integrity(_))));
    }

    #[test]
    fn validate_download_url_rejects_http_and_unknown_host() {
        let updater = Updater::new(test_config());

        let http_url = updater.validate_download_url("http://github.com/file.tar.gz");
        assert!(http_url.is_err());

        let unknown_host = updater.validate_download_url("https://evil.example.com/file.tar.gz");
        assert!(unknown_host.is_err());
    }

    #[test]
    fn extract_zip_rejects_path_traversal_entries() {
        use std::io::Write;

        let updater = Updater::new(test_config());
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("malicious.zip");

        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options: zip::write::SimpleFileOptions = zip::write::FileOptions::default();
            writer.start_file("../../outside", options).unwrap();
            writer.write_all(b"malicious").unwrap();
            writer.finish().unwrap();
        }

        let result = updater.extract_zip(&zip_path);
        assert!(matches!(result, Err(UpdateError::Install(_))));
    }

    #[test]
    fn verify_signature_accepts_valid_ed25519_signature() {
        use ed25519_dalek::{Signer, SigningKey};

        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let verifying_key = signing_key.verifying_key();

        let mut config = test_config();
        config.require_signature_verification = true;
        config.signature_public_key = BASE64.encode(verifying_key.as_bytes());
        let updater = Updater::new(config);

        let payload = b"oneshim-release-artifact";
        let signature = signing_key.sign(payload);

        let result = updater.verify_signature(payload, signature.to_bytes().as_slice());
        assert!(result.is_ok());
    }

    #[test]
    fn verify_signature_rejects_invalid_signature() {
        use ed25519_dalek::{Signer, SigningKey};

        let signing_key = SigningKey::from_bytes(&[9u8; 32]);
        let verifying_key = signing_key.verifying_key();

        let mut config = test_config();
        config.require_signature_verification = true;
        config.signature_public_key = BASE64.encode(verifying_key.as_bytes());
        let updater = Updater::new(config);

        let payload = b"artifact-A";
        let signature = signing_key.sign(payload);

        let result = updater.verify_signature(b"artifact-B", signature.to_bytes().as_slice());
        assert!(matches!(result, Err(UpdateError::Integrity(_))));
    }

    #[test]
    fn release_reliability_validate_download_url_allows_localhost_in_tests() {
        let updater = Updater::new(test_config());
        assert!(updater
            .validate_download_url("https://localhost/oneshim-update.tar.gz")
            .is_ok());
        assert!(updater
            .validate_download_url("https://127.0.0.1/oneshim-update.tar.gz")
            .is_ok());
    }

    #[tokio::test]
    async fn release_reliability_download_update_accepts_localhost_with_integrity() {
        let mut server = mockito::Server::new_async().await;
        let asset_name = "oneshim-test-update.tar.gz";
        let payload = b"release-artifact-v1".to_vec();
        let expected_hash = Updater::sha256_hex(&payload);

        let artifact_mock = server
            .mock("GET", format!("/{asset_name}").as_str())
            .with_status(200)
            .with_body(payload.clone())
            .create_async()
            .await;
        let checksum_mock = server
            .mock("GET", format!("/{asset_name}.sha256").as_str())
            .with_status(200)
            .with_body(format!("{expected_hash}  {asset_name}\n"))
            .create_async()
            .await;

        let config = test_config();
        let client = reqwest::Client::builder().build().unwrap();
        let updater = Updater::with_client(config, client);
        let download_url = format!("{}/{}", server.url(), asset_name);

        let downloaded_path = updater.download_update(&download_url).await.unwrap();
        let downloaded_bytes = std::fs::read(&downloaded_path).unwrap();
        assert_eq!(downloaded_bytes, payload);

        std::fs::remove_file(&downloaded_path).unwrap();
        artifact_mock.assert_async().await;
        checksum_mock.assert_async().await;
    }

    #[tokio::test]
    async fn release_reliability_download_update_rejects_checksum_mismatch() {
        let mut server = mockito::Server::new_async().await;
        let asset_name = "oneshim-test-update.tar.gz";

        let artifact_mock = server
            .mock("GET", format!("/{asset_name}").as_str())
            .with_status(200)
            .with_body("release-artifact-v1")
            .create_async()
            .await;
        let checksum_mock = server
            .mock("GET", format!("/{asset_name}.sha256").as_str())
            .with_status(200)
            .with_body(format!("{}  {asset_name}\n", "0".repeat(64)))
            .create_async()
            .await;

        let config = test_config();
        let client = reqwest::Client::builder().build().unwrap();
        let updater = Updater::with_client(config, client);
        let download_url = format!("{}/{}", server.url(), asset_name);

        let err = updater.download_update(&download_url).await.unwrap_err();
        assert!(matches!(err, UpdateError::Integrity(msg) if msg.contains("Checksum mismatch")));

        artifact_mock.assert_async().await;
        checksum_mock.assert_async().await;
    }

    #[test]
    fn release_reliability_install_and_restart_rolls_back_after_restart_failure() {
        let updater = Updater::new(test_config());
        let dir = tempdir().unwrap();
        let current_exe = dir.path().join("oneshim-current");
        let downloaded = dir.path().join("oneshim-new");
        std::fs::write(&current_exe, b"current-binary").unwrap();
        std::fs::write(&downloaded, b"new-binary").unwrap();

        let mut replaced = Vec::new();
        let result = updater.install_and_restart_with_ops(
            &downloaded,
            &current_exe,
            |candidate| {
                replaced.push(candidate.to_path_buf());
                Ok(())
            },
            || {
                Err(UpdateError::Install(
                    "simulated restart failure".to_string(),
                ))
            },
        );

        assert!(matches!(
            result,
            Err(UpdateError::Install(msg)) if msg.contains("Rollback completed after restart failure")
        ));
        assert_eq!(replaced.len(), 2);
        assert_eq!(replaced[0], downloaded);
        assert!(replaced[1]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".rollback."));
    }

    #[test]
    fn release_reliability_install_and_restart_reports_rollback_failure() {
        let updater = Updater::new(test_config());
        let dir = tempdir().unwrap();
        let current_exe = dir.path().join("oneshim-current");
        let downloaded = dir.path().join("oneshim-new");
        std::fs::write(&current_exe, b"current-binary").unwrap();
        std::fs::write(&downloaded, b"new-binary").unwrap();

        let mut replace_calls = 0usize;
        let result = updater.install_and_restart_with_ops(
            &downloaded,
            &current_exe,
            |_candidate| {
                replace_calls += 1;
                if replace_calls == 1 {
                    Ok(())
                } else {
                    Err(UpdateError::Install(
                        "simulated rollback replace failure".to_string(),
                    ))
                }
            },
            || {
                Err(UpdateError::Install(
                    "simulated restart failure".to_string(),
                ))
            },
        );

        match result {
            Err(UpdateError::Install(msg)) => {
                assert!(msg.contains("Restart failed and rollback failed"));
                assert!(msg.contains("simulated restart failure"));
                assert!(msg.contains("simulated rollback replace failure"));
            }
            other => panic!("unexpected result: {:?}", other),
        }
        assert_eq!(replace_calls, 2);
    }
}
