//! OS keychain secret store adapter.
//!
//! Hybrid approach: `keyring` crate for OS keychain storage,
//! JSON file as enumeration cache for `delete_namespace`/list.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::ports::secret_store::SecretStore;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Keys that OAuth flows are known to store.
/// `delete_namespace` always tries these in addition to registry contents.
const KNOWN_OAUTH_KEYS: &[&str] = &[
    "access_token",
    "refresh_token",
    "scopes",
    "expires_at",
    "id_token",
];

/// Known OAuth provider IDs (matches OAuthProviderConfig presets).
pub const KNOWN_PROVIDERS: &[&str] = &["openai"];

/// JSON enumeration cache — tracks which namespace/key combinations exist
/// in the OS keychain. NOT source of truth; keychain is authoritative.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct KeychainRegistry {
    pub version: u32,
    pub namespaces: HashMap<String, BTreeSet<String>>,
}

impl KeychainRegistry {
    fn new() -> Self {
        Self {
            version: 1,
            namespaces: HashMap::new(),
        }
    }

    /// Load from disk. Returns empty registry on missing/corrupt file.
    pub fn load_or_default(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match serde_json::from_str::<Self>(&contents) {
                Ok(reg) => reg,
                Err(e) => {
                    warn!(
                        "Corrupt keychain registry at {}: {e}. Starting empty.",
                        path.display()
                    );
                    Self::new()
                }
            },
            Err(_) => Self::new(),
        }
    }

    /// Atomic write: temp file + rename.
    pub fn save(&self, path: &std::path::Path) -> Result<(), CoreError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| CoreError::SecretStoreError(format!("registry serialization: {e}")))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn add_key(&mut self, namespace: &str, key: &str) {
        self.namespaces
            .entry(namespace.to_owned())
            .or_default()
            .insert(key.to_owned());
    }

    pub fn remove_key(&mut self, namespace: &str, key: &str) {
        if let Some(keys) = self.namespaces.get_mut(namespace) {
            keys.remove(key);
            if keys.is_empty() {
                self.namespaces.remove(namespace);
            }
        }
    }

    /// Remove namespace, returning its key set for keychain deletion.
    pub fn remove_namespace(&mut self, namespace: &str) -> BTreeSet<String> {
        self.namespaces.remove(namespace).unwrap_or_default()
    }

    pub fn keys_for(&self, namespace: &str) -> BTreeSet<String> {
        self.namespaces.get(namespace).cloned().unwrap_or_default()
    }

    pub fn all_namespaces(&self) -> Vec<String> {
        self.namespaces.keys().cloned().collect()
    }
}

/// Sync core — all keyring and registry operations.
/// Used directly by CLI (no tokio needed), wrapped by KeychainSecretStore for async port.
pub struct KeychainOps {
    service_name: String,
    registry: parking_lot::Mutex<KeychainRegistry>,
    registry_path: PathBuf,
}

/// Status of a namespace in the keychain.
#[derive(Debug)]
pub struct NamespaceStatus {
    pub connected: bool,
    pub keys_found: Vec<String>,
    pub expires_at: Option<String>,
}

impl KeychainOps {
    /// Create ops with the given registry file path.
    /// The caller (setup.rs) resolves the config directory.
    pub fn new(registry_path: PathBuf) -> Result<Self, CoreError> {
        let registry = KeychainRegistry::load_or_default(&registry_path);
        Ok(Self {
            service_name: "oneshim".into(),
            registry: parking_lot::Mutex::new(registry),
            registry_path,
        })
    }

    fn entry(&self, namespace: &str, key: &str) -> Result<keyring::Entry, CoreError> {
        let user = format!("{namespace}.{key}");
        keyring::Entry::new(&self.service_name, &user)
            .map_err(|e| CoreError::SecretStoreError(format!("keyring entry creation: {e}")))
    }

    fn map_keyring_err(e: keyring::Error) -> CoreError {
        CoreError::SecretStoreError(format!("keychain: {e}"))
    }

    pub fn store_sync(&self, namespace: &str, key: &str, value: &str) -> Result<(), CoreError> {
        // 1. Write to keychain
        self.entry(namespace, key)?
            .set_password(value)
            .map_err(Self::map_keyring_err)?;
        // 2. Update registry cache
        let mut reg = self.registry.lock();
        reg.add_key(namespace, key);
        // 3. Persist (best-effort — keychain already has the value)
        if let Err(e) = reg.save(&self.registry_path) {
            warn!("Failed to persist keychain registry: {e}");
        }
        Ok(())
    }

    pub fn retrieve_sync(&self, namespace: &str, key: &str) -> Result<Option<String>, CoreError> {
        match self.entry(namespace, key)?.get_password() {
            Ok(val) => Ok(Some(val)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Self::map_keyring_err(e)),
        }
    }

