use crate::dto::ProxyEntryInput;
use domain::ProxyScheme;

const MAX_CSV_LINES: usize = 500;
const MAX_JSON_BYTES: usize = 1_000_000; // 1MB

/// Parses a CSV-like text where each non‑empty, non‑comment line represents
/// one proxy. Supported formats per line:
///   host:port
///   host:port:username:password
///   scheme://host:port
///   scheme://username:password@host:port
///
/// The scheme defaults to `http` if not provided. Blank lines and lines
/// starting with `#` are ignored. The total number of valid lines is
/// limited to `MAX_CSV_LINES` (500) and is checked **before** any detailed
/// parsing to prevent resource exhaustion on huge inputs.
pub fn parse_csv(input: &str) -> Result<Vec<ProxyEntryInput>, String> {
    // Early DoS guard: count valid lines first and reject if too many.
    let valid_line_count = input
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count();

    if valid_line_count > MAX_CSV_LINES {
        return Err(format!(
            "CSV contains {} proxy lines; maximum allowed is {}",
            valid_line_count, MAX_CSV_LINES
        ));
    }

    let mut entries = Vec::with_capacity(valid_line_count);

    for (idx, raw_line) in input.lines().enumerate() {
        let line_number = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let entry = parse_csv_line(line).map_err(|e| format!("line {}: {}", line_number, e))?;
        entries.push(entry);
    }

    Ok(entries)
}

/// Parses a single CSV line into a `ProxyEntryInput`.
///
/// Two shapes are supported:
///   - With an explicit scheme prefix: `scheme://host:port` or
///     `scheme://username:password@host:port`.
///   - Without a scheme prefix (defaults to `http`): plain colon-separated
///     `host:port` or `host:port:username:password`. This shape must be
///     handled separately from the scheme-prefixed shape, because a naive
///     "split on the last colon" approach misparses the 4-part form (it
///     would take the password as the port).
fn parse_csv_line(line: &str) -> Result<ProxyEntryInput, String> {
    if let Some(idx) = line.find("://") {
        let scheme_str = &line[..idx];
        let rest = &line[idx + 3..];
        let scheme = parse_scheme(scheme_str)?;

        let (username, password, host_port) = if let Some(at_pos) = rest.rfind('@') {
            let userinfo = &rest[..at_pos];
            let host_port = &rest[at_pos + 1..];
            let (username, password) = match userinfo.split_once(':') {
                Some((u, p)) => (Some(u.to_string()), Some(p.to_string())),
                None => return Err("expected username:password before '@'".to_string()),
            };
            (username, password, host_port)
        } else {
            (None, None, rest)
        };

        let (host, port_str) = parse_host_port(host_port)?;
        let host = host.to_string();
        let port = parse_port(port_str)?;

        Ok(ProxyEntryInput {
            host: host.to_string(),
            port,
            scheme,
            username,
            password,
        })
    } else {
        // No scheme prefix: either host:port or host:port:user:pass.
        // Bracketed IPv6 is accepted as [addr]:port so public IPv6 proxies
        // remain usable while private IPv6 is still rejected by ProxyHost.
        if line.starts_with('[') {
            let close = line
                .find(']')
                .ok_or_else(|| "invalid bracketed IPv6 host".to_string())?;
            let host = &line[1..close];
            let rest = &line[close + 1..];
            let port_str = rest
                .strip_prefix(':')
                .ok_or_else(|| "expected host:port".to_string())?;
            let port = parse_port(port_str)?;
            return Ok(ProxyEntryInput {
                host: host.to_string(),
                port,
                scheme: ProxyScheme::Http,
                username: None,
                password: None,
            });
        }
        let parts: Vec<&str> = line.split(':').collect();
        match parts.as_slice() {
            [host, port_str] => {
                if host.is_empty() {
                    return Err("host cannot be empty".to_string());
                }
                let port = parse_port(port_str)?;
                Ok(ProxyEntryInput {
                    host: host.to_string(),
                    port,
                    scheme: ProxyScheme::Http,
                    username: None,
                    password: None,
                })
            }
            [host, port_str, username, password] => {
                if host.is_empty() {
                    return Err("host cannot be empty".to_string());
                }
                let port = parse_port(port_str)?;
                Ok(ProxyEntryInput {
                    host: host.to_string(),
                    port,
                    scheme: ProxyScheme::Http,
                    username: Some(username.to_string()),
                    password: Some(password.to_string()),
                })
            }
            _ => Err("expected host:port or host:port:username:password".to_string()),
        }
    }
}

fn parse_host_port(value: &str) -> Result<(&str, &str), String> {
    if value.starts_with('[') {
        let close = value
            .find(']')
            .ok_or_else(|| "invalid bracketed IPv6 host".to_string())?;
        let host = &value[1..close];
        let port = value
            .get(close + 1..)
            .and_then(|v| v.strip_prefix(':'))
            .ok_or_else(|| "expected host:port".to_string())?;
        if host.is_empty() {
            return Err("host cannot be empty".to_string());
        }
        Ok((host, port))
    } else {
        let (host, port) = value
            .rsplit_once(':')
            .ok_or_else(|| "expected host:port".to_string())?;
        if host.is_empty() {
            return Err("host cannot be empty".to_string());
        }
        Ok((host, port))
    }
}

/// Parses a port string into a `u16`.
fn parse_port(port_str: &str) -> Result<u16, String> {
    port_str
        .parse()
        .map_err(|_| format!("invalid port '{}'", port_str))
}

