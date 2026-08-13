//! TLS certificate inspection for SSL expiry monitoring.
//!
//! This module is used to power the "SSL certificate expiry" monitor
//! feature (`Monitor.ssl_check_enabled` / `Monitor.ssl_expiry_warning_days`,
//! `Check.cert_expires_at` / `Check.cert_days_remaining`). It performs a
//! real TLS handshake against a target and reads the leaf certificate's
//! `notAfter` timestamp so callers can compute how many days remain before
//! the certificate expires.
//!
//! ## SSRF note
//!
//! This module does **not** perform any SSRF validation itself. It is
//! designed to be called with a [`std::net::SocketAddr`] that has already
//! been resolved and validated by the caller, exactly like the pattern
//! already used in `checker.rs`, where
//! `crate::ssrf::resolve_and_validate_destination` is called before ever
//! opening a connection. Any future caller of [`fetch_certificate_info`]
//! MUST perform that same validation first; this function will happily
//! connect to whatever `SocketAddr` it is given.
//!
//! ## Expired / invalid certificates
//!
//! [`fetch_certificate_info`] uses a real `rustls` `ClientConfig` backed by
//! the platform's native root store, and does **not** disable certificate
//! verification. This means an expired, self-signed, or otherwise invalid
//! certificate chain will typically cause the TLS handshake itself to fail
//! (rustls rejects the certificate before the connection is usable), and
//! this function will return an `Err` rather than a `CertInfo` with a
//! negative `days_remaining`. This is expected and acceptable behavior for
//! this feature: a handshake failure is itself strong evidence that
//! something is wrong with the certificate, and the caller can treat it as
//! a check failure. `days_remaining` going negative is only observable in
//! the (rare) case where a cert has already expired by a very small margin
//! and the local clock still permits the handshake before verification
//! catches it.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The leaf certificate's expiry information for a TLS endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertInfo {
    /// The certificate's `notAfter` timestamp.
    pub expires_at: DateTime<Utc>,
    /// Whole days between "now" and `expires_at`. Negative if the
    /// certificate has already expired (see module docs: in practice this
    /// usually surfaces as a handshake error instead, not a negative
    /// value here).
    pub days_remaining: i64,
}

/// Connect to `socket_addr` over TLS (using `hostname` for SNI / server
/// name verification) and return the leaf certificate's expiry info.
///
/// `socket_addr` is expected to have already been through SSRF validation
/// by the caller (see module docs) — this function performs none itself.
///
/// The TCP connect and the full TLS handshake are both bounded by
/// `timeout`.
pub async fn fetch_certificate_info(
    hostname: &str,
    socket_addr: SocketAddr,
    timeout_duration: Duration,
) -> Result<CertInfo, BoxError> {
    let client_config = production_client_config()?;
    fetch_certificate_info_with_config(hostname, socket_addr, timeout_duration, client_config).await
}

/// Shared implementation. Split out so tests can inject a `ClientConfig`
/// with a custom root store (trusting a test server's self-signed cert)
/// while the public production entry point above always uses the real
/// platform root store. This keeps production code free of any
/// verification-related test hooks.
async fn fetch_certificate_info_with_config(
    hostname: &str,
    socket_addr: SocketAddr,
    timeout_duration: Duration,
    client_config: ClientConfig,
) -> Result<CertInfo, BoxError> {
    let server_name = ServerName::try_from(hostname.to_string())
        .map_err(|e| format!("invalid hostname for TLS SNI '{hostname}': {e}"))?;

    let connector = TlsConnector::from(Arc::new(client_config));

    let tcp_stream = timeout(timeout_duration, TcpStream::connect(socket_addr))
        .await
        .map_err(|_| format!("timed out connecting to {socket_addr}"))??;

    let tls_stream = timeout(timeout_duration, connector.connect(server_name, tcp_stream))
        .await
        .map_err(|_| format!("timed out during TLS handshake with {socket_addr}"))??;

    // Leaf (first) peer certificate, DER-encoded.
    let (_io, connection) = tls_stream.get_ref();
    let peer_certs = connection
        .peer_certificates()
        .ok_or("TLS handshake completed but no peer certificates were presented")?;
    let leaf_der = peer_certs
        .first()
        .ok_or("peer certificate chain was empty")?;

    let (_, parsed) = x509_parser::parse_x509_certificate(leaf_der.as_ref())
        .map_err(|e| format!("failed to parse leaf certificate DER: {e}"))?;

    let not_after = parsed.validity().not_after;
    let expires_at = DateTime::<Utc>::from_timestamp(not_after.timestamp(), 0)
        .ok_or("certificate notAfter timestamp is out of range")?;

    let now = Utc::now();
    let days_remaining = (expires_at - now).num_days();

    Ok(CertInfo {
        expires_at,
        days_remaining,
    })
}

