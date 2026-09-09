use crate::dto::{BulkMonitorImportError, BulkMonitorImportRow};
use domain::{HttpMethod, MonitorKind, RegionCode};

const MAX_ROWS: usize = 10_000;
const MAX_FIELD_LEN: usize = 2_048;

const LEGACY_HEADER: [&str; 15] = [
    "name",
    "url",
    "kind",
    "interval_seconds",
    "timeout_seconds",
    "method",
    "expected_status",
    "tcp_port",
    "dns_record_type",
    "dns_expected_value",
    "heartbeat_interval_seconds",
    "heartbeat_grace_period_seconds",
    "is_public",
    "ssl_check_enabled",
    "ssl_expiry_warning_days",
];

const HEADER: [&str; 17] = [
    "name",
    "url",
    "kind",
    "interval_seconds",
    "timeout_seconds",
    "method",
    "expected_status",
    "tcp_port",
    "dns_record_type",
    "dns_expected_value",
    "heartbeat_interval_seconds",
    "heartbeat_grace_period_seconds",
    "is_public",
    "ssl_check_enabled",
    "ssl_expiry_warning_days",
    "regions",
    "auth_type",
];

pub fn parse_csv(
    input: &str,
) -> Result<(Vec<BulkMonitorImportRow>, Vec<BulkMonitorImportError>), String> {
    let mut lines = input.lines();
    let header_line = lines.next().ok_or_else(|| "CSV is empty".to_owned())?;
    let header = split_csv_line(header_line)?;

    let legacy = header
        .iter()
        .map(String::as_str)
        .eq(LEGACY_HEADER.iter().copied());
    let modern = header.iter().map(String::as_str).eq(HEADER.iter().copied());
    if !legacy && !modern {
        return Err(format!(
            "invalid CSV header; expected: {}",
            HEADER.join(",")
        ));
    }

    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut data_rows = 0usize;

    for (index, line) in lines.enumerate() {
        let row_number = index + 2;
        if line.trim().is_empty() {
            continue;
        }

        data_rows += 1;
        if data_rows > MAX_ROWS {
            return Err(format!("CSV contains more than {MAX_ROWS} data rows"));
        }

        let mut fields = match split_csv_line(line) {
            Ok(fields) => fields,
            Err(error) => {
                errors.push(BulkMonitorImportError {
                    row: row_number,
                    errors: vec![error],
                });
                continue;
            }
        };

        if legacy {
            fields.push(String::new());
            fields.push("none".to_owned());
        }

        if fields.len() != HEADER.len() {
            errors.push(BulkMonitorImportError {
                row: row_number,
                errors: vec![format!(
                    "expected {} columns, got {}",
                    HEADER.len(),
                    fields.len()
                )],
            });
            continue;
        }

        if fields.iter().any(|field| field.len() > MAX_FIELD_LEN) {
            errors.push(BulkMonitorImportError {
                row: row_number,
                errors: vec![format!("a field exceeds {MAX_FIELD_LEN} bytes")],
            });
            continue;
        }

        match parse_row(&fields) {
            Ok(mut row) => {
                row.source_row = row_number;
                rows.push(row);
            }
            Err(row_errors) => errors.push(BulkMonitorImportError {
                row: row_number,
                errors: row_errors,
            }),
        }
    }

    Ok((rows, errors))
}

