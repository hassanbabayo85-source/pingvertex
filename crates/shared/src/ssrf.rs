//! SSRF (Server-Side Request Forgery) protection shared by every crate that
//! sends an HTTP request to a user-supplied URL.
//!
//! This module used to live only in `crates/worker/src/ssrf.rs`, where it
//! protects monitor checks. It was moved here (2026 notification feature
//! work) so `crates/infra`'s webhook notification service can reuse the
//! EXACT same validation — not a re-implementation that could quietly drift
//! out of sync — instead of duplicating private-IP/DNS-rebinding logic.
//! `crates/worker/src/ssrf.rs` now just re-exports this module, so existing
//! call sites (`crate::ssrf::...`) and tests are unaffected.
//!
//! Kept in `shared` (rather than `domain`) because DNS resolution is I/O,
//! which does not belong in the I/O-free domain layer.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use url::Url;

// -------- IP da Ayyukan Tacewa (Strictly RFC 6890 / 1918) --------

/// Duba idan adireshin IPv4 na cikin jerin sassa masu zaman kansu ko na musamman.
fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();

    // 0.0.0.0/8 ("this network" / unspecified). Also closes a subtle gap
    // introduced by unwrapping IPv4-mapped IPv6 addresses below: `::`
    // and `::1` both unwrap to 0.0.0.0 / 0.0.0.1 via `Ipv6Addr::to_ipv4()`,
    // so this range needs to be blocked here too, not just via
    // `Ipv6Addr::is_unspecified()`/`is_loopback()` on the un-unwrapped form.
    if octets[0] == 0 {
        return true;
    }

    // 10.0.0.0/8
    if octets[0] == 10 {
        return true;
    }

    // 100.64.0.0/10 (Carrier-grade NAT)
    if octets[0] == 100 && (64..=127).contains(&octets[1]) {
        return true;
    }

    // 127.0.0.0/8 (Loopback)
    if octets[0] == 127 {
        return true;
    }

    // 169.254.0.0/16 (Link-local)
    if octets[0] == 169 && octets[1] == 254 {
        return true;
    }

    // 172.16.0.0/12 (Private)
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return true;
    }

    // 192.0.0.0/24 (IETF Protocol Assignments)
    if octets[0] == 192 && octets[1] == 0 && octets[2] == 0 {
        return true;
    }

    // 192.0.2.0/24 (Documentation/TEST-NET)
    if octets[0] == 192 && octets[1] == 0 && octets[2] == 2 {
        return true;
    }

    // 192.168.0.0/16 (Private)
    if octets[0] == 192 && octets[1] == 168 {
        return true;
    }

    // 198.18.0.0/15 (Benchmarking)
    if octets[0] == 198 && (18..=19).contains(&octets[1]) {
        return true;
    }

    // 198.51.100.0/24 (Documentation/TEST-NET-2)
    if octets[0] == 198 && octets[1] == 51 && octets[2] == 100 {
        return true;
    }

    // 203.0.113.0/24 (Documentation/TEST-NET-3)
    if octets[0] == 203 && octets[1] == 0 && octets[2] == 113 {
        return true;
    }

    // 224.0.0.0/4 (Multicast)
    if (224..=239).contains(&octets[0]) {
        return true;
    }

    // 240.0.0.0/4 (Reserved)
    if octets[0] >= 240 {
        return true;
    }

    false
}

