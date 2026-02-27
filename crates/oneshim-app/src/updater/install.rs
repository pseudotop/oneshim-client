//! Download, decompress, binary replacement, restart.
#![allow(dead_code)]

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use super::{UpdateError, Updater};

impl Updater {
    pub async fn download_update(&self, download_url: &str) -> Result<PathBuf, UpdateError> {
        let validated_url = self.validate_download_url(download_url)?;
        tracing::info!("Starting update download: {}", validated_url);

        let response = self.http_client.get(validated_url.clone()).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::Download(format!(
                "Download failed: HTTP {}",
                response.status()
            )));
        }

        let bytes = response.bytes().await?;

        let expected_hash = self.fetch_expected_sha256(&validated_url).await?;
        let actual_hash = Self::sha256_hex(&bytes);
        if actual_hash != expected_hash {
            return Err(UpdateError::Integrity(format!(
                "Checksum mismatch: expected={}, actual={}",
                expected_hash, actual_hash
            )));
        }

        if self.config.require_signature_verification {
            let signature = self.fetch_signature(&validated_url).await?;
            self.verify_signature(&bytes, &signature)?;
        }

        let temp_dir = std::env::temp_dir();
        let file_name = validated_url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("oneshim-update")
            .trim();
        let unique = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let temp_path = temp_dir.join(format!("oneshim-{unique}-{file_name}"));

        let mut outfile = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        outfile.write_all(&bytes)?;
        outfile.sync_all()?;

        tracing::info!("Update download completed: {:?}", temp_path);
        Ok(temp_path)
    }

    pub(super) async fn fetch_signature(
        &self,
        download_url: &reqwest::Url,
    ) -> Result<Vec<u8>, UpdateError> {
        let sig_url = reqwest::Url::parse(&format!("{}.sig", download_url))
            .map_err(|e| UpdateError::Integrity(format!("Failed to parse signature URL: {}", e)))?;

        self.validate_download_url(sig_url.as_str())?;

        let response = self.http_client.get(sig_url.clone()).send().await?;
        if !response.status().is_success() {
            return Err(UpdateError::Integrity(format!(
                "Failed to download signature file: HTTP {} ({})",
                response.status(),
                sig_url
            )));
        }

        let body = response.bytes().await?;
        let body = String::from_utf8(body.to_vec()).map_err(|e| {
            UpdateError::Integrity(format!("Invalid signature file encoding: {}", e))
        })?;

        let sig_b64 = body
            .split_whitespace()
            .next()
            .ok_or_else(|| UpdateError::Integrity("Signature file is empty".to_string()))?;

        BASE64.decode(sig_b64).map_err(|e| {
            UpdateError::Integrity(format!("Failed to decode signature base64: {}", e))
        })
    }

    pub(super) fn verify_signature(
        &self,
        payload: &[u8],
        signature_bytes: &[u8],
    ) -> Result<(), UpdateError> {
        let key_b64 = self
            .config
            .signature_public_key
            .split_whitespace()
            .next()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                UpdateError::Integrity(
                    "Public key for signature verification is not configured (update.signature_public_key)"
                        .to_string(),
                )
            })?;

        let key_bytes = BASE64.decode(key_b64).map_err(|e| {
            UpdateError::Integrity(format!("Failed to decode public key base64: {}", e))
        })?;
        let key_len = key_bytes.len();
        let key_array: [u8; 32] = key_bytes.try_into().map_err(|_| {
            UpdateError::Integrity(format!(
                "Invalid public key length: {} bytes (expected 32)",
                key_len
            ))
        })?;

        let signature_array: [u8; 64] = signature_bytes.try_into().map_err(|_| {
            UpdateError::Integrity(format!(
                "Invalid signature length: {} bytes (expected 64)",
                signature_bytes.len()
            ))
        })?;

        let public_key = VerifyingKey::from_bytes(&key_array)
            .map_err(|e| UpdateError::Integrity(format!("Failed to parse public key: {}", e)))?;
        let signature = Signature::from_bytes(&signature_array);

        public_key
            .verify(payload, &signature)
            .map_err(|e| UpdateError::Integrity(format!("Signature verification failed: {}", e)))
    }

    pub(super) async fn fetch_expected_sha256(
        &self,
        download_url: &reqwest::Url,
    ) -> Result<String, UpdateError> {
        let checksum_url = reqwest::Url::parse(&format!("{}.sha256", download_url))
            .map_err(|e| UpdateError::Integrity(format!("Failed to parse checksum URL: {}", e)))?;

        self.validate_download_url(checksum_url.as_str())?;

        let response = self.http_client.get(checksum_url.clone()).send().await?;
        if !response.status().is_success() {
            return Err(UpdateError::Integrity(format!(
                "Failed to download checksum file: HTTP {} ({})",
                response.status(),
                checksum_url
            )));
        }

        let body = response.bytes().await?;
        let body = String::from_utf8(body.to_vec()).map_err(|e| {
            UpdateError::Integrity(format!("Invalid checksum file encoding: {}", e))
        })?;

        Self::parse_sha256_manifest(&body)
    }

    pub(super) fn parse_sha256_manifest(content: &str) -> Result<String, UpdateError> {
        let hash = content
            .split_whitespace()
            .next()
            .ok_or_else(|| UpdateError::Integrity("Checksum file is empty".to_string()))?
            .to_ascii_lowercase();

        let is_hex = hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit());
        if !is_hex {
            return Err(UpdateError::Integrity(format!(
                "Invalid SHA-256 format: {}",
                hash
            )));
        }

        Ok(hash)
    }

    pub(super) fn sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    pub(super) fn validate_download_url(&self, url: &str) -> Result<reqwest::Url, UpdateError> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|e| UpdateError::Download(format!("Failed to parse download URL: {}", e)))?;

        let Some(host) = parsed.host_str() else {
            return Err(UpdateError::Download(
                "Download URL host is missing".to_string(),
            ));
        };

        if parsed.scheme() != "https" {
            #[cfg(test)]
            if parsed.scheme() == "http" && matches!(host, "localhost" | "127.0.0.1") {
                // Local test server is allowed for deterministic updater tests.
            } else {
                return Err(UpdateError::Download(format!(
                    "Only HTTPS download URLs are allowed: {}",
                    parsed
                )));
            }

            #[cfg(not(test))]
            return Err(UpdateError::Download(format!(
                "Only HTTPS download URLs are allowed: {}",
                parsed
            )));
        }

        if !Self::is_allowed_download_host(host) {
            return Err(UpdateError::Download(format!(
                "Disallowed download host: {}",
                host
            )));
        }

        Ok(parsed)
    }

    pub(super) fn is_allowed_download_host(host: &str) -> bool {
        let allowlisted = Self::ALLOWED_DOWNLOAD_HOSTS.iter().any(|allowed_host| {
            host == *allowed_host || host.ends_with(&format!(".{}", allowed_host))
        });
        if allowlisted {
            return true;
        }

        #[cfg(test)]
        {
            matches!(host, "localhost" | "127.0.0.1")
        }

        #[cfg(not(test))]
        {
            false
        }
    }

    pub(super) fn is_safe_archive_path(path: &Path) -> bool {
        path.components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
    }

    pub(super) fn backup_path_for(current_exe: &Path) -> Result<PathBuf, UpdateError> {
        let parent = current_exe.parent().ok_or_else(|| {
            UpdateError::Install(
                "Failed to locate parent directory of current executable".to_string(),
            )
        })?;

        let file_name = current_exe
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("oneshim")
            .to_string();
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
        Ok(parent.join(format!("{}.rollback.{}", file_name, ts)))
    }

    pub(super) fn install_and_restart_with_ops<FReplace, FRestart>(
        &self,
        downloaded_path: &Path,
        current_exe: &Path,
        mut replace_binary: FReplace,
        mut restart_app: FRestart,
    ) -> Result<(), UpdateError>
    where
        FReplace: FnMut(&Path) -> Result<(), UpdateError>,
        FRestart: FnMut() -> Result<(), UpdateError>,
    {
        tracing::info!("Starting update installation: {:?}", downloaded_path);

        let backup_path = Self::backup_path_for(current_exe)?;
        std::fs::copy(current_exe, &backup_path)?;

        let file_name = downloaded_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let binary_path = if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
            self.extract_tar_gz(downloaded_path)?
        } else if file_name.ends_with(".zip") {
            self.extract_zip(downloaded_path)?
        } else {
            downloaded_path.to_path_buf()
        };

        replace_binary(&binary_path)?;

        tracing::info!("Update installation completed, restarting application...");

        match restart_app() {
            Ok(()) => Ok(()),
            Err(restart_err) => {
                tracing::error!(
                    "Restart failed, attempting rollback: backup={:?}, error={}",
                    backup_path,
                    restart_err
                );

                match replace_binary(&backup_path) {
                    Ok(()) => Err(UpdateError::Install(format!(
                        "Rollback completed after restart failure: {}",
                        restart_err
                    ))),
                    Err(rollback_err) => Err(UpdateError::Install(format!(
                        "Restart failed and rollback failed: restart={}, rollback={}",
                        restart_err, rollback_err
                    ))),
                }
            }
        }
    }

    /// # Safety
    pub fn install_and_restart(&self, downloaded_path: &Path) -> Result<(), UpdateError> {
        use self_update::self_replace;

        let current_exe = std::env::current_exe()?;
        self.install_and_restart_with_ops(
            downloaded_path,
            &current_exe,
            |candidate| {
                self_replace::self_replace(candidate)
                    .map_err(|e| UpdateError::Install(format!("Failed to replace binary: {}", e)))
            },
            || self.restart_app(),
        )
    }

    pub(super) fn extract_tar_gz(&self, archive_path: &Path) -> Result<PathBuf, UpdateError> {
        use flate2::read::GzDecoder;
        use std::fs::File;

        let file = File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);

        let extract_dir = archive_path
            .parent()
            .unwrap_or(std::path::Path::new("/tmp"));
        for entry in archive.entries()? {
            let mut entry = entry?;
            let entry_path = entry.path()?;

            if !Self::is_safe_archive_path(&entry_path) {
                return Err(UpdateError::Install(format!(
                    "Unsafe tar entry path: {}",
                    entry_path.display()
                )));
            }

            let outpath = extract_dir.join(entry_path.as_ref());

            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let entry_type = entry.header().entry_type();
            if entry_type.is_dir() {
                std::fs::create_dir_all(&outpath)?;
                continue;
            }

            if !entry_type.is_file() {
                return Err(UpdateError::Install(format!(
                    "Unsupported tar entry type: {}",
                    entry_path.display()
                )));
            }

            entry.unpack(&outpath)?;
        }

        self.find_binary_in_dir(extract_dir)
    }

    pub(super) fn extract_zip(&self, archive_path: &Path) -> Result<PathBuf, UpdateError> {
        use std::fs::File;

        let file = File::open(archive_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| UpdateError::Install(format!("Failed to open ZIP archive: {}", e)))?;

        let extract_dir = archive_path
            .parent()
            .unwrap_or(std::path::Path::new("/tmp"));

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| UpdateError::Install(format!("Failed to read ZIP entry: {}", e)))?;

            let relative_path = file.enclosed_name().ok_or_else(|| {
                UpdateError::Install(format!("Unsafe ZIP entry path: {}", file.name()))
            })?;

            if !Self::is_safe_archive_path(&relative_path) {
                return Err(UpdateError::Install(format!(
                    "Unsafe ZIP entry path: {}",
                    file.name()
                )));
            }

            let outpath = extract_dir.join(relative_path);

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

    pub(super) fn find_binary_in_dir(&self, dir: &std::path::Path) -> Result<PathBuf, UpdateError> {
        let binary_name = if cfg!(windows) {
            "oneshim.exe"
        } else {
            "oneshim"
        };

        let direct_path = dir.join(binary_name);
        if direct_path.exists() {
            return Ok(direct_path);
        }

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
            "Binary '{}' not found",
            binary_name
        )))
    }

    pub(super) fn restart_app(&self) -> Result<(), UpdateError> {
        let current_exe = std::env::current_exe()?;
        let args: Vec<String> = std::env::args().skip(1).collect();

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new(&current_exe).args(&args).exec();
            Err(UpdateError::Install(format!("Restart failed: {}", err)))
        }

        #[cfg(windows)]
        {
            std::process::Command::new(&current_exe)
                .args(&args)
                .spawn()
                .map_err(|e| UpdateError::Install(format!("Restart failed: {}", e)))?;
            std::process::exit(0);
        }

        #[cfg(not(any(unix, windows)))]
        {
            Err(UpdateError::UnsupportedPlatform(
                "Restart is not supported on this platform".to_string(),
            ))
        }
    }
}
