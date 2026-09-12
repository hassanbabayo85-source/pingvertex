//! Builds the `axum-server` Rustls configuration for the internal-agent
//! mTLS listener.
//!
//! Unlike the plain HTTP listener (`main.rs`'s `axum::serve`, used for
//! every other route), this listener REQUIRES the client to present a
//! certificate signed by our own internal CA (`certs/ca/ca-cert.pem`) —
//! built via `rustls::server::WebPkiClientVerifier`, which enforces
//! mandatory client auth (there is no "optional" variant used here: an
//! internal agent with no cert, or a cert not signed by our CA, must be
//! rejected at the TLS layer, before any application code — including
//! `agent_auth` — ever runs).

use std::sync::Arc;

use axum_server::tls_rustls::RustlsConfig;
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

/// Paths to the internal CA / server identity, all relative to the
/// process's working directory (`/app` in the container — see
/// `Dockerfile.api`, which will need to COPY `certs/` alongside the
/// binary).
pub struct MtlsPaths<'a> {
    pub ca_cert: &'a str,
    pub server_cert: &'a str,
    pub server_key: &'a str,
}

/// Build the Rustls server config used by the internal-agent mTLS
/// listener: presents `server_cert`/`server_key` as this server's
/// identity, and requires every connecting client to present a
/// certificate chaining up to `ca_cert`.
pub fn build_mtls_config(paths: MtlsPaths<'_>) -> anyhow::Result<RustlsConfig> {
    // --- Trust root: the client certs we accept must chain to this CA. ---
    let ca_pem = std::fs::read(paths.ca_cert)
        .map_err(|e| anyhow::anyhow!("failed to read CA cert {}: {e}", paths.ca_cert))?;
    let ca_certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&ca_pem)
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("failed to parse CA cert {}: {e}", paths.ca_cert))?;

    let mut root_store = RootCertStore::empty();
    for cert in ca_certs {
        root_store
            .add(cert)
            .map_err(|e| anyhow::anyhow!("failed to add CA cert to root store: {e}"))?;
    }

    let client_verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build mTLS client verifier: {e}"))?;

    // --- This server's own identity, presented to connecting agents. ---
    let server_cert_pem = std::fs::read(paths.server_cert)
        .map_err(|e| anyhow::anyhow!("failed to read server cert {}: {e}", paths.server_cert))?;
    let server_certs: Vec<CertificateDer<'static>> =
        CertificateDer::pem_slice_iter(&server_cert_pem)
            .collect::<Result<_, _>>()
            .map_err(|e| anyhow::anyhow!("failed to parse server cert: {e}"))?;

    let server_key_pem = std::fs::read(paths.server_key)
        .map_err(|e| anyhow::anyhow!("failed to read server key {}: {e}", paths.server_key))?;
    let server_key = PrivateKeyDer::from_pem_slice(&server_key_pem)
        .map_err(|e| anyhow::anyhow!("failed to parse server key: {e}"))?;

    let mut config = ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(server_certs, server_key)
        .map_err(|e| anyhow::anyhow!("failed to build mTLS server config: {e}"))?;

    // Required per `RustlsConfig::from_config`'s doc comment: ALPN is NOT
    // set automatically when building `ServerConfig` manually (only the
    // `from_pem_file`-style constructors do this for you).
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(RustlsConfig::from_config(Arc::new(config)))
}