/// Duba idan adireshin IPv6 na cikin jerin sassa masu zaman kansu ko na musamman.
///
/// SECURITY: this also unwraps IPv6 encodings that carry an embedded IPv4
/// address — `::ffff:a.b.c.d` (IPv4-mapped), the deprecated `::a.b.c.d`
/// (IPv4-compatible), `2002::/16` (6to4), and `2001::/32` (Teredo) — and
/// validates THAT address against `is_private_ipv4` too.
///
/// Without this, an attacker could bypass every check below simply by
/// writing a private/internal IPv4 target in one of these IPv6 forms —
/// e.g. `http://[::ffff:169.254.169.254]/` resolves as `IpAddr::V6` and
/// would pass every check that only looks at the "native" IPv6 ranges
/// (loopback/ULA/link-local/multicast), while the OS network stack
/// dials the wrapped IPv4 address on the wire. This is a well-known,
/// actively-exploited SSRF bypass technique — it isn't hypothetical.
fn is_private_ipv6(ip: Ipv6Addr) -> bool {
    // ::ffff:a.b.c.d (IPv4-mapped) and the deprecated ::a.b.c.d
    // (IPv4-compatible) both carry a real IPv4 address in the low 32
    // bits. `to_ipv4()` recognizes both forms.
    if let Some(mapped) = ip.to_ipv4() {
        return is_private_ipv4(mapped);
    }

    // 2002::/16 (6to4): the embedded IPv4 address is segments[1..3].
    let segments = ip.segments();
    if segments[0] == 0x2002 {
        let embedded = Ipv4Addr::new(
            (segments[1] >> 8) as u8,
            (segments[1] & 0xff) as u8,
            (segments[2] >> 8) as u8,
            (segments[2] & 0xff) as u8,
        );
        return is_private_ipv4(embedded);
    }

    // 2001::/32 (Teredo): the client's IPv4 address is the last 32 bits,
    // obfuscated by XOR with 0xffffffff.
    if segments[0] == 0x2001 && segments[1] == 0x0000 {
        let obfuscated = ((segments[6] as u32) << 16) | segments[7] as u32;
        let embedded = Ipv4Addr::from(obfuscated ^ 0xffff_ffff);
        return is_private_ipv4(embedded);
    }

    // ::1 (Loopback)
    if ip.is_loopback() {
        return true;
    }

    // :: (Unspecified)
    if ip.is_unspecified() {
        return true;
    }

    // fc00::/7 (Unique Local Addresses)
    if ip.segments()[0] & 0xfe00 == 0xfc00 {
        return true;
    }

    // fe80::/10 (Link-local)
    if ip.segments()[0] & 0xffc0 == 0xfe80 {
        return true;
    }

    // ff00::/8 (Multicast)
    if ip.segments()[0] & 0xff00 == 0xff00 {
        return true;
    }

    false
}

/// Babban aikin da ke duba ko wani `IpAddr` na cikin jerin haramtattun sassa.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => is_private_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_private_ipv6(ipv6),
    }
}

/// Destination da aka resolve kuma aka tabbatar da cewa public ce.
///
/// `socket_addr` shi ne IP + port da aka yi validation a lokacin DNS lookup.
/// Checker zai yi amfani da wannan address wajen pinning DNS, don haka
/// request ɗin ba zai sake yin wani DNS lookup daban ba.
#[derive(Debug, Clone)]
pub struct ResolvedDestination {
    pub url: Url,
    pub host: String,
    pub socket_addr: SocketAddr,
}

/// Resolve URL sau ɗaya, tabbatar da duk IPs, sannan dawo da IP ɗin da
/// request ɗin zai yi amfani da shi.
pub async fn resolve_and_validate_destination(url: &str) -> Result<ResolvedDestination, String> {
    let url = Url::parse(url).map_err(|err| format!("invalid monitor URL: {err}"))?;

    let host = url
        .host_str()
        .ok_or_else(|| "monitor URL has no host".to_string())?
        .to_string();

    let port = url
        .port_or_known_default()
        .ok_or_else(|| "monitor URL has no valid port".to_string())?;

    let allow_private = std::env::var("PINGVERTEX_SSRF_ALLOW_PRIVATE").as_deref() == Ok("true");

    // Idan user ya saka IP kai tsaye, babu DNS lookup da ake bukata.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !allow_private && is_private_ip(ip) {
            return Err(format!(
                "SSRF blocked: private/internal IP address supplied directly: {ip}"
            ));
        }

        return Ok(ResolvedDestination {
            url,
            host,
            socket_addr: SocketAddr::new(ip, port),
        });
    }

    // IMPORTANT:
    // Wannan shi ne DNS lookup guda daya da muke dogaro da shi.
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|err| format!("failed to resolve monitor host '{host}': {err}"))?
        .collect();

    if addrs.is_empty() {
        return Err(format!(
            "monitor host '{host}' did not resolve to any IP addresses"
        ));
    }

    // Kada mu bar wani internal address ya shiga cikin resolved set.
    for addr in &addrs {
        if !allow_private && is_private_ip(addr.ip()) {
            return Err(format!(
                "SSRF blocked: host '{host}' resolves to private/internal address {}",
                addr.ip()
            ));
        }
    }

    // Mun tabbatar da address ɗin, don haka wannan shi ne address ɗin
    // da za mu pin a reqwest client.
    let socket_addr = addrs[0];

    Ok(ResolvedDestination {
        url,
        host,
        socket_addr,
    })
}

