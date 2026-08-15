/// Ports recognized as legitimate, well-known proxy ports.
///
/// This is intentionally an ALLOWLIST rather than a denylist: only ports
/// explicitly known to be used by real-world proxy servers are accepted.
/// This keeps the "Proxy Check" feature scoped strictly to known proxy
/// endpoints and prevents it from being repurposed as a general-purpose
/// port scanner against arbitrary hosts/ports (e.g. SSH, RDP, database
/// ports, etc).
pub fn is_allowed_proxy_port(port: u16) -> bool {
    matches!(
        port,
        80 | 443 | 1080 | 3128 | 8080 | 8081 | 8000 | 8443 | 8888 | 9050
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_known_proxy_ports() {
        assert!(is_allowed_proxy_port(80));
        assert!(is_allowed_proxy_port(443));
        assert!(is_allowed_proxy_port(1080));
        assert!(is_allowed_proxy_port(3128));
        assert!(is_allowed_proxy_port(8080));
        assert!(is_allowed_proxy_port(8081));
        assert!(is_allowed_proxy_port(8000));
        assert!(is_allowed_proxy_port(8443));
        assert!(is_allowed_proxy_port(8888));
        assert!(is_allowed_proxy_port(9050));
    }

    #[test]
    fn rejects_non_proxy_ports() {
        assert!(!is_allowed_proxy_port(22)); // SSH
        assert!(!is_allowed_proxy_port(3389)); // RDP
        assert!(!is_allowed_proxy_port(6379)); // Redis
        assert!(!is_allowed_proxy_port(5432)); // Postgres
        assert!(!is_allowed_proxy_port(27017)); // MongoDB
        assert!(!is_allowed_proxy_port(0));
        assert!(!is_allowed_proxy_port(65535));
    }
}
