use crate::errors::DomainError;
use std::net::IpAddr;

/// Hard cap on the byte length of a proxy host.
///
/// 253 characters is the practical maximum length of a fully-qualified DNS
/// hostname (255 bytes on the wire, minus the root label and a trailing
/// dot). Checked before any parsing so an oversized string never reaches
/// the (comparatively expensive) IP-literal parser below.
const MAX_PROXY_HOST_LENGTH: usize = 253;

fn is_valid_host_char(c: char) -> bool {
    // Deliberately minimal: alphanumeric characters, `.` and `-` for
    // hostnames, and `:` for IPv6 literals. No regex crate is available in
    // domain/Cargo.toml, and this feature does not need one — this is a
    // basic sanity filter, not a full hostname grammar validator.
    c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':'
}

/// A validated proxy host: either a literal IP address or a hostname.
///
/// # SSRF defense — this is LAYER 1 ONLY
///
/// `ProxyHost::new` is the *first* of two required layers of SSRF
/// (Server-Side Request Forgery) defense for the Proxy Check feature:
///
/// - **Layer 1 (here, in the domain layer):** if the supplied value parses
///   as a literal `std::net::IpAddr` — i.e. the caller wrote an IP address
///   directly, not a hostname — it is checked against
///   `shared::ssrf::is_private_ip`, and rejected with
///   `DomainError::ProxyHostIsPrivateOrReserved` if it falls in any
///   private, loopback, link-local, multicast, or otherwise reserved
///   range (covering both IPv4 and IPv6, including the IPv4-in-IPv6
///   encodings `is_private_ip` guards against). This layer is pure,
///   synchronous, and does **no I/O** — the domain layer is not allowed to
///   perform I/O — so it can only catch SSRF attempts that are already
///   spelled out as a literal IP.
///
/// - **Layer 2 (NOT implemented here — required at the worker layer):**
///   if the supplied value is a *hostname* (e.g. `"proxy.example.com"`),
///   this constructor deliberately does **not** attempt to resolve it.
///   DNS resolution is I/O and does not belong in the domain layer. A
///   hostname that is perfectly innocuous at validation time can still
///   resolve to a private/internal/cloud-metadata IP address — and,
///   worse, a hostname that resolves to a public IP at validation time
///   can be re-pointed to a private IP later (DNS rebinding), so even a
///   one-time resolve-and-check at submission time is not sufficient on
///   its own.
///
///   Because of this, the worker layer that actually dials a proxy
///   constructed from a hostname-based `ProxyHost` MUST, before every
///   connection attempt:
///   1. Resolve the hostname via DNS.
///   2. Reject the resolved address(es) if any of them are
///      private/loopback/link-local/reserved (the same check
///      `shared::ssrf::is_private_ip` performs).
///   3. Pin the connection to the specific validated IP address it just
///      resolved (rather than resolving again at connect time), so an
///      attacker cannot swap the DNS record between the check and the
///      connection (DNS rebinding).
///
///   `crates/shared/src/ssrf.rs` already implements exactly this pattern
///   (`resolve_and_validate_destination` / `ResolvedDestination`) for
///   monitor checks and webhook notifications — the proxy-check worker
///   should reuse it rather than re-implement it.
///
/// **Do not assume `ProxyHost::new` alone makes it safe to connect to a
/// hostname-based proxy.** It only guarantees the literal-IP case is safe.
/// Anyone extending this type or its call sites should re-read this doc
/// comment before removing or relying-away-from the worker-layer DNS
/// re-check described above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyHost(String);

impl ProxyHost {
    /// Creates a validated proxy host.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let value = value.trim();

        if value.is_empty() {
            return Err(DomainError::EmptyProxyHost);
        }

        // Checked before any parsing so an oversized string never reaches
        // the IP-literal parser below.
        if value.len() > MAX_PROXY_HOST_LENGTH {
            return Err(DomainError::ProxyHostTooLong);
        }

        if !value.chars().all(is_valid_host_char) {
            return Err(DomainError::InvalidProxyHost);
        }

        // If this parses as a literal IP address, it's Layer 1 SSRF
        // territory — reject it outright if it's private/reserved. See
        // the type-level doc comment for why hostnames are NOT resolved
        // (and therefore not checked against `is_private_ip`) here.
        if let Ok(ip) = value.parse::<IpAddr>()
            && shared::ssrf::is_private_ip(ip)
        {
            return Err(DomainError::ProxyHostIsPrivateOrReserved);
        }

        Ok(Self(value.to_string()))
    }

    /// Returns the proxy host as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ProxyHost {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_loopback_ipv4() {
        assert_eq!(
            ProxyHost::new("127.0.0.1"),
            Err(DomainError::ProxyHostIsPrivateOrReserved)
        );
    }

    #[test]
    fn rejects_private_ipv4() {
        assert_eq!(
            ProxyHost::new("10.0.0.1"),
            Err(DomainError::ProxyHostIsPrivateOrReserved)
        );
    }

    #[test]
    fn rejects_link_local_ipv4_metadata_address() {
        assert_eq!(
            ProxyHost::new("169.254.169.254"),
            Err(DomainError::ProxyHostIsPrivateOrReserved)
        );
    }

    #[test]
    fn rejects_loopback_ipv6() {
        assert_eq!(
            ProxyHost::new("::1"),
            Err(DomainError::ProxyHostIsPrivateOrReserved)
        );
    }

    #[test]
    fn accepts_public_ipv4() {
        assert!(ProxyHost::new("8.8.8.8").is_ok());
    }

    #[test]
    fn accepts_hostname() {
        let host = ProxyHost::new("proxy.example.com").unwrap();
        assert_eq!(host.as_str(), "proxy.example.com");
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(ProxyHost::new(""), Err(DomainError::EmptyProxyHost));
        assert_eq!(ProxyHost::new("   "), Err(DomainError::EmptyProxyHost));
    }

    #[test]
    fn rejects_longer_than_253_characters() {
        let long_host = format!("{}.example.com", "a".repeat(250));
        assert!(long_host.len() > 253);

        assert_eq!(
            ProxyHost::new(long_host),
            Err(DomainError::ProxyHostTooLong)
        );
    }

    #[test]
    fn rejects_invalid_characters() {
        assert_eq!(
            ProxyHost::new("proxy.com; rm -rf"),
            Err(DomainError::InvalidProxyHost)
        );
    }
}
