//! 로컬 SQLite 데이터베이스 암호화 키 관리
//!
//! # 키 저장 전략
//! 1. 키 파일 (`app_data_dir/.db_key`): 32바이트 원시 키
//! 2. 파일 권한: Unix에서 0o600 (소유자만 읽기/쓰기)
//!
//! # 향후 작업
//! SQLCipher 또는 at-rest 암호화 레이어와 연동 예정.
//! 현재는 키 생성/저장/로드 인프라만 제공.

use oneshim_core::error::CoreError;
use std::path::{Path, PathBuf};

/// 32바이트 AES-256 데이터베이스 암호화 키
#[derive(Clone)]
pub struct EncryptionKey([u8; 32]);

impl EncryptionKey {
    /// 키 파일에서 로드하거나 신규 생성
    ///
    /// - 파일 존재 시: 로드 (32바이트 검증)
    /// - 파일 없을 시: 생성 후 파일에 저장 (Unix: 0o600 권한)
    pub fn load_or_create(app_data_dir: &Path) -> Result<Self, CoreError> {
        let key_path = app_data_dir.join(".db_key");

        if key_path.exists() {
            return Self::load_from_file(&key_path);
        }

        let key = Self::generate()?;
        key.save_to_file(&key_path)?;
        tracing::info!("New DB encryption key generated: {:?}", key_path);
        Ok(key)
    }

    /// 원시 바이트에서 키 생성 (테스트용)
    #[cfg(test)]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// SQLite pragma key 형식 (hex 문자열)
    pub fn as_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// 원시 바이트 참조
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn generate() -> Result<Self, CoreError> {
        let mut key = [0u8; 32];
        getrandom::getrandom(&mut key)
            .map_err(|e| CoreError::Internal(format!("OS random number generation failed: {e}")))?;
        Ok(Self(key))
    }

    fn load_from_file(path: &PathBuf) -> Result<Self, CoreError> {
        let bytes = std::fs::read(path)
            .map_err(|e| CoreError::Internal(format!("Key file read failed ({path:?}): {e}")))?;

        if bytes.len() != 32 {
            return Err(CoreError::Internal(format!(
                "Key file size error: expected 32 bytes, got {} bytes",
                bytes.len()
            )));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(Self(key))
    }

    fn save_to_file(&self, path: &PathBuf) -> Result<(), CoreError> {
        std::fs::write(path, &self.0)
            .map_err(|e| CoreError::Internal(format!("Key file write failed ({path:?}): {e}")))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| CoreError::Internal(format!("Key file permission set failed: {e}")))?;
        }

        Ok(())
    }
}

// 키가 로그에 출력되지 않도록 Debug 구현 안전하게
impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EncryptionKey([redacted])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn generates_32_byte_key() {
        let dir = TempDir::new().unwrap();
        let key = EncryptionKey::load_or_create(dir.path()).unwrap();
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn hex_is_64_chars() {
        let dir = TempDir::new().unwrap();
        let key = EncryptionKey::load_or_create(dir.path()).unwrap();
        assert_eq!(key.as_hex().len(), 64);
        assert!(key.as_hex().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn load_returns_same_key_as_generated() {
        let dir = TempDir::new().unwrap();
        let key1 = EncryptionKey::load_or_create(dir.path()).unwrap();
        let key2 = EncryptionKey::load_or_create(dir.path()).unwrap();
        assert_eq!(key1.as_hex(), key2.as_hex());
    }

    #[test]
    fn key_file_created_with_correct_size() {
        let dir = TempDir::new().unwrap();
        EncryptionKey::load_or_create(dir.path()).unwrap();
        let content = fs::read(dir.path().join(".db_key")).unwrap();
        assert_eq!(content.len(), 32);
    }

    #[test]
    fn debug_does_not_leak_key_bytes() {
        let key = EncryptionKey::from_bytes([0xAB; 32]);
        let debug_str = format!("{key:?}");
        assert!(!debug_str.contains("AB"));
        assert!(debug_str.contains("redacted"));
    }
}