/// Install rustls's default crypto provider (`aws-lc-rs`, matching what the
/// rest of this workspace already pulls in via reqwest's `rustls-tls`
/// feature) exactly once per process. `ClientConfig::builder()` panics if
/// no provider has been installed and none could be auto-detected, which
/// can happen in tests that build many `ClientConfig`s in isolation.
fn ensure_crypto_provider_installed() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Ignore the error: it only fails if a provider was already
        // installed (e.g. by another crate earlier in the process), which
        // is fine — we just need *some* provider installed.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// The `ClientConfig` used in production: real certificate verification
/// against the platform's native root store. Never disables verification.
fn production_client_config() -> Result<ClientConfig, BoxError> {
    ensure_crypto_provider_installed();

    let mut root_store = rustls::RootCertStore::empty();

    let native_certs = rustls_native_certs::load_native_certs();
    for err in native_certs.errors.iter() {
        tracing::warn!(error = %err, "error loading a native root certificate");
    }
    for cert in native_certs.certs {
        // Ignore individual malformed roots rather than failing the whole
        // store — this mirrors rustls-native-certs' own documented usage.
        let _ = root_store.add(cert);
    }

    if root_store.is_empty() {
        return Err("no native root certificates could be loaded".into());
    }

    Ok(ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::{IpAddr, Ipv4Addr, TcpListener as StdTcpListener};

    use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    /// Generate a self-signed cert (via `rcgen`) valid for ~30 more days,
    /// spin up a local TLS echo-ish server (it doesn't need to send any
    /// application data — `fetch_certificate_info` only needs the
    /// handshake to complete), and return the bound address plus the
    /// server task handle.
    async fn spawn_test_tls_server() -> (SocketAddr, CertificateDer<'static>) {
        ensure_crypto_provider_installed();

        let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::days(1);
        params.not_after = now + time::Duration::days(30);

        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key_pair).unwrap();

        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = PrivatePkcs8KeyDer::from(key_pair.serialize_der());

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert_der.clone()],
                rustls::pki_types::PrivateKeyDer::Pkcs8(key_der),
            )
            .expect("valid self-signed server config");

        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        let std_listener =
            StdTcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind test server");
        std_listener.set_nonblocking(true).unwrap();
        let listener = TcpListener::from_std(std_listener).expect("tokio listener");
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                // Complete the handshake; the client only needs the
                // certificate, not any application data.
                let _ = acceptor.accept(stream).await;
            }
        });

        (addr, cert_der)
    }

    /// Build a `ClientConfig` that trusts only the given self-signed test
    /// certificate as a root, instead of the platform root store. This is
    /// the "(ii) install the test server's self-signed cert as a trusted
    /// root" approach from the task description — it was simpler than
    /// threading a `#[cfg(test)]`-only parameter through the public API,
    /// and keeps `fetch_certificate_info`'s signature untouched.
    fn test_client_config(trusted_cert: &CertificateDer<'static>) -> ClientConfig {
        ensure_crypto_provider_installed();

        let mut root_store = rustls::RootCertStore::empty();
        root_store
            .add(trusted_cert.clone())
            .expect("add self-signed test cert to root store");

        ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth()
    }

    #[tokio::test]
    async fn returns_days_remaining_for_a_valid_certificate() {
        let (addr, cert_der) = spawn_test_tls_server().await;
        let client_config = test_client_config(&cert_der);

        let info = fetch_certificate_info_with_config(
            "localhost",
            addr,
            Duration::from_secs(5),
            client_config,
        )
        .await
        .expect("fetch_certificate_info should succeed against the test server");

        assert!(
            (29..=31).contains(&info.days_remaining),
            "expected days_remaining to be approximately 30, got {}",
            info.days_remaining
        );
    }

    #[tokio::test]
    async fn returns_err_for_unreachable_address_within_timeout() {
        // Bind to port 0 to get a free port, then immediately drop the
        // listener so the port is (almost certainly) refused/unreachable.
        let std_listener =
            StdTcpListener::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp port");
        let addr = std_listener.local_addr().unwrap();
        drop(std_listener);

        let started = std::time::Instant::now();
        let result = fetch_certificate_info("localhost", addr, Duration::from_secs(3)).await;
        let elapsed = started.elapsed();

        assert!(result.is_err(), "expected an error for a closed port");
        assert!(
            elapsed < Duration::from_secs(5),
            "expected fetch_certificate_info to fail quickly, took {elapsed:?}"
        );
    }
}
