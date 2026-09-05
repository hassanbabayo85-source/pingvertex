use crate::api::client::{get_json, get_json_query, post_json};
use serde::{Deserialize, Serialize};

pub const MAX_PROXIES: usize = 500;
pub const MAX_JSON_BYTES: usize = 1_000_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyInput {
    pub host: String,
    pub port: u16,
    pub scheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateProxyCheckRequest {
    pub proxies: Vec<ProxyInput>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProxyJob {
    pub id: String,
    pub status: String,
    pub total_count: u32,
    pub checked_count: u32,
    pub successful_count: u32,
    pub failed_count: u32,
    pub started_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub completed_at: Option<String>,
    pub failed_at: Option<String>,
    pub canceled_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProxyResult {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub scheme: String,
    pub result: Option<ProxyResultBody>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProxyResultBody {
    pub reachable: bool,
    pub latency_ms: Option<u32>,
    pub supports_https: bool,
    pub country_code: Option<String>,
    pub anonymity: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ProxyResults {
    pub items: Vec<ProxyResult>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

pub async fn create(
    req: &CreateProxyCheckRequest,
) -> Result<ProxyJob, crate::api::client::ApiError> {
    post_json("/proxy-checks", req).await
}

pub async fn get(id: &str) -> Result<ProxyJob, crate::api::client::ApiError> {
    get_json(&format!("/proxy-checks/{id}")).await
}

pub async fn results(
    id: &str,
    limit: u32,
    offset: u32,
    status: Option<&str>,
    scheme: Option<&str>,
) -> Result<ProxyResults, crate::api::client::ApiError> {
    let limit_s = limit.to_string();
    let offset_s = offset.to_string();
    let mut query = vec![("limit", limit_s.as_str()), ("offset", offset_s.as_str())];
    if let Some(value) = status {
        query.push(("status", value));
    }
    if let Some(value) = scheme {
        query.push(("scheme", value));
    }
    get_json_query(&format!("/proxy-checks/{id}/results"), &query).await
}

/// Fetches the complete result set, using the API's 100-row page limit.
/// The server remains authoritative; this helper only handles pagination.
pub async fn all_results(id: &str) -> Result<Vec<ProxyResult>, crate::api::client::ApiError> {
    let mut offset = 0u32;
    let mut all = Vec::new();

    loop {
        let page = results(id, 100, offset, None, None).await?;
        let page_len = page.items.len() as u32;
        all.extend(page.items);

        if all.len() as u32 >= page.total || page_len == 0 {
            break;
        }
        offset = offset.saturating_add(page_len);
    }

    Ok(all)
}

pub async fn cancel(id: &str) -> Result<ProxyJob, crate::api::client::ApiError> {
    post_json(
        &format!("/proxy-checks/{id}/cancel"),
        &serde_json::json!({}),
    )
    .await
}

pub fn parse_proxy_text(input: &str) -> Result<Vec<ProxyInput>, String> {
    let mut entries = Vec::new();

    for (line_idx, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if entries.len() >= MAX_PROXIES {
            return Err("Maximum 500 proxies per job.".into());
        }

        let entry = parse_proxy_line(line)
            .map_err(|message| format!("Line {}: {}", line_idx + 1, message))?;
        entries.push(entry);
    }

    if entries.is_empty() {
        return Err("Enter at least one proxy.".into());
    }

    Ok(entries)
}

fn parse_proxy_line(line: &str) -> Result<ProxyInput, String> {
    let (scheme, rest) = if let Some((raw_scheme, value)) = line.split_once("://") {
        (parse_scheme(raw_scheme)?, value)
    } else {
        ("http".to_string(), line)
    };

    let (userinfo, host_port) = match rest.rsplit_once('@') {
        Some((credentials, host_port)) => (Some(credentials), host_port),
        None => (None, rest),
    };

    let (host, port_str) = if host_port.starts_with('[') {
        let close = host_port
            .find(']')
            .ok_or_else(|| "invalid bracketed IPv6 host".to_string())?;
        let host = &host_port[1..close];
        let port = host_port
            .get(close + 1..)
            .and_then(|value| value.strip_prefix(':'))
            .ok_or_else(|| "expected host:port".to_string())?;
        (host, port)
    } else {
        host_port
            .rsplit_once(':')
            .ok_or_else(|| "expected host:port".to_string())?
    };

    if host.is_empty() {
        return Err("host cannot be empty".into());
    }
    if host.len() > 253
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
    {
        return Err("invalid proxy host".into());
    }

    let port: u16 = port_str
        .parse()
        .map_err(|_| format!("invalid port '{}'", port_str))?;

    // Mirror the backend's proxy-port allowlist for early UX feedback.
    // The backend remains authoritative.
    if !matches!(
        port,
        80 | 443 | 1080 | 3128 | 8080 | 8081 | 8000 | 8443 | 8888 | 9050
    ) {
        return Err(format!("port {} is not an allowed proxy port", port));
    }

    let (username, password) = match userinfo {
        Some(value) => {
            let (username, password) = value
                .split_once(':')
                .ok_or_else(|| "expected username:password before '@'".to_string())?;
            if username.is_empty() || password.is_empty() {
                return Err("proxy credentials cannot be empty".into());
            }
            (Some(username.to_string()), Some(password.to_string()))
        }
        None => (None, None),
    };

    Ok(ProxyInput {
        host: host.trim_matches(['[', ']']).to_string(),
        port,
        scheme,
        username,
        password,
    })
}

fn parse_scheme(raw: &str) -> Result<String, String> {
    match raw.to_ascii_lowercase().as_str() {
        "http" => Ok("http".into()),
        "https" => Ok("https".into()),
        "socks" | "socks5" => Ok("socks5".into()),
        _ => Err(format!("unsupported proxy scheme '{}'", raw)),
    }
}

pub fn parse_json_input(input: &str) -> Result<Vec<ProxyInput>, String> {
    if input.len() > MAX_JSON_BYTES {
        return Err("JSON input exceeds the 1 MB limit.".into());
    }

    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|_| "Invalid JSON.".to_string())?;

    let entries: Vec<ProxyInput> = match value {
        serde_json::Value::Array(value) => serde_json::from_value(serde_json::Value::Array(value))
            .map_err(|_| "JSON proxies are not in the expected format.".to_string())?,
        serde_json::Value::Object(mut object) => {
            let proxies = object
                .remove("proxies")
                .ok_or_else(|| "JSON object must contain a 'proxies' array.".to_string())?;
            serde_json::from_value(proxies)
                .map_err(|_| "JSON proxies are not in the expected format.".to_string())?
        }
        _ => return Err("JSON must be an array or an object containing 'proxies'.".into()),
    };

    if entries.is_empty() {
        return Err("Enter at least one proxy.".into());
    }
    if entries.len() > MAX_PROXIES {
        return Err("Maximum 500 proxies per job.".into());
    }

    for (idx, entry) in entries.iter().enumerate() {
        validate_proxy_input(entry).map_err(|e| format!("Proxy {}: {}", idx + 1, e))?;
    }

    Ok(entries)
}

fn validate_proxy_input(entry: &ProxyInput) -> Result<(), String> {
    if entry.host.is_empty() || entry.host.len() > 253 {
        return Err("invalid proxy host".into());
    }
    if !matches!(
        entry.scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "socks" | "socks5"
    ) {
        return Err("unsupported proxy scheme".into());
    }
    if !matches!(
        entry.port,
        80 | 443 | 1080 | 3128 | 8080 | 8081 | 8000 | 8443 | 8888 | 9050
    ) {
        return Err(format!("port {} is not an allowed proxy port", entry.port));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_schemes_and_credentials_without_exposing_them() {
        let values = parse_proxy_text(
            "http://1.2.3.4:8080\nhttps://user:secret@8.8.8.8:443\nsocks5://9.9.9.9:1080",
        )
        .unwrap();
        assert_eq!(values[0].scheme, "http");
        assert_eq!(values[1].scheme, "https");
        assert_eq!(values[1].username.as_deref(), Some("user"));
        assert_eq!(values[1].password.as_deref(), Some("secret"));
        assert_eq!(values[2].scheme, "socks5");
    }

    #[test]
    fn rejects_malformed_and_unsupported_proxies() {
        assert!(parse_proxy_text("not-a-proxy").is_err());
        assert!(parse_proxy_text("ftp://1.2.3.4:8080").is_err());
        assert!(parse_proxy_text("1.2.3.4:22").is_err());
    }

    #[test]
    fn rejects_more_than_500() {
        let input = (0..501)
            .map(|_| "1.2.3.4:8080")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_proxy_text(&input).is_err());
    }

    #[test]
    fn parses_json_array_and_object() {
        let array = r#"[{"host":"1.2.3.4","port":8080,"scheme":"http"}]"#;
        let object = r#"{"proxies":[{"host":"1.2.3.4","port":8080,"scheme":"Http"}]}"#;
        assert_eq!(parse_json_input(array).unwrap().len(), 1);
        assert_eq!(parse_json_input(object).unwrap().len(), 1);
    }

    #[test]
    fn rejects_json_over_1mb() {
        let huge = "x".repeat(MAX_JSON_BYTES + 1);
        assert!(parse_json_input(&huge).is_err());
    }
}