/// Strict variant for proxy checks. Unlike monitor/webhook validation this
/// function NEVER honors `PINGVERTEX_SSRF_ALLOW_PRIVATE`; proxy validation is
/// always public-internet-only because a proxy checker is itself an outbound
/// network primitive that must not become an SSRF bridge.
pub async fn resolve_and_validate_public_destination(
    url: &str,
) -> Result<ResolvedDestination, String> {
    let url = Url::parse(url).map_err(|err| format!("invalid proxy URL: {err}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| "proxy URL has no host".to_string())?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "proxy URL has no valid port".to_string())?;

    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost") {
        return Err("SSRF blocked: localhost is not an allowed proxy destination".to_string());
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(format!(
                "SSRF blocked: private/internal IP address supplied directly: {ip}"
            ));
        }
        return Ok(ResolvedDestination {
            url,
            host,
            socket_addr: SocketAddr::new(ip, port),
        });
    }

    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|err| format!("failed to resolve proxy host '{host}': {err}"))?
        .collect();
    if addrs.is_empty() {
        return Err(format!(
            "proxy host '{host}' did not resolve to any IP addresses"
        ));
    }
    for addr in &addrs {
        if is_private_ip(addr.ip()) {
            return Err(format!(
                "SSRF blocked: proxy host '{host}' resolves to private/internal address {}",
                addr.ip()
            ));
        }
    }
    Ok(ResolvedDestination {
        url,
        host,
        socket_addr: addrs[0],
    })
}

/// Backwards-compatible validation helper.
///
/// Wannan yana tabbatar da destination amma ba ya dawo da address.
pub async fn validate_destination(url: &str) -> Result<(), String> {
    resolve_and_validate_destination(url).await.map(|_| ())
}

/// Resolve a raw `host`/`port` pair (not a full URL) and validate that every
/// resolved address is public, returning the first pinned `SocketAddr`.
///
/// This is the TCP-checker equivalent of `resolve_and_validate_destination`
/// for endpoints that are not expressed as a URL (e.g. `host:port` monitor
/// targets). It follows the EXACT same DNS-rebinding-safe pattern: resolve
/// once, validate every resolved address, then pin the connection to the
/// validated address rather than resolving again at connect time.
pub async fn resolve_and_validate_host_port(host: &str, port: u16) -> Result<SocketAddr, String> {
    let allow_private = std::env::var("PINGVERTEX_SSRF_ALLOW_PRIVATE").as_deref() == Ok("true");
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !allow_private && is_private_ip(ip) {
            return Err(format!(
                "SSRF blocked: private/internal IP address supplied directly: {ip}"
            ));
        }

        return Ok(SocketAddr::new(ip, port));
    }

    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|err| format!("failed to resolve host '{host}': {err}"))?
        .collect();

    if addrs.is_empty() {
        return Err(format!("host '{host}' did not resolve to any IP addresses"));
    }

    for addr in &addrs {
        if !allow_private && is_private_ip(addr.ip()) {
            return Err(format!(
                "SSRF blocked: host '{host}' resolves to private/internal address {}",
                addr.ip()
            ));
        }
    }

    Ok(addrs[0])
}

