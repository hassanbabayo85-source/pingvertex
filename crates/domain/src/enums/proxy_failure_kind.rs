use serde::{Deserialize, Serialize};

/// PART 3 — structured failure reason for a proxy check.
///
/// This is the machine-readable companion to `ProxyResult.error_code`
/// (a legacy free-form `String`) and `ProxyResult.error_message` (a
/// human-readable `String`). The enum exists so the frontend can
/// classify failures without string-matching on human text, and so
/// that callers can answer questions like "was this a timeout?" or
/// "was this the target's fault?" without pattern-matching prefixes.
///
/// `ProxyCheckOutcome` says WHICH SIDE failed (proxy vs target vs both
/// working). `ProxyFailureKind` says WHAT KIND of failure it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyFailureKind {
    /// Timeout — connecting to the proxy, during the protocol handshake,
    /// or waiting for the target through the proxy.
    Timeout,

    /// Proxy rejected the supplied credentials (HTTP 401/407, SOCKS5
    /// username/password auth failure).
    AuthFailed,

    /// The proxy refused the TCP connection outright.
    ConnectionRefused,

    /// DNS resolution of the proxy host failed, or the target hostname
    /// could not be resolved by the proxy.
    DnsFailure,

    /// TLS handshake failure — either against an HTTPS proxy (before any
    /// target was involved) or through the tunnel to the target.
    TlsFailure,

    /// Proxy protocol failure: bad SOCKS reply code, unexpected HTTP
    /// status during CONNECT, or malformed proxy response.
    ProtocolFailure,

    /// The target returned an unexpected HTTP status through the proxy.
    /// Only meaningful with `Outcome::WorkingTargetUnreachable` — the
    /// proxy itself is fine.
    TargetHttpError,

    /// Input was invalid (bad host syntax, disallowed port, unknown
    /// scheme). This is a caller-side problem, not a proxy problem.
    InvalidInput,

    /// SSRF guard rejected the destination (private IP, localhost,
    /// loopback via IPv6 encodings, etc.). Also a caller-side problem.
    SsrfBlocked,

    /// Any other failure not covered above. Callers that want to be
    /// exhaustive should still handle this.
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_uses_snake_case() {
        for (variant, expected) in [
            (ProxyFailureKind::Timeout, "\"timeout\""),
            (ProxyFailureKind::AuthFailed, "\"auth_failed\""),
            (ProxyFailureKind::ConnectionRefused, "\"connection_refused\""),
            (ProxyFailureKind::DnsFailure, "\"dns_failure\""),
            (ProxyFailureKind::TlsFailure, "\"tls_failure\""),
            (ProxyFailureKind::ProtocolFailure, "\"protocol_failure\""),
            (ProxyFailureKind::TargetHttpError, "\"target_http_error\""),
            (ProxyFailureKind::InvalidInput, "\"invalid_input\""),
            (ProxyFailureKind::SsrfBlocked, "\"ssrf_blocked\""),
            (ProxyFailureKind::Other, "\"other\""),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected);
            let back: ProxyFailureKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant);
        }
    }
}