/// Converts a scheme string into `domain::ProxyScheme`.
///
/// NOTE: `domain::ProxyScheme` only has `Http`, `Https`, `Socks5` variants
/// (no `Socks4`), so "socks4" is rejected as an invalid scheme here.
fn parse_scheme(s: &str) -> Result<ProxyScheme, String> {
    match s.to_lowercase().as_str() {
        "http" => Ok(ProxyScheme::Http),
        "https" => Ok(ProxyScheme::Https),
        "socks" | "socks5" => Ok(ProxyScheme::Socks5),
        _ => Err(format!("invalid scheme '{}'", s)),
    }
}

/// Parses a JSON array of `ProxyEntryInput` objects.
///
/// A size limit is enforced **before** calling `serde_json::from_str` to
/// avoid expensive parsing of overly large payloads. The array length is
/// also limited to the same 500 entries.
pub fn parse_json(input: &str) -> Result<Vec<ProxyEntryInput>, String> {
    if input.len() > MAX_JSON_BYTES {
        return Err(format!(
            "JSON input size {} bytes exceeds maximum allowed {} bytes",
            input.len(),
            MAX_JSON_BYTES
        ));
    }

    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| format!("invalid JSON: {}", e))?;
    let entries: Vec<ProxyEntryInput> = match value {
        serde_json::Value::Array(_) => {
            serde_json::from_value(value).map_err(|e| format!("invalid JSON proxies: {}", e))?
        }
        serde_json::Value::Object(mut object) => {
            let proxies = object
                .remove("proxies")
                .ok_or_else(|| "JSON body must contain a 'proxies' array".to_string())?;
            serde_json::from_value(proxies).map_err(|e| format!("invalid JSON proxies: {}", e))?
        }
        _ => {
            return Err("JSON body must be an array or an object containing 'proxies'".to_string());
        }
    };

    if entries.len() > MAX_CSV_LINES {
        return Err(format!(
            "JSON array contains {} proxies; maximum allowed is {}",
            entries.len(),
            MAX_CSV_LINES
        ));
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_all_formats_and_defaults() {
        let input = "\
1.2.3.4:8080
1.2.3.4:8081:user:pass
http://1.2.3.4:8082
https://user:pass@1.2.3.4:8083
";
        let entries = parse_csv(input).unwrap();
        assert_eq!(entries.len(), 4);

        // First: default scheme, no credentials
        assert_eq!(entries[0].host, "1.2.3.4");
        assert_eq!(entries[0].port, 8080);
        assert!(entries[0].username.is_none());
        assert!(entries[0].password.is_none());

        // Second: default scheme, with credentials
        assert_eq!(entries[1].host, "1.2.3.4");
        assert_eq!(entries[1].port, 8081);
        assert_eq!(entries[1].username.as_deref(), Some("user"));
        assert_eq!(entries[1].password.as_deref(), Some("pass"));

        // Third: scheme provided, no credentials
        assert_eq!(entries[2].host, "1.2.3.4");
        assert_eq!(entries[2].port, 8082);

        // Fourth: scheme and credentials
        assert_eq!(entries[3].host, "1.2.3.4");
        assert_eq!(entries[3].port, 8083);
        assert_eq!(entries[3].username.as_deref(), Some("user"));
        assert_eq!(entries[3].password.as_deref(), Some("pass"));
    }

    #[test]
    fn csv_skips_comments_and_blank_lines() {
        let input = "\
# comment line
\n
1.2.3.4:8080

# another comment
5.6.7.8:9090
";
        let entries = parse_csv(input).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].host, "1.2.3.4");
        assert_eq!(entries[1].host, "5.6.7.8");
    }

    #[test]
    fn csv_rejects_more_than_500_lines() {
        let line = "1.2.3.4:8080\n";
        let input = line.repeat(501);
        let err = parse_csv(&input).unwrap_err();
        assert!(err.contains("maximum allowed is 500"));
    }

    #[test]
    fn csv_rejects_invalid_port() {
        let input = "1.2.3.4:abc\n";
        let err = parse_csv(input).unwrap_err();
        assert!(err.contains("invalid port 'abc'"));
    }

    #[test]
    fn json_valid_array() {
        let input = r#"[{"host":"1.2.3.4","port":8080,"scheme":"Http"}]"#;
        let entries = parse_json(input).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].host, "1.2.3.4");
        assert_eq!(entries[0].port, 8080);
    }

    #[test]
    fn json_rejects_large_payload() {
        let huge = "a".repeat(1_000_001);
        let err = parse_json(&huge).unwrap_err();
        assert!(err.contains("exceeds maximum allowed"));
    }
    #[test]
    fn csv_supports_ipv6_and_all_protocols() {
        let input = "[2001:4860:4860::8888]:8080
https://1.2.3.4:8443
socks5://1.2.3.4:1080
socks://user:pass@1.2.3.4:1081
";
        let entries = parse_csv(input).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].host, "2001:4860:4860::8888");
        assert_eq!(entries[0].scheme, ProxyScheme::Http);
        assert_eq!(entries[1].scheme, ProxyScheme::Https);
        assert_eq!(entries[2].scheme, ProxyScheme::Socks5);
        assert_eq!(entries[3].username.as_deref(), Some("user"));
    }

    #[test]
    fn parser_rejects_invalid_scheme_and_malformed_ipv6() {
        assert!(parse_csv("ftp://1.2.3.4:80").is_err());
        assert!(parse_csv("[2001:db8::1:8080").is_err());
    }

    #[test]
    fn json_rejects_more_than_500_entries() {
        let input = serde_json::to_string(
            &(0..501)
                .map(|_| serde_json::json!({"host":"1.2.3.4","port":8080,"scheme":"Http"}))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let err = parse_json(&input).unwrap_err();
        assert!(err.contains("maximum allowed is 500"));
    }
}
