//! TLS ServerConfig builder + cert file loader.
//! The hot-reload mechanism lives in cert_resolver.rs (swap inner Arc).
//! This module provides the PEM → CertifiedKey path.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::sign::CertifiedKey;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TlsLoadError {
    #[error("read {path:?}: {source}")]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse cert PEM: {0}")]
    ParseCert(String),
    #[error("parse key PEM: {0}")]
    ParseKey(String),
    #[error("empty cert chain in {0:?}")]
    EmptyChain(std::path::PathBuf),
    #[error("no private key in {0:?}")]
    NoKey(std::path::PathBuf),
}

pub fn load_certified_key(
    cert_path: &Path,
    key_path: &Path,
) -> Result<Arc<CertifiedKey>, TlsLoadError> {
    let cert_pem = fs::read(cert_path).map_err(|e| TlsLoadError::Read {
        path: cert_path.to_owned(),
        source: e,
    })?;
    let key_pem = fs::read(key_path).map_err(|e| TlsLoadError::Read {
        path: key_path.to_owned(),
        source: e,
    })?;

    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_pem.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsLoadError::ParseCert(e.to_string()))?;
    if certs.is_empty() {
        return Err(TlsLoadError::EmptyChain(cert_path.to_owned()));
    }

    let private_key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_pem.as_slice())
        .map_err(|e| TlsLoadError::ParseKey(e.to_string()))?
        .ok_or_else(|| TlsLoadError::NoKey(key_path.to_owned()))?;

    let signing_key = aws_lc_rs::sign::any_supported_type(&private_key)
        .map_err(|e| TlsLoadError::ParseKey(format!("no supported signer: {e:?}")))?;

    Ok(Arc::new(CertifiedKey::new(certs, signing_key)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_cert_pair(dir: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
        // rcgen 0.14 API
        use rcgen::{CertificateParams, KeyPair};
        let key_pair = KeyPair::generate().unwrap();
        let params = CertificateParams::new(vec!["localhost".into()]).unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");
        fs::write(&cert_path, cert.pem()).unwrap();
        fs::write(&key_path, key_pair.serialize_pem()).unwrap();
        (cert_path, key_path)
    }

    #[test]
    fn load_certified_key_from_pem() {
        let dir = TempDir::new().unwrap();
        let (cert_path, key_path) = write_cert_pair(&dir);
        let key = load_certified_key(&cert_path, &key_path).expect("load");
        assert_eq!(key.cert.len(), 1, "one leaf cert expected");
    }

    #[test]
    fn load_fails_on_missing_cert() {
        let result = load_certified_key(
            std::path::Path::new("/does/not/exist.pem"),
            std::path::Path::new("/does/not/exist.key"),
        );
        assert!(result.is_err());
    }
}
