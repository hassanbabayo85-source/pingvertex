use crate::enums::{AnonymityLevel, ProxyCheckOutcome, ProxyFailureKind};
use serde::{Deserialize, Serialize};

/// Safe, client-facing result data for one proxy check.
/// No credentials or decrypted secret material is ever represented here.
///
/// # Backward compatibility (migration)
///
/// Added seven new fields. Every one of them is `Option<T>`, and
/// every one of them is serialized by serde with its natural `Option`
/// default (`None`) — which means JSONB rows written BEFORE
/// which do not carry these keys, deserialize cleanly with `None`. No
/// data migration is required, and no old row will fail to load.
///
/// The three legacy fields (`reachable`, `error_code`, `error_message`)
/// are preserved with their original semantics so that existing API
/// clients, tests, and dashboards keep working unchanged:
///
///   * `reachable` — `true` when BOTH the proxy and the target through it
///     succeeded. Still the "fully verified" bit it always was.
///   * `error_code` — free-form string, still populated. Callers should
///     prefer [`ProxyResult::effective_failure_kind`] going forward.
///   * `error_message` — human-readable text, still populated.
///
/// New consumers should read:
///
///   * [`ProxyResult::effective_outcome`] to distinguish "proxy is dead"
///     from "proxy is fine but the target was unreachable". The whole
/// point of is that a target failure is NOT a dead proxy.
///   * [`ProxyResult::effective_failure_kind`] to get a structured reason
///     without matching on the free-form `error_code` string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyResult {
    // ---- Legacy fields (unchanged) -----------------------------------
    pub host: String,
    pub port: u16,
    pub scheme: String,
    /// `true` when the proxy AND the target through it both succeeded.
    pub reachable: bool,
    /// Legacy total-latency field. Retained for backward compatibility;
    /// new code should read [`ProxyResult::total_time_ms`] and
    /// treat this as its legacy alias.
    pub latency_ms: Option<u32>,
    pub supports_https: bool,
    pub country_code: Option<String>,
    pub anonymity: Option<AnonymityLevel>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,

    // ---- additions --------------------------------------------
    /// Three-way classification — `Working` / `WorkingTargetUnreachable`
    /// / `ProxyFailed`. `None` on rows written previously; use
    /// [`ProxyResult::effective_outcome`] to get a value in that case.
    #[serde(default)]
    pub outcome: Option<ProxyCheckOutcome>,

    /// Structured failure reason. `None` on success and on rows written
    /// previously. Use [`ProxyResult::effective_failure_kind`] to get
    /// a value that also covers legacy rows.
    #[serde(default)]
    pub failure_kind: Option<ProxyFailureKind>,

    /// Time, in milliseconds, spent establishing the TCP connection to
    /// the proxy. Never includes time spent talking to the target.
    #[serde(default)]
    pub connection_latency_ms: Option<u32>,

    /// Time, in milliseconds, spent on the TLS handshake:
    ///   * for HTTPS proxies, the handshake with the proxy itself;
    ///   * for any tunneled scheme (HTTP CONNECT / SOCKS4 / SOCKS5),
    ///     the handshake with the target through the tunnel.
    /// `None` when no TLS handshake was attempted (e.g. an HTTP-only
    /// check that never got to the tunnel step).
    #[serde(default)]
    pub tls_handshake_latency_ms: Option<u32>,

    /// Time, in milliseconds, from when the first verification byte was
    /// written to when the first response byte arrived. For an HTTP
    /// absolute-form check this is the proxy's own TTFB; for a CONNECT
    /// or SOCKS handshake this is the TTFB of the handshake response.
    #[serde(default)]
    pub ttfb_ms: Option<u32>,

    /// Total wall-clock time, in milliseconds, for the entire check.
    /// Always >= every other timing field on the same row.
    #[serde(default)]
    pub total_time_ms: Option<u32>,

    /// HTTP status code observed during verification. For an HTTP proxy
    /// check this is the status of the absolute-form response; for
    /// CONNECT-based checks it is the status of the CONNECT reply (200
    /// on success). `None` when no HTTP layer was involved (e.g. a
    /// pure SOCKS handshake failure).
    #[serde(default)]
    pub http_status_code: Option<u16>,
}

