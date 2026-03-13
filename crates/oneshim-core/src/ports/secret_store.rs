//! Secret store port — secure storage for managed credentials.
//!
//! Secrets (OAuth tokens, refresh tokens) must never be stored in plaintext
//! config. This port abstracts OS keychain or in-memory storage.

use async_trait::async_trait;

use crate::error::CoreError;

/// Secure secret storage abstraction.
///
/// Implementations may use OS keychain (macOS Keychain, Windows Credential
/// Manager, Linux Secret Service) or an in-memory fallback.
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Store a secret value under a namespaced key.
    async fn store(&self, namespace: &str, key: &str, value: &str) -> Result<(), CoreError>;

    /// Retrieve a secret value. Returns `None` if not found.
    async fn retrieve(&self, namespace: &str, key: &str) -> Result<Option<String>, CoreError>;

    /// Delete a secret value. No-op if key does not exist.
    async fn delete(&self, namespace: &str, key: &str) -> Result<(), CoreError>;

    /// Delete all secrets under a namespace.
    async fn delete_namespace(&self, namespace: &str) -> Result<(), CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory secret store for testing.
    pub struct InMemorySecretStore {
        store: Mutex<HashMap<String, String>>,
    }

    impl InMemorySecretStore {
        pub fn new() -> Self {
            Self {
                store: Mutex::new(HashMap::new()),
            }
        }

        fn make_key(namespace: &str, key: &str) -> String {
            format!("{namespace}.{key}")
        }
    }

    #[async_trait]
    impl SecretStore for InMemorySecretStore {
        async fn store(&self, namespace: &str, key: &str, value: &str) -> Result<(), CoreError> {
            let mut map = self.store.lock().unwrap();
            map.insert(Self::make_key(namespace, key), value.to_string());
            Ok(())
        }

        async fn retrieve(&self, namespace: &str, key: &str) -> Result<Option<String>, CoreError> {
            let map = self.store.lock().unwrap();
            Ok(map.get(&Self::make_key(namespace, key)).cloned())
        }

        async fn delete(&self, namespace: &str, key: &str) -> Result<(), CoreError> {
            let mut map = self.store.lock().unwrap();
            map.remove(&Self::make_key(namespace, key));
            Ok(())
        }

        async fn delete_namespace(&self, namespace: &str) -> Result<(), CoreError> {
            let mut map = self.store.lock().unwrap();
            let prefix = format!("{namespace}.");
            map.retain(|k, _| !k.starts_with(&prefix));
            Ok(())
        }
    }

    #[tokio::test]
    async fn store_and_retrieve() {
        let store = InMemorySecretStore::new();
        store
            .store("openai", "access_token", "tok_abc")
            .await
            .unwrap();
        let val = store.retrieve("openai", "access_token").await.unwrap();
        assert_eq!(val, Some("tok_abc".to_string()));
    }

    #[tokio::test]
    async fn retrieve_missing_returns_none() {
        let store = InMemorySecretStore::new();
        let val = store.retrieve("openai", "missing").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn delete_removes_key() {
        let store = InMemorySecretStore::new();
        store.store("openai", "token", "val").await.unwrap();
        store.delete("openai", "token").await.unwrap();
        let val = store.retrieve("openai", "token").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn delete_namespace_removes_all_keys() {
        let store = InMemorySecretStore::new();
        store.store("openai", "access", "a").await.unwrap();
        store.store("openai", "refresh", "b").await.unwrap();
        store.store("other", "key", "c").await.unwrap();
        store.delete_namespace("openai").await.unwrap();
        assert_eq!(store.retrieve("openai", "access").await.unwrap(), None);
        assert_eq!(store.retrieve("openai", "refresh").await.unwrap(), None);
        assert_eq!(
            store.retrieve("other", "key").await.unwrap(),
            Some("c".to_string())
        );
    }

    #[tokio::test]
    async fn namespace_isolation() {
        let store = InMemorySecretStore::new();
        store.store("openai", "token", "openai_tok").await.unwrap();
        store.store("openrouter", "token", "or_tok").await.unwrap();
        assert_eq!(
            store.retrieve("openai", "token").await.unwrap(),
            Some("openai_tok".to_string())
        );
        assert_eq!(
            store.retrieve("openrouter", "token").await.unwrap(),
            Some("or_tok".to_string())
        );
    }
}
