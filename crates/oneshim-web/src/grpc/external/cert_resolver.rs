//! ResolvesServerCert impl holding an Arc<ArcSwap<Arc<CertifiedKey>>>.
//! Swap is atomic — live connections keep the cert they started with;
//! new connections get the latest. No ServerConfig rebuild needed.

use std::sync::Arc;

use arc_swap::ArcSwap;
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;

#[derive(Debug)]
pub struct HotReloadCertResolver {
    current: ArcSwap<CertifiedKey>,
}

impl HotReloadCertResolver {
    pub fn new(initial: Arc<CertifiedKey>) -> Self {
        Self {
            current: ArcSwap::new(initial),
        }
    }

    pub fn swap(&self, next: Arc<CertifiedKey>) {
        self.current.store(next);
    }

    pub fn current(&self) -> Arc<CertifiedKey> {
        self.current.load_full()
    }
}

impl ResolvesServerCert for HotReloadCertResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.current.load_full())
    }
}

#[cfg(all(test, feature = "external-grpc-tools"))]
mod tests {
    use super::*;
    use rustls::sign::CertifiedKey;
    use std::sync::Arc;

    fn fixture_certified_key() -> Arc<CertifiedKey> {
        // rcgen 0.14 API (matches pattern in crates/oneshim-network/src/sync/lan_tls.rs)
        use rcgen::{CertificateParams, KeyPair};
        let key_pair = KeyPair::generate().expect("keypair");
        let params = CertificateParams::new(vec!["localhost".into()]).expect("params");
        let cert = params.self_signed(&key_pair).expect("self-signed");

        let cert_der = rustls::pki_types::CertificateDer::from(cert.der().to_vec());
        let key_pkcs8_der = key_pair.serialize_der();
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(key_pkcs8_der).expect("key");
        let signing_key =
            rustls::crypto::aws_lc_rs::sign::any_supported_type(&key_der).expect("signer");
        Arc::new(CertifiedKey::new(vec![cert_der], signing_key))
    }

    #[test]
    fn resolver_returns_current_key() {
        let key = fixture_certified_key();
        let resolver = HotReloadCertResolver::new(key.clone());
        let resolved = resolver.current();
        assert!(Arc::ptr_eq(&resolved, &key));
    }

    #[test]
    fn resolver_swap_changes_current() {
        let key1 = fixture_certified_key();
        let resolver = HotReloadCertResolver::new(key1.clone());
        let key2 = fixture_certified_key();
        resolver.swap(key2.clone());
        let resolved = resolver.current();
        assert!(Arc::ptr_eq(&resolved, &key2));
        assert!(!Arc::ptr_eq(&resolved, &key1));
    }
}