impl ProxyResult {
    /// Three-way outcome, working around legacy rows that predate
    /// the `outcome` field by deriving it from the legacy `reachable`
    /// and `error_code` fields.
    ///
    /// Legacy derivation rule:
    ///   * `reachable == true`                     -> `Working`
    ///   * `reachable == false`, code starts with
    ///     `TARGET_`                               -> `WorkingTargetUnreachable`
    ///   * anything else                           -> `ProxyFailed`
    pub fn effective_outcome(&self) -> ProxyCheckOutcome {
        if let Some(outcome) = self.outcome {
            return outcome;
        }
        if self.reachable {
            return ProxyCheckOutcome::Working;
        }
        match self.error_code.as_deref() {
            Some(code) if code.starts_with("TARGET_") => {
                ProxyCheckOutcome::WorkingTargetUnreachable
            }
            _ => ProxyCheckOutcome::ProxyFailed,
        }
    }

    /// `true` when the proxy itself is alive — regardless of whether the
    /// target through it could be reached.
    ///
    /// This is the correct predicate for "should I keep this proxy in
    /// my list?". It deliberately returns `true` for
    /// `WorkingTargetUnreachable`, because a target failure is not a
    /// proxy failure.
    pub fn is_alive(&self) -> bool {
        self.effective_outcome().proxy_is_alive()
    }

    /// `true` only when BOTH the proxy and the target through it worked.
    ///
    /// This is the correct predicate for "fully verified". Do not use it
    /// to decide whether a proxy is usable — use [`Self::is_alive`].
    pub fn is_fully_working(&self) -> bool {
        self.effective_outcome().fully_working()
    }

    /// `true` only when the failure was specifically on the target side.
    pub fn is_target_failure(&self) -> bool {
        self.effective_outcome().is_target_failure()
    }

    /// Structured failure kind, working around legacy rows that
    /// predate the `failure_kind` field by mapping the legacy
    /// `error_code` string onto the closest matching variant.
    ///
    /// Returns `None` only when there is no failure at all — i.e. the
    /// outcome is `Working` and `error_code` is unset.
    pub fn effective_failure_kind(&self) -> Option<ProxyFailureKind> {
        if let Some(kind) = self.failure_kind {
            return Some(kind);
        }
        if self.effective_outcome().fully_working() {
            // Working outcome always implies "no failure", even if an
            // informational error_code is present (e.g. "the HTTP proxy
            // works but its HTTPS tunnel does not" — that is a warning,
            // not a failure).
            return None;
        }
        Some(match self.error_code.as_deref() {
            Some("PRIVATE_IP_BLOCKED") => ProxyFailureKind::SsrfBlocked,
            Some("INVALID_PROXY") => ProxyFailureKind::InvalidInput,
            Some("DNS_FAILED") => ProxyFailureKind::DnsFailure,
            Some("CONNECTION_REFUSED") => ProxyFailureKind::ConnectionRefused,
            Some("TARGET_TIMEOUT") | Some("CONNECTION_TIMEOUT") => ProxyFailureKind::Timeout,
            Some("AUTH_FAILED") => ProxyFailureKind::AuthFailed,
            Some("TLS_FAILED") => ProxyFailureKind::TlsFailure,
            Some("HTTP_PROXY_FAILED")
            | Some("HTTPS_PROXY_FAILED")
            | Some("SOCKS4_FAILED")
            | Some("SOCKS5_FAILED") => ProxyFailureKind::ProtocolFailure,
            _ => ProxyFailureKind::Other,
        })
    }

