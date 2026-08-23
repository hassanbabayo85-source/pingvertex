//! Small TLS connector helper for proxy protocol verification.
//!
//! This is deliberately separate from `crates/worker/src/tls_cert.rs`
//! (which is about SSL certificate expiry inspection). That module's
//! production config functions are private to it; rather than expose or
//! modify that module's internals, this file owns the tiny TLS connector
//! needed by `proxy_checker` to speak TLS to a proxy or through a tunnel.
//!
//! SSRF note: this helper performs NO DNS resolution or IP validation. It
//! connects only to a `SocketAddr` already validated by the caller via
//! `shared::ssrf` (the same rule as `tls_cert` and `checker`).

use std::sync::Arc;
use std::sync::Once;

use rustls::ClientConfig;
use tokio_rustls::TlsConnector;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn ensure_crypto_provider_installed() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Ignore the result: it only fails if a provider is already
        // installed (e.g. by another crate earlier in the process), which
        // is fine — we just need *some* provider installed.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

fn production_client_config() -> Result<ClientConfig, BoxError> {
    ensure_crypto_provider_installed();

    let mut root_store = rustls::RootCertStore::empty();

    let native_certs = rustls_native_certs::load_native_certs();
    for err in native_certs.errors.iter() {
        tracing::warn!(error = %err, "error loading a native root certificate");
    }
    for cert in native_certs.certs {
        let _ = root_store.add(cert);
    }

    if root_store.is_empty() {
        return Err("no native root certificates could be loaded".into());
    }

    Ok(ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth())
}

/// Returns a TLS connector configured with real certificate verification
/// against the platform native root store. Never disables verification.
pub(crate) fn tls_connector() -> Result<TlsConnector, BoxError> {
    let config = production_client_config()?;
    Ok(TlsConnector::from(Arc::new(config)))
}
