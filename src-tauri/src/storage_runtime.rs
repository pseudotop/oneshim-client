use anyhow::Result;
use oneshim_storage::sqlite::SqliteStorage;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

pub(crate) struct StorageRuntimeBundle {
    pub(crate) sqlite_storage: Arc<SqliteStorage>,
}

pub(crate) struct StorageRuntimeBuilder<'a> {
    db_path: &'a Path,
    data_dir: &'a Path,
    retention_days: u32,
}

impl<'a> StorageRuntimeBuilder<'a> {
    pub(crate) fn new(db_path: &'a Path, data_dir: &'a Path, retention_days: u32) -> Self {
        Self {
            db_path,
            data_dir,
            retention_days,
        }
    }

    pub(crate) fn build(&self) -> Result<StorageRuntimeBundle> {
        let encryption_key =
            match oneshim_storage::encryption::EncryptionKey::load_or_create(self.data_dir) {
                Ok(key) => {
                    info!(
                        "DB encryption key ready ({})",
                        self.data_dir.join(".db_key").display()
                    );
                    Some(key)
                }
                Err(error) => {
                    warn!("DB encryption key provisioning failed (non-fatal): {error}");
                    None
                }
            };

        let sqlite_storage = Arc::new(SqliteStorage::open(self.db_path, self.retention_days)?);
        if encryption_key.is_some() {
            info!(
                "SQLite initialized: {} (encryption key provisioned, SQLCipher pending)",
                self.db_path.display()
            );
        } else {
            info!("SQLite initialized: {} (plaintext)", self.db_path.display());
        }

        Ok(StorageRuntimeBundle { sqlite_storage })
    }
}