    pub fn delete_sync(&self, namespace: &str, key: &str) -> Result<(), CoreError> {
        match self.entry(namespace, key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(Self::map_keyring_err(e)),
        }
        let mut reg = self.registry.lock();
        reg.remove_key(namespace, key);
        if let Err(e) = reg.save(&self.registry_path) {
            warn!("Failed to persist keychain registry: {e}");
        }
        Ok(())
    }

    pub fn delete_namespace_sync(&self, namespace: &str) -> Result<(), CoreError> {
        // Build key set = registry keys ∪ KNOWN_OAUTH_KEYS
        let registry_keys = {
            let reg = self.registry.lock();
            reg.keys_for(namespace)
        };
        let mut all_keys: BTreeSet<String> = registry_keys;
        for k in KNOWN_OAUTH_KEYS {
            all_keys.insert((*k).to_owned());
        }

        let mut errors = Vec::new();
        for key in &all_keys {
            match self.entry(namespace, key) {
                Ok(entry) => match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => errors.push(format!("{key}: {e}")),
                },
                Err(e) => errors.push(format!("{key}: {e}")),
            }
        }

        // Update registry regardless of individual errors
        let mut reg = self.registry.lock();
        reg.remove_namespace(namespace);
        if let Err(e) = reg.save(&self.registry_path) {
            warn!("Failed to persist keychain registry: {e}");
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(CoreError::SecretStoreError(format!(
                "partial delete_namespace failure: {}",
                errors.join("; ")
            )))
        }
    }

    /// Probe keychain availability by writing and deleting a test entry.
    pub fn probe_availability(&self) -> Result<(), CoreError> {
        let entry = keyring::Entry::new(&self.service_name, "__probe__")
            .map_err(|e| CoreError::SecretStoreError(format!("keyring probe entry: {e}")))?;
        entry.set_password("probe").map_err(Self::map_keyring_err)?;
        let _ = entry.delete_credential();
        Ok(())
    }

    pub fn all_namespaces(&self) -> Vec<String> {
        self.registry.lock().all_namespaces()
    }

    /// Probe a namespace: check which known keys exist in the keychain.
    pub fn probe_namespace(&self, namespace: &str) -> NamespaceStatus {
        let mut keys_found = Vec::new();
        let mut expires_at = None;

        for key in KNOWN_OAUTH_KEYS {
            if let Ok(Some(val)) = self.retrieve_sync(namespace, key) {
                keys_found.push((*key).to_owned());
                if *key == "expires_at" {
                    expires_at = Some(val);
                }
            }
        }

        NamespaceStatus {
            connected: keys_found.contains(&"access_token".to_owned()),
            keys_found,
            expires_at,
        }
    }
}

/// Async adapter — wraps KeychainOps via spawn_blocking to implement SecretStore.
pub struct KeychainSecretStore {
    ops: Arc<KeychainOps>,
}

impl KeychainSecretStore {
    pub fn new(ops: Arc<KeychainOps>) -> Self {
        Self { ops }
    }
}

#[async_trait]
impl SecretStore for KeychainSecretStore {
    async fn store(&self, namespace: &str, key: &str, value: &str) -> Result<(), CoreError> {
        let ops = self.ops.clone();
        let ns = namespace.to_owned();
        let k = key.to_owned();
        let v = value.to_owned();
        tokio::task::spawn_blocking(move || ops.store_sync(&ns, &k, &v))
            .await
            .map_err(|e| CoreError::SecretStoreError(format!("spawn_blocking: {e}")))?
    }

    async fn retrieve(&self, namespace: &str, key: &str) -> Result<Option<String>, CoreError> {
        let ops = self.ops.clone();
        let ns = namespace.to_owned();
        let k = key.to_owned();
        tokio::task::spawn_blocking(move || ops.retrieve_sync(&ns, &k))
            .await
            .map_err(|e| CoreError::SecretStoreError(format!("spawn_blocking: {e}")))?
    }

    async fn delete(&self, namespace: &str, key: &str) -> Result<(), CoreError> {
        let ops = self.ops.clone();
        let ns = namespace.to_owned();
        let k = key.to_owned();
        tokio::task::spawn_blocking(move || ops.delete_sync(&ns, &k))
            .await
            .map_err(|e| CoreError::SecretStoreError(format!("spawn_blocking: {e}")))?
    }

    async fn delete_namespace(&self, namespace: &str) -> Result<(), CoreError> {
        let ops = self.ops.clone();
        let ns = namespace.to_owned();
        tokio::task::spawn_blocking(move || ops.delete_namespace_sync(&ns))
            .await
            .map_err(|e| CoreError::SecretStoreError(format!("spawn_blocking: {e}")))?
    }
}

/// No-op secret store for headless environments where OS keychain is unavailable.
/// Write operations return errors; reads return None.
pub struct NullSecretStore;

#[async_trait]
impl SecretStore for NullSecretStore {
    async fn store(&self, _ns: &str, _key: &str, _val: &str) -> Result<(), CoreError> {
        Err(CoreError::SecretStoreError(
            "OS keychain unavailable — OAuth credentials cannot be stored".into(),
        ))
    }