    /// Best-effort total time, preferring the field but falling
    /// back to the legacy `latency_ms` when reading a legacy row.
    pub fn effective_total_time_ms(&self) -> Option<u32> {
        self.total_time_ms.or(self.latency_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> ProxyResult {
        ProxyResult {
            host: "1.2.3.4".into(),
            port: 8080,
            scheme: "http".into(),
            reachable: true,
            latency_ms: Some(120),
            supports_https: true,
            country_code: None,
            anonymity: None,
            error_code: None,
            error_message: None,
            outcome: None,
            failure_kind: None,
            connection_latency_ms: None,
            tls_handshake_latency_ms: None,
            ttfb_ms: None,
            total_time_ms: None,
            http_status_code: None,
        }
    }

    #[test]
    fn effective_outcome_prefers_explicit_value() {
        let mut r = base();
        r.reachable = false;
        r.outcome = Some(ProxyCheckOutcome::WorkingTargetUnreachable);
        assert_eq!(
            r.effective_outcome(),
            ProxyCheckOutcome::WorkingTargetUnreachable
        );
        assert!(r.is_alive());
        assert!(!r.is_fully_working());
        assert!(r.is_target_failure());
    }

    #[test]
    fn pre_part3_row_with_target_prefix_maps_to_target_failure() {
        // Simulates a JSONB row written previously: `outcome` is
        // absent, `reachable` is false, and the legacy error_code
        // carries the TARGET_ prefix.
        let r = ProxyResult {
            outcome: None,
            reachable: false,
            error_code: Some("TARGET_TIMEOUT".into()),
            ..base()
        };
        assert_eq!(
            r.effective_outcome(),
            ProxyCheckOutcome::WorkingTargetUnreachable
        );
        assert!(r.is_alive(), "target failure must NOT be a dead proxy");
        assert_eq!(r.effective_failure_kind(), Some(ProxyFailureKind::Timeout));
    }

    #[test]
    fn pre_part3_row_without_target_prefix_maps_to_proxy_failed() {
        let r = ProxyResult {
            outcome: None,
            reachable: false,
            error_code: Some("CONNECTION_REFUSED".into()),
            ..base()
        };
        assert_eq!(r.effective_outcome(), ProxyCheckOutcome::ProxyFailed);
        assert!(!r.is_alive());
        assert_eq!(
            r.effective_failure_kind(),
            Some(ProxyFailureKind::ConnectionRefused)
        );
    }

    #[test]
    fn pre_part3_successful_row_maps_to_working() {
        let r = base();
        assert_eq!(r.effective_outcome(), ProxyCheckOutcome::Working);
        assert!(r.is_alive());
        assert!(r.is_fully_working());
        assert_eq!(r.effective_failure_kind(), None);
    }

    #[test]
    fn effective_total_time_falls_back_to_legacy_latency() {
        let r = base();
        assert_eq!(r.effective_total_time_ms(), Some(120));

        let r2 = ProxyResult {
            total_time_ms: Some(200),
            ..base()
        };
        assert_eq!(r2.effective_total_time_ms(), Some(200));
    }

    #[test]
    fn serde_roundtrip_preserves_all_fields() {
        let r = ProxyResult {
            outcome: Some(ProxyCheckOutcome::Working),
            failure_kind: None,
            connection_latency_ms: Some(30),
            tls_handshake_latency_ms: Some(50),
            ttfb_ms: Some(100),
            total_time_ms: Some(120),
            http_status_code: Some(200),
            ..base()
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: ProxyResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn deserializing_pre_part3_json_yields_none_for_new_fields() {
        // Simulates a JSONB row written previously.
        let json = r#"{
            "host":"1.2.3.4",
            "port":8080,
            "scheme":"http",
            "reachable":false,
            "latency_ms":120,
            "supports_https":false,
            "country_code":null,
            "anonymity":null,
            "error_code":"TARGET_TIMEOUT",
            "error_message":"target timed out"
        }"#;
        let r: ProxyResult = serde_json::from_str(json).unwrap();
        assert_eq!(r.outcome, None);
        assert_eq!(r.failure_kind, None);
        assert_eq!(r.connection_latency_ms, None);
        assert_eq!(r.tls_handshake_latency_ms, None);
        assert_eq!(r.ttfb_ms, None);
        assert_eq!(r.total_time_ms, None);
        assert_eq!(r.http_status_code, None);
        // And the backward-compat helpers still classify correctly:
        assert_eq!(
            r.effective_outcome(),
            ProxyCheckOutcome::WorkingTargetUnreachable
        );
        assert!(r.is_alive());
    }
}
