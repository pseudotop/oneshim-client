//! Secret store port — canonical storage for provider credentials.
//!
//! Secrets (OAuth tokens, API keys, bridge credentials) must not default to
//! plaintext config storage. This port abstracts OS keychain, explicit file
//! backends, environment-backed sources, or in-memory test doubles.

use async_trait::async_trait;

use crate::error::CoreError;

pub const DEFAULT_SECRET_PROFILE: &str = "default";
pub const PROVIDER_API_KEY_SECRET_KEY: &str = "api_key";
pub const PROVIDER_OAUTH_SESSION_SECRET_KEY: &str = "oauth_session";

fn validate_secret_segment(raw: &str, field_name: &str) -> Result<String, CoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidArguments(format!(
            "{field_name} must not be empty"
        )));
    }

    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(CoreError::InvalidArguments(format!(
            "{field_name} must contain only ASCII alphanumeric characters, '.', '_' or '-'"
        )));
    }

    Ok(trimmed.to_string())
}

pub fn provider_secret_namespace(provider_id: &str, profile_id: &str) -> Result<String, CoreError> {
    let provider_id = validate_secret_segment(provider_id, "provider_id")?;
    let profile_id = validate_secret_segment(profile_id, "profile_id")?;
    Ok(format!("provider/{provider_id}/{profile_id}"))
}

pub fn provider_api_key_secret_ref(
    provider_id: &str,
    profile_id: &str,
) -> Result<(String, &'static str), CoreError> {
    Ok((
        provider_secret_namespace(provider_id, profile_id)?,
        PROVIDER_API_KEY_SECRET_KEY,
    ))
}

pub fn provider_oauth_session_secret_ref(
    provider_id: &str,
    profile_id: &str,
) -> Result<(String, &'static str), CoreError> {
    Ok((
        provider_secret_namespace(provider_id, profile_id)?,
        PROVIDER_OAUTH_SESSION_SECRET_KEY,
    ))
}

pub fn secret_env_var_name(namespace: &str, key: &str) -> String {
    let normalized_namespace = namespace
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let normalized_key = key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("ONESHIM_SECRET_{normalized_namespace}_{normalized_key}")
}

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

    #[test]
    fn provider_namespace_uses_stable_shape() {
        let namespace = provider_secret_namespace("openai", DEFAULT_SECRET_PROFILE).unwrap();
        assert_eq!(namespace, "provider/openai/default");
    }

    #[test]
    fn provider_secret_ref_uses_api_key_key_name() {
        let (namespace, key) = provider_api_key_secret_ref("openrouter", "team").unwrap();
        assert_eq!(namespace, "provider/openrouter/team");
        assert_eq!(key, PROVIDER_API_KEY_SECRET_KEY);
    }

    #[test]
    fn provider_namespace_rejects_invalid_segments() {
        let err = provider_secret_namespace("openai/codex", "default").unwrap_err();
        assert!(matches!(err, CoreError::InvalidArguments(_)));
    }

    #[test]
    fn env_secret_var_name_normalizes_namespace_and_key() {
        let env_name = secret_env_var_name("provider/openai/default", "api_key");
        assert_eq!(env_name, "ONESHIM_SECRET_PROVIDER_OPENAI_DEFAULT_API_KEY");
    }
}
