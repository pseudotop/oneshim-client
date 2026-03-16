use async_trait::async_trait;
use chrono::{DateTime, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    IntegrationAuthContext, IntegrationAuthScheme, IntegrationCapabilityScope,
};
use oneshim_core::ports::integration::IntegrationAuthPort;

use super::transport::{IntegrationRequestProof, IntegrationRequestProofFactory};

pub struct StaticIntegrationAuthPort {
    context: IntegrationAuthContext,
}

impl StaticIntegrationAuthPort {
    pub fn new(context: IntegrationAuthContext) -> Self {
        Self { context }
    }
}

#[async_trait]
impl IntegrationAuthPort for StaticIntegrationAuthPort {
    async fn resolve_session_auth(
        &self,
        _requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationAuthContext, CoreError> {
        let mut context = self.context.clone();
        if context.resource_indicator.is_none() {
            context.resource_indicator = resource_indicator.map(str::to_string);
        }
        Ok(context)
    }
}

pub struct EnvIntegrationAuthPort {
    token_env_var: String,
    scheme: IntegrationAuthScheme,
    expires_at: Option<DateTime<Utc>>,
    resource_indicator: Option<String>,
}

impl EnvIntegrationAuthPort {
    pub fn new(
        token_env_var: impl Into<String>,
        scheme: IntegrationAuthScheme,
        expires_at: Option<DateTime<Utc>>,
        resource_indicator: Option<String>,
    ) -> Self {
        Self {
            token_env_var: token_env_var.into(),
            scheme,
            expires_at,
            resource_indicator,
        }
    }
}

#[async_trait]
impl IntegrationAuthPort for EnvIntegrationAuthPort {
    async fn resolve_session_auth(
        &self,
        _requested_scopes: &[IntegrationCapabilityScope],
        resource_indicator: Option<&str>,
    ) -> Result<IntegrationAuthContext, CoreError> {
        let access_token = std::env::var(&self.token_env_var).map_err(|_| {
            CoreError::Auth(format!(
                "integration access token env var `{}` is not configured.",
                self.token_env_var
            ))
        })?;

        Ok(IntegrationAuthContext {
            access_token,
            scheme: self.scheme.clone(),
            expires_at: self.expires_at,
            resource_indicator: self
                .resource_indicator
                .clone()
                .or_else(|| resource_indicator.map(str::to_string)),
        })
    }
}

pub struct NoopIntegrationRequestProofFactory;

#[async_trait]
impl IntegrationRequestProofFactory for NoopIntegrationRequestProofFactory {
    async fn build_proof(
        &self,
        _auth: &IntegrationAuthContext,
        _method: &str,
        _url: &str,
    ) -> Result<Option<IntegrationRequestProof>, CoreError> {
        Ok(None)
    }
}

pub struct StaticIntegrationRequestProofFactory {
    proof: IntegrationRequestProof,
}

impl StaticIntegrationRequestProofFactory {
    pub fn new(header_name: impl Into<String>, header_value: impl Into<String>) -> Self {
        Self {
            proof: IntegrationRequestProof {
                header_name: header_name.into(),
                header_value: header_value.into(),
            },
        }
    }
}

#[async_trait]
impl IntegrationRequestProofFactory for StaticIntegrationRequestProofFactory {
    async fn build_proof(
        &self,
        _auth: &IntegrationAuthContext,
        _method: &str,
        _url: &str,
    ) -> Result<Option<IntegrationRequestProof>, CoreError> {
        Ok(Some(self.proof.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn static_auth_port_preserves_auth_context() {
        let port = StaticIntegrationAuthPort::new(IntegrationAuthContext {
            access_token: "token-1".to_string(),
            scheme: IntegrationAuthScheme::BearerToken,
            expires_at: None,
            resource_indicator: None,
        });

        let context = port
            .resolve_session_auth(
                &[IntegrationCapabilityScope::InsightWrite],
                Some("https://integration.example.com"),
            )
            .await
            .unwrap();

        assert_eq!(context.access_token, "token-1");
        assert_eq!(context.scheme, IntegrationAuthScheme::BearerToken);
        assert_eq!(
            context.resource_indicator.as_deref(),
            Some("https://integration.example.com")
        );
    }

    #[tokio::test]
    async fn env_auth_port_reads_token_from_env() {
        let env_name = "ONESHIM_TEST_INTEGRATION_TOKEN";
        unsafe {
            std::env::set_var(env_name, "token-from-env");
        }

        let port = EnvIntegrationAuthPort::new(
            env_name,
            IntegrationAuthScheme::DpopBearer,
            None,
            Some("https://integration.example.com".to_string()),
        );

        let context = port
            .resolve_session_auth(&[IntegrationCapabilityScope::SessionManage], None)
            .await
            .unwrap();

        assert_eq!(context.access_token, "token-from-env");
        assert_eq!(context.scheme, IntegrationAuthScheme::DpopBearer);

        unsafe {
            std::env::remove_var(env_name);
        }
    }

    #[tokio::test]
    async fn noop_proof_factory_returns_none() {
        let factory = NoopIntegrationRequestProofFactory;
        let proof = factory
            .build_proof(
                &IntegrationAuthContext {
                    access_token: "token".to_string(),
                    scheme: IntegrationAuthScheme::BearerToken,
                    expires_at: None,
                    resource_indicator: None,
                },
                "POST",
                "https://integration.example.com/bootstrap",
            )
            .await
            .unwrap();

        assert!(proof.is_none());
    }

    #[tokio::test]
    async fn static_proof_factory_returns_configured_header() {
        let factory = StaticIntegrationRequestProofFactory::new("dpop", "proof-value");
        let proof = factory
            .build_proof(
                &IntegrationAuthContext {
                    access_token: "token".to_string(),
                    scheme: IntegrationAuthScheme::DpopBearer,
                    expires_at: None,
                    resource_indicator: None,
                },
                "POST",
                "https://integration.example.com/bootstrap",
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(proof.header_name, "dpop");
        assert_eq!(proof.header_value, "proof-value");
    }
}
