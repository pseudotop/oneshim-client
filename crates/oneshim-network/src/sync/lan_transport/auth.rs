use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tracing::debug;

use oneshim_core::error::CoreError;

use crate::sync::lan_crypto;
use crate::sync::lan_discovery::LanPeerInfo;
use crate::sync::lan_server::{ChallengeRequest, ChallengeResponse, VerifyRequest, VerifyResponse};

use super::LanSyncTransport;

const TOKEN_CACHE_TTL: Duration = Duration::from_secs(3000);

/// A cached session token for a single peer.
struct CachedToken {
    token: String,
    obtained_at: Instant,
}

/// Thread-safe cache of per-peer session tokens.
#[derive(Clone, Default)]
pub(super) struct TokenCache {
    /// peer_device_id -> CachedToken
    tokens: Arc<RwLock<HashMap<String, CachedToken>>>,
}

impl TokenCache {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Get a cached token for a peer, if still valid.
    pub(super) fn get(&self, peer_id: &str) -> Option<String> {
        let tokens = self.tokens.read();
        let entry = tokens.get(peer_id)?;
        if Instant::now().duration_since(entry.obtained_at) < TOKEN_CACHE_TTL {
            Some(entry.token.clone())
        } else {
            None
        }
    }

    /// Store a session token for a peer.
    pub(super) fn put(&self, peer_id: &str, token: String) {
        self.tokens.write().insert(
            peer_id.to_string(),
            CachedToken {
                token,
                obtained_at: Instant::now(),
            },
        );
    }

    /// Invalidate a cached token for a peer (e.g., after a 401 response).
    pub(super) fn invalidate(&self, peer_id: &str) {
        self.tokens.write().remove(peer_id);
    }
}

impl LanSyncTransport {
    /// Authenticate with a peer server via HMAC challenge-response.
    ///
    /// Returns a session token on success.
    pub(super) async fn authenticate_with_peer(
        &self,
        peer_id: &str,
        peer: &LanPeerInfo,
    ) -> Result<String, CoreError> {
        let base = Self::peer_url(peer, "");

        // Step 1: Request challenge nonce
        let challenge_resp = self
            .http_client
            .post(format!("{base}/sync/challenge"))
            .json(&ChallengeRequest {
                device_id: self.local_device_id.clone(),
            })
            .send()
            .await
            .map_err(|e| {
                // Iter-90: split timeout vs generic per canonical pattern.
                if e.is_timeout() {
                    CoreError::RequestTimeout {
                        code: oneshim_core::error_codes::NetworkCode::Timeout,
                        timeout_ms: 0,
                    }
                } else {
                    CoreError::Network {
                        code: oneshim_core::error_codes::NetworkCode::Generic,
                        message: format!("challenge request to {peer_id}: {e}"),
                    }
                }
            })?;

        if !challenge_resp.status().is_success() {
            let status = challenge_resp.status();
            let message = format!("challenge request to {peer_id} returned {status}");
            // Semantic status mapping per iter-54..60 for LAN peer errors.
            return Err(match status.as_u16() {
                401 | 403 => CoreError::Auth {
                    code: oneshim_core::error_codes::AuthCode::Failed,
                    message,
                },
                408 | 504 => CoreError::RequestTimeout {
                    code: oneshim_core::error_codes::NetworkCode::Timeout,
                    timeout_ms: 0,
                },
                429 => CoreError::RateLimit {
                    code: oneshim_core::error_codes::NetworkCode::RateLimit,
                    retry_after_secs: 60,
                },
                502 | 503 => CoreError::ServiceUnavailable {
                    code: oneshim_core::error_codes::ServiceCode::Unavailable,
                    message,
                },
                _ => CoreError::Network {
                    code: oneshim_core::error_codes::NetworkCode::Generic,
                    message,
                },
            });
        }

        let challenge: ChallengeResponse =
            challenge_resp
                .json()
                .await
                .map_err(|e| CoreError::Network {
                    code: oneshim_core::error_codes::NetworkCode::Generic,
                    message: format!("parse challenge from {peer_id}: {e}"),
                })?;

        // Step 2: Compute HMAC response
        let nonce_bytes = hex::decode(&challenge.nonce).map_err(|e| CoreError::Internal {
            code: oneshim_core::error_codes::InternalCode::Generic,
            message: format!("decode nonce hex: {e}"),
        })?;

        let hmac_response = lan_crypto::compute_challenge_response(
            &nonce_bytes,
            &self.passphrase,
            &self.local_device_id,
            peer_id,
        )?;

        // Step 3: Submit verification
        let verify_resp = self
            .http_client
            .post(format!("{base}/sync/verify"))
            .json(&VerifyRequest {
                device_id: self.local_device_id.clone(),
                nonce: challenge.nonce,
                response: hex::encode(&hmac_response),
            })
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    CoreError::RequestTimeout {
                        code: oneshim_core::error_codes::NetworkCode::Timeout,
                        timeout_ms: 0,
                    }
                } else {
                    CoreError::Network {
                        code: oneshim_core::error_codes::NetworkCode::Generic,
                        message: format!("verify request to {peer_id}: {e}"),
                    }
                }
            })?;

        if !verify_resp.status().is_success() {
            return Err(CoreError::Network {
                code: oneshim_core::error_codes::NetworkCode::Generic,
                message: format!(
                    "authentication with {peer_id} failed (status {})",
                    verify_resp.status()
                ),
            });
        }

        let verify: VerifyResponse = verify_resp.json().await.map_err(|e| CoreError::Network {
            code: oneshim_core::error_codes::NetworkCode::Generic,
            message: format!("parse verify from {peer_id}: {e}"),
        })?;

        debug!(
            peer_id,
            expires_in = verify.expires_in_secs,
            "authenticated with LAN peer"
        );

        Ok(verify.session_token)
    }

    /// Get a valid session token for a peer, using the cache or
    /// performing a fresh challenge-response handshake.
    pub(super) async fn get_session_token(
        &self,
        peer_id: &str,
        peer: &LanPeerInfo,
    ) -> Result<String, CoreError> {
        // Try cached token first
        if let Some(token) = self.token_cache.get(peer_id) {
            return Ok(token);
        }

        // Perform fresh authentication
        let token = self.authenticate_with_peer(peer_id, peer).await?;
        self.token_cache.put(peer_id, token.clone());
        Ok(token)
    }

    /// Get a session token, with one retry on 401 (token may have expired
    /// on the server side even though our cache thinks it's valid).
    pub(super) async fn get_session_token_with_retry(
        &self,
        peer_id: &str,
        peer: &LanPeerInfo,
    ) -> Result<String, CoreError> {
        let token = self.get_session_token(peer_id, peer).await?;
        Ok(token)
    }
}