/// Error returned by `validate_production_ssrf_config` when boot should be
/// refused. Implements `std::error::Error` so it composes with both
/// `anyhow::Result` (agent) and `Box<dyn Error>` (api/worker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsrfConfigError {
    message: String,
}

impl std::fmt::Display for SsrfConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SsrfConfigError {}

/// P0 hardening: refuse to boot with `PINGVERTEX_SSRF_ALLOW_PRIVATE=true`
/// unless `ENVIRONMENT` is explicitly set to a recognized non-production
/// value.
///
/// RATIONALE: allowing private/internal IPs gives any authenticated user
/// with monitor/webhook creation rights a direct SSRF bridge to cloud
/// metadata endpoints and internal services, so this must be a boot-time
/// failure, not a warning.
///
/// FAIL-CLOSED, not fail-open: earlier this only refused to boot when
/// `ENVIRONMENT == "production"` exactly, which meant an unset
/// `ENVIRONMENT` (the default in every `docker-compose*.yml` in this
/// repo — none of them set it) or any typo/variant ("prod", "Production",
/// "staging") silently skipped the check entirely and let the server boot
/// with private-IP SSRF enabled. This inverts the check to an explicit
/// allowlist of known-safe non-production values instead: anything NOT
/// on that list is treated as production for this guard's purposes,
/// including an unset `ENVIRONMENT`.
///
/// Error message deliberately does NOT include any secret values.
pub fn validate_production_ssrf_config() -> Result<(), SsrfConfigError> {
    const SAFE_NON_PRODUCTION_ENVIRONMENTS: &[&str] = &["development", "dev", "test", "local"];

    let allow_private = std::env::var("PINGVERTEX_SSRF_ALLOW_PRIVATE").as_deref() == Ok("true");
    if !allow_private {
        return Ok(());
    }

    let environment = std::env::var("ENVIRONMENT").unwrap_or_default();
    let is_safe = SAFE_NON_PRODUCTION_ENVIRONMENTS
        .iter()
        .any(|safe| safe.eq_ignore_ascii_case(environment.trim()));

    if !is_safe {
        return Err(SsrfConfigError {
            message: format!(
                "refusing to boot: PINGVERTEX_SSRF_ALLOW_PRIVATE=true is only allowed when \
                 ENVIRONMENT is explicitly one of {SAFE_NON_PRODUCTION_ENVIRONMENTS:?} \
                 (got {environment:?}); this enables SSRF access to private/internal IPs and \
                 must never run in production"
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env_test_lock::ENV_MUTEX;

    #[test]
    fn blocks_private_ipv4_ranges() {
        let private_ips = [
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
        ];

        for ip_str in private_ips {
            let ip: IpAddr = ip_str.parse().unwrap();
            assert!(is_private_ip(ip), "Expected private IP {ip} to be blocked");
        }
    }

    #[test]
    fn blocks_zero_network_ipv4() {
        let ip: IpAddr = "0.0.0.0".parse().unwrap();
        assert!(is_private_ip(ip));
        let ip: IpAddr = "0.0.0.1".parse().unwrap();
        assert!(is_private_ip(ip));
    }

    #[test]
    fn blocks_ipv6_internal_addresses() {
        let private_ips = ["::1", "::", "fc00::1", "fe80::1", "ff02::1"];

        for ip_str in private_ips {
            let ip: IpAddr = ip_str.parse().unwrap();
            assert!(
                is_private_ip(ip),
                "Expected private IPv6 {ip} to be blocked"
            );
        }
    }

    #[test]
    fn blocks_ipv6_encodings_that_embed_a_private_ipv4_address() {
        let bypass_attempts = [
            "::ffff:169.254.169.254",
            "::ffff:a9fe:a9fe",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:192.168.1.1",
            "::169.254.169.254",
            "2002:a9fe:a9fe::",
            "2001:0000:0000:0000:0000:0000:5601:5601",
        ];

        for ip_str in bypass_attempts {
            let ip: IpAddr = ip_str.parse().unwrap();
            assert!(
                is_private_ip(ip),
                "Expected IPv6 bypass attempt {ip} (embedding a private IPv4 address) to be blocked"
            );
        }
    }

    #[test]
    fn allows_public_ipv4() {
        let public_ips = ["1.1.1.1", "8.8.8.8", "93.184.216.34"];

        for ip_str in public_ips {
            let ip: IpAddr = ip_str.parse().unwrap();
            assert!(!is_private_ip(ip), "Expected public IP {ip} to be allowed");
        }
    }

    #[test]
    fn allows_public_ipv6() {
        let public_ips = ["2001:4860:4860::8888", "2606:2800:220:1:248:1893:25c8:1946"];

        for ip_str in public_ips {
            let ip: IpAddr = ip_str.parse().unwrap();
            assert!(
                !is_private_ip(ip),
                "Expected public IPv6 {ip} to be allowed"
            );
        }
    }

    #[tokio::test]
    async fn strict_proxy_destination_rejects_localhost() {
        let result = resolve_and_validate_public_destination("http://localhost:8080").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn validate_destination_rejects_ipv4_loopback() {
        let _guard = ENV_MUTEX.lock().await;
        unsafe {
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }

        let url = Url::parse("http://127.0.0.1:8080/status").unwrap();

        assert!(
            validate_destination(url.as_str()).await.is_err(),
            "Expected 127.0.0.1 to be rejected"
        );
    }

    #[tokio::test]
    async fn validate_destination_rejects_ipv6_loopback() {
        let _guard = ENV_MUTEX.lock().await;
        unsafe {
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }

        let url = Url::parse("http://[::1]:8080/status").unwrap();

        assert!(
            validate_destination(url.as_str()).await.is_err(),
            "Expected ::1 to be rejected"
        );
    }

    #[tokio::test]
    async fn validate_destination_allows_public_ip() {
        let _guard = ENV_MUTEX.lock().await;
        unsafe {
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }

        let url = Url::parse("http://1.1.1.1").unwrap();

        assert!(
            validate_destination(url.as_str()).await.is_ok(),
            "Expected 1.1.1.1 to be allowed"
        );
    }

    #[tokio::test]
    async fn resolve_destination_returns_pinned_socket_address_for_public_ip() {
        let url = "http://1.1.1.1:80/status";

        let destination = resolve_and_validate_destination(url)
            .await
            .expect("public IP should be allowed");

        assert_eq!(destination.host, "1.1.1.1");
        assert_eq!(
            destination.socket_addr,
            "1.1.1.1:80".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            destination.url,
            Url::parse("http://1.1.1.1/status").unwrap()
        );
    }

    // ---- P0 hardening: production SSRF guard tests ----

    #[test]
    fn production_private_true_fails() {
        let _guard = ENV_MUTEX.blocking_lock();
        unsafe {
            std::env::set_var("ENVIRONMENT", "production");
            std::env::set_var("PINGVERTEX_SSRF_ALLOW_PRIVATE", "true");
        }

        assert!(validate_production_ssrf_config().is_err());

        unsafe {
            std::env::remove_var("ENVIRONMENT");
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }
    }

    #[test]
    fn production_private_false_allows() {
        let _guard = ENV_MUTEX.blocking_lock();
        unsafe {
            std::env::set_var("ENVIRONMENT", "production");
            std::env::set_var("PINGVERTEX_SSRF_ALLOW_PRIVATE", "false");
        }

        assert!(validate_production_ssrf_config().is_ok());

        unsafe {
            std::env::remove_var("ENVIRONMENT");
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }
    }

    #[test]
    fn development_private_true_allows() {
        let _guard = ENV_MUTEX.blocking_lock();
        unsafe {
            std::env::set_var("ENVIRONMENT", "development");
            std::env::set_var("PINGVERTEX_SSRF_ALLOW_PRIVATE", "true");
        }

        assert!(validate_production_ssrf_config().is_ok());

        unsafe {
            std::env::remove_var("ENVIRONMENT");
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }
    }

    #[test]
    fn unset_environment_with_private_true_fails() {
        // Regression test for the fail-open bug: an unset `ENVIRONMENT`
        // (the default in every docker-compose*.yml here) must NOT
        // silently allow private-IP SSRF. This is the scenario that
        // slipped through the old `environment == "production"` check.
        let _guard = ENV_MUTEX.blocking_lock();
        unsafe {
            std::env::remove_var("ENVIRONMENT");
            std::env::set_var("PINGVERTEX_SSRF_ALLOW_PRIVATE", "true");
        }

        assert!(validate_production_ssrf_config().is_err());

        unsafe {
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }
    }

    #[test]
    fn unrecognized_environment_value_with_private_true_fails() {
        // Regression test: a typo'd or non-listed ENVIRONMENT value
        // (e.g. "prod", "staging", "Production") must fail closed too,
        // not just the exact unset/"production" cases.
        let _guard = ENV_MUTEX.blocking_lock();
        for bad_value in ["prod", "staging", "Production", "PRODUCTION "] {
            unsafe {
                std::env::set_var("ENVIRONMENT", bad_value);
                std::env::set_var("PINGVERTEX_SSRF_ALLOW_PRIVATE", "true");
            }

            assert!(
                validate_production_ssrf_config().is_err(),
                "expected ENVIRONMENT={bad_value:?} with ALLOW_PRIVATE=true to fail"
            );
        }

        unsafe {
            std::env::remove_var("ENVIRONMENT");
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }
    }

    #[test]
    fn every_safe_environment_value_with_private_true_allows() {
        let _guard = ENV_MUTEX.blocking_lock();
        for good_value in ["development", "dev", "test", "local", "DEV", "Test"] {
            unsafe {
                std::env::set_var("ENVIRONMENT", good_value);
                std::env::set_var("PINGVERTEX_SSRF_ALLOW_PRIVATE", "true");
            }

            assert!(
                validate_production_ssrf_config().is_ok(),
                "expected ENVIRONMENT={good_value:?} with ALLOW_PRIVATE=true to succeed"
            );
        }

        unsafe {
            std::env::remove_var("ENVIRONMENT");
            std::env::remove_var("PINGVERTEX_SSRF_ALLOW_PRIVATE");
        }
    }
    #[test]
    fn private_ip_matrix_covers_common_ssrf_ranges() {
        let blocked = [
            "0.0.0.0",
            "10.1.2.3",
            "100.64.1.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "192.168.1.1",
            "192.0.2.1",
            "198.18.0.1",
            "203.0.113.10",
            "224.0.0.1",
            "240.0.0.1",
            "::",
            "::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
        ];
        for raw in blocked {
            let ip: IpAddr = raw.parse().expect(raw);
            assert!(is_private_ip(ip), "expected SSRF block for {raw}");
        }
    }

    #[test]
    fn public_ip_matrix_has_known_safe_addresses() {
        for raw in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "2606:4700:4700::1111",
        ] {
            let ip: IpAddr = raw.parse().expect(raw);
            assert!(!is_private_ip(ip), "unexpected SSRF block for {raw}");
        }
    }

    #[tokio::test]
    async fn public_proxy_destination_rejects_localhost_and_private_literals() {
        for url in [
            "http://localhost:8080",
            "http://127.0.0.1:8080",
            "http://[::1]:8080",
            "http://169.254.169.254:80",
            "http://[::ffff:169.254.169.254]:80",
        ] {
            assert!(
                resolve_and_validate_public_destination(url).await.is_err(),
                "expected proxy destination rejection for {url}"
            );
        }
    }
}
