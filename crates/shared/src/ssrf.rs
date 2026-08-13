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
fn is_private_ipv6(ip: Ipv6Addr) -> bool {
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

    // Idan user ya saka IP kai tsaye, babu DNS lookup da ake bukata.
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
        if is_private_ip(addr.ip()) {
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

/// Backwards-compatible validation helper.
///
/// Wannan yana tabbatar da destination amma ba ya dawo da address.
pub async fn validate_destination(url: &str) -> Result<(), String> {
    resolve_and_validate_destination(url).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn validate_destination_rejects_ipv4_loopback() {
        let url = Url::parse("http://127.0.0.1:8080/status").unwrap();

        assert!(
            validate_destination(url.as_str()).await.is_err(),
            "Expected 127.0.0.1 to be rejected"
        );
    }

    #[tokio::test]
    async fn validate_destination_rejects_ipv6_loopback() {
        let url = Url::parse("http://[::1]:8080/status").unwrap();

        assert!(
            validate_destination(url.as_str()).await.is_err(),
            "Expected ::1 to be rejected"
        );
    }

    #[tokio::test]
    async fn validate_destination_allows_public_ip() {
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
}