fn parse_row(fields: &[String]) -> Result<BulkMonitorImportRow, Vec<String>> {
    let mut errors = Vec::new();

    let name = fields[0].trim().to_owned();
    let url = fields[1].trim().to_owned();
    let kind = parse_kind(&fields[2], &mut errors);
    let method = parse_method(&fields[5], &mut errors);

    if name.is_empty() {
        errors.push("name is required".to_owned());
    }
    if url.is_empty() {
        errors.push("url is required".to_owned());
    }

    let interval_seconds = parse_required(&fields[3], "interval_seconds", &mut errors);
    let timeout_seconds = parse_required(&fields[4], "timeout_seconds", &mut errors);
    let expected_status = parse_or(&fields[6], 200, "expected_status", &mut errors);
    let tcp_port = parse_optional(&fields[7], "tcp_port", &mut errors);
    let dns_record_type = optional_string(&fields[8]);
    let dns_expected_value = optional_string(&fields[9]);
    let heartbeat_interval_seconds =
        parse_optional(&fields[10], "heartbeat_interval_seconds", &mut errors);
    let heartbeat_grace_period_seconds =
        parse_optional(&fields[11], "heartbeat_grace_period_seconds", &mut errors);
    let is_public = parse_bool(&fields[12], "is_public", false, &mut errors);
    let ssl_check_enabled = parse_bool(&fields[13], "ssl_check_enabled", true, &mut errors);
    let ssl_expiry_warning_days = parse_or(&fields[14], 14, "ssl_expiry_warning_days", &mut errors);
    let regions = parse_regions(&fields[15], &mut errors);
    let auth_type = if fields[16].trim().is_empty() {
        "none".to_owned()
    } else {
        fields[16].trim().to_ascii_lowercase()
    };
    if auth_type != "none" {
        errors.push(
            "authentication configuration is unsupported in bulk import; use auth_type=none"
                .to_owned(),
        );
    }

    if interval_seconds == 0 {
        errors.push("interval_seconds must be greater than zero".to_owned());
    }
    if timeout_seconds == 0 {
        errors.push("timeout_seconds must be greater than zero".to_owned());
    }
    if expected_status == 0 {
        errors.push("expected_status must be greater than zero".to_owned());
    }

    match kind {
        Some(MonitorKind::Tcp) if tcp_port.is_none() => {
            errors.push("tcp_port is required for Tcp monitors".to_owned());
        }
        Some(MonitorKind::Dns) if dns_record_type.is_none() => {
            errors.push("dns_record_type is required for Dns monitors".to_owned());
        }
        Some(MonitorKind::Heartbeat) if heartbeat_interval_seconds.is_none() => {
            errors.push("heartbeat_interval_seconds is required for Heartbeat monitors".to_owned());
        }
        _ => {}
    }

    match (kind, method) {
        (Some(MonitorKind::Http), None) => {}
        (Some(MonitorKind::Flow), None) => {}
        (Some(MonitorKind::Tcp), None) => {}
        (Some(MonitorKind::Dns), None) => {}
        (Some(MonitorKind::Heartbeat), None) => {}
        _ => {}
    }

    if errors.is_empty() {
        Ok(BulkMonitorImportRow {
            source_row: 0,
            name,
            url,
            kind: kind.expect("kind validated"),
            interval_seconds,
            timeout_seconds,
            method: method.expect("method validated"),
            expected_status,
            tcp_port,
            dns_record_type,
            dns_expected_value,
            heartbeat_interval_seconds,
            heartbeat_grace_period_seconds,
            is_public,
            ssl_check_enabled,
            ssl_expiry_warning_days,
            regions,
            auth_type,
        })
    } else {
        Err(errors)
    }
}

fn parse_regions(value: &str, errors: &mut Vec<String>) -> Vec<RegionCode> {
    if value.trim().is_empty() {
        return vec![RegionCode::default_region()];
    }
    let mut regions = Vec::new();
    for raw in value.split(';').map(str::trim).filter(|v| !v.is_empty()) {
        match RegionCode::new(raw) {
            Ok(region) if !regions.contains(&region) => regions.push(region),
            Ok(_) => {}
            Err(_) => errors.push(format!("unsupported region: {raw}")),
        }
    }
    if regions.is_empty() {
        errors.push("at least one region is required".to_owned());
    }
    regions
}

fn parse_kind(value: &str, errors: &mut Vec<String>) -> Option<MonitorKind> {
    match serde_json::from_str::<MonitorKind>(&format!("\"{}\"", value.trim())) {
        Ok(kind) => Some(kind),
        Err(_) => {
            errors.push(format!("invalid monitor kind: {}", value.trim()));
            None
        }
    }
}

fn parse_method(value: &str, errors: &mut Vec<String>) -> Option<HttpMethod> {
    match value.trim().to_ascii_uppercase().as_str() {
        "GET" => Some(HttpMethod::Get),
        "HEAD" => Some(HttpMethod::Head),
        other => {
            errors.push(format!("invalid HTTP method: {other}"));
            None
        }
    }
}

fn optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_required<T>(value: &str, name: &str, errors: &mut Vec<String>) -> T
where
    T: std::str::FromStr + Default,
{
    match value.trim().parse::<T>() {
        Ok(value) => value,
        Err(_) => {
            errors.push(format!("{name} is invalid"));
            T::default()
        }
    }
}

fn parse_or<T>(value: &str, default: T, name: &str, errors: &mut Vec<String>) -> T
where
    T: std::str::FromStr + Copy,
{
    if value.trim().is_empty() {
        default
    } else {
        match value.trim().parse::<T>() {
            Ok(value) => value,
            Err(_) => {
                errors.push(format!("{name} is invalid"));
                default
            }
        }
    }
}