    async fn retrieve(&self, _ns: &str, _key: &str) -> Result<Option<String>, CoreError> {
        Ok(None)
    }

    async fn delete(&self, _ns: &str, _key: &str) -> Result<(), CoreError> {
        Ok(())
    }

    async fn delete_namespace(&self, _ns: &str) -> Result<(), CoreError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn registry_add_key_idempotent() {
        let mut reg = KeychainRegistry::new();
        reg.add_key("openai", "access_token");
        reg.add_key("openai", "access_token");
        assert_eq!(reg.keys_for("openai").len(), 1);
    }

    #[test]
    fn registry_remove_key_cleans_empty_namespace() {
        let mut reg = KeychainRegistry::new();
        reg.add_key("openai", "token");
        reg.remove_key("openai", "token");
        assert!(reg.all_namespaces().is_empty());
    }

    #[test]
    fn registry_remove_namespace_returns_keys() {
        let mut reg = KeychainRegistry::new();
        reg.add_key("openai", "access_token");
        reg.add_key("openai", "refresh_token");
        let keys = reg.remove_namespace("openai");
        assert_eq!(keys.len(), 2);
        assert!(keys.contains("access_token"));
        assert!(reg.all_namespaces().is_empty());
    }

    #[test]
    fn registry_save_and_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("registry.json");

        let mut reg = KeychainRegistry::new();
        reg.add_key("openai", "access_token");
        reg.add_key("openai", "refresh_token");
        reg.save(&path).unwrap();

        let loaded = KeychainRegistry::load_or_default(&path);
        assert_eq!(loaded.keys_for("openai").len(), 2);
    }

    #[test]
    fn registry_load_corrupt_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("registry.json");
        std::fs::write(&path, "NOT JSON").unwrap();

        let reg = KeychainRegistry::load_or_default(&path);
        assert!(reg.all_namespaces().is_empty());
    }

    #[test]
    fn registry_load_missing_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");

        let reg = KeychainRegistry::load_or_default(&path);
        assert!(reg.all_namespaces().is_empty());
    }

    #[test]
    fn registry_btreeset_sorted_deterministic() {
        let mut reg = KeychainRegistry::new();
        reg.add_key("openai", "z_key");
        reg.add_key("openai", "a_key");
        let keys: Vec<String> = reg.keys_for("openai").into_iter().collect();
        assert_eq!(keys, vec!["a_key", "z_key"]);
    }

    #[tokio::test]
    async fn null_store_rejects_writes() {
        let store = NullSecretStore;
        let result = store.store("openai", "token", "val").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn null_store_returns_none() {
        let store = NullSecretStore;
        let result = store.retrieve("openai", "token").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn null_store_delete_is_noop() {
        let store = NullSecretStore;
        store.delete("openai", "token").await.unwrap();
        store.delete_namespace("openai").await.unwrap();
    }

    /// Integration tests — require OS keychain. Run with:
    /// `cargo test -p oneshim-storage -- --ignored keychain`
    mod integration {
        use super::*;

        fn make_ops() -> (KeychainOps, TempDir) {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("registry.json");
            let ops = KeychainOps::new(path).unwrap();
            (ops, dir)
        }

        #[test]
        #[ignore]
        fn keychain_store_and_retrieve() {
            let (ops, _dir) = make_ops();
            let ns = "test_oneshim_integration";
            ops.store_sync(ns, "test_key", "test_value").unwrap();
            let val = ops.retrieve_sync(ns, "test_key").unwrap();
            assert_eq!(val, Some("test_value".to_owned()));
            // Cleanup
            ops.delete_sync(ns, "test_key").unwrap();
        }

        #[test]
        #[ignore]
        fn keychain_delete_returns_none() {
            let (ops, _dir) = make_ops();
            let ns = "test_oneshim_integration";
            ops.store_sync(ns, "del_key", "val").unwrap();
            ops.delete_sync(ns, "del_key").unwrap();
            let val = ops.retrieve_sync(ns, "del_key").unwrap();
            assert_eq!(val, None);
        }

        #[test]
        #[ignore]
        fn keychain_delete_namespace_clears_all() {
            let (ops, _dir) = make_ops();
            let ns = "test_oneshim_ns_delete";
            ops.store_sync(ns, "access_token", "a").unwrap();
            ops.store_sync(ns, "refresh_token", "b").unwrap();
            ops.delete_namespace_sync(ns).unwrap();
            assert_eq!(ops.retrieve_sync(ns, "access_token").unwrap(), None);
            assert_eq!(ops.retrieve_sync(ns, "refresh_token").unwrap(), None);
        }

        #[test]
        #[ignore]
        fn keychain_probe_availability() {
            let (ops, _dir) = make_ops();
            ops.probe_availability().unwrap();
        }
    }
}