fn parse_optional<T>(value: &str, name: &str, errors: &mut Vec<String>) -> Option<T>
where
    T: std::str::FromStr,
{
    if value.trim().is_empty() {
        None
    } else {
        match value.trim().parse::<T>() {
            Ok(value) => Some(value),
            Err(_) => {
                errors.push(format!("{name} is invalid"));
                None
            }
        }
    }
}

fn parse_bool(value: &str, name: &str, default: bool, errors: &mut Vec<String>) -> bool {
    if value.trim().is_empty() {
        return default;
    }

    match value.trim().parse::<bool>() {
        Ok(value) => value,
        Err(_) => {
            errors.push(format!("{name} must be true or false"));
            default
        }
    }
}

fn split_csv_line(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(character),
        }
    }

    if quoted {
        return Err("unterminated quoted CSV field".to_owned());
    }

    fields.push(current.trim().to_owned());
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER_LINE: &str = "name,url,kind,interval_seconds,timeout_seconds,method,expected_status,tcp_port,dns_record_type,dns_expected_value,heartbeat_interval_seconds,heartbeat_grace_period_seconds,is_public,ssl_check_enabled,ssl_expiry_warning_days";

    #[test]
    fn parses_valid_http_row() {
        let csv =
            format!("{HEADER_LINE}\nsite,https://example.com,http,60,10,GET,200,,,,,,,true,14");
        let (rows, errors) = parse_csv(&csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(errors.is_empty());
        assert_eq!(rows[0].name, "site");
        assert_eq!(rows[0].interval_seconds, 60);
    }

    #[test]
    fn parses_head_method() {
        let csv =
            format!("{HEADER_LINE}\nsite,https://example.com,Http,60,10,HEAD,200,,,,,,,true,14");
        let (rows, errors) = parse_csv(&csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(errors.is_empty());
        assert_eq!(rows[0].method, HttpMethod::Head);
    }

    #[test]
    fn rejects_missing_tcp_port() {
        let csv = format!("{HEADER_LINE}\ntcp,example.com,Tcp,60,10,GET,200,,,,,,,true,14");
        let (rows, errors) = parse_csv(&csv).unwrap();
        assert!(rows.is_empty());
        assert!(
            errors[0]
                .errors
                .iter()
                .any(|error| { error == "tcp_port is required for Tcp monitors" })
        );
    }

    #[test]
    fn rejects_invalid_method() {
        let csv =
            format!("{HEADER_LINE}\nsite,https://example.com,Http,60,10,POST,200,,,,,,,true,14");
        let (rows, errors) = parse_csv(&csv).unwrap();
        assert!(rows.is_empty());
        assert!(
            errors[0]
                .errors
                .iter()
                .any(|error| { error.contains("invalid HTTP method") })
        );
    }

    #[test]
    fn preserves_source_row_number_for_preview_errors() {
        let csv = format!(
            "{HEADER_LINE}\nfirst,https://one.example,Http,60,10,GET,200,,,,,,,true,14\nsecond,https://two.example,Http,60,10,GET,200,,,,,,,true,14"
        );
        let (rows, errors) = parse_csv(&csv).unwrap();
        assert!(errors.is_empty());
        assert_eq!(rows[0].source_row, 2);
        assert_eq!(rows[1].source_row, 3);
    }

    #[test]
    fn rejects_invalid_header() {
        assert!(parse_csv("name,url").is_err());
    }
    #[test]
    fn modern_csv_supports_regions_and_rejects_auth() {
        let header = "name,url,kind,interval_seconds,timeout_seconds,method,expected_status,tcp_port,dns_record_type,dns_expected_value,heartbeat_interval_seconds,heartbeat_grace_period_seconds,is_public,ssl_check_enabled,ssl_expiry_warning_days,regions,auth_type";
        let csv = format!(
            "{header}\nsite,https://example.com,http,60,10,GET,200,,,,,,,true,14,us-east;eu-west,none"
        );
        let (rows, errors) = parse_csv(&csv).unwrap();
        assert!(errors.is_empty());
        assert_eq!(rows[0].regions.len(), 2);

        let bad = format!(
            "{header}\nsite,https://example.com,http,60,10,GET,200,,,,,,,true,14,us-east,basic"
        );
        let (_, errors) = parse_csv(&bad).unwrap();
        assert!(errors.iter().any(|e| {
            e.errors
                .iter()
                .any(|x| x.contains("authentication configuration"))
        }));
    }
}
