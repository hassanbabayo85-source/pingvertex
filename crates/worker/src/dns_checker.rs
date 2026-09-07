use std::time::{Duration, Instant};

use domain::{CheckErrorKind, CheckResult, Monitor, RegionCode};
use hickory_resolver::{
    Resolver, TokioResolver,
    config::{CLOUDFLARE, ResolverConfig},
    net::{DnsError, NetError, runtime::TokioRuntimeProvider},
    proto::rr::{RData, RecordType},
};
use tokio::time::timeout;
use url::Url;

type Result = std::result::Result<CheckResult, Box<dyn std::error::Error + Send + Sync>>;
type OutcomeResult = std::result::Result<DnsCheckOutcome, Box<dyn std::error::Error + Send + Sync>>;

/// The result of a DNS check execution plus its canonical error
/// classification. Mirrors `HttpCheckOutcome` so active check executors
/// can persist the exact same `ErrorType` taxonomy.
#[derive(Debug, Clone)]
pub struct DnsCheckOutcome {
    pub result: CheckResult,
    pub error_kind: CheckErrorKind,
    pub attempted: bool,
}

impl DnsCheckOutcome {
    fn success(region: RegionCode, latency_ms: u128) -> Self {
        Self {
            result: CheckResult::success(region, 0, latency_ms),
            error_kind: CheckErrorKind::Unknown,
            attempted: true,
        }
    }

    fn failure(
        region: RegionCode,
        latency_ms: u128,
        error_kind: CheckErrorKind,
        error: impl Into<String>,
    ) -> Self {
        Self {
            result: CheckResult::failure(region, latency_ms, error),
            error_kind,
            attempted: true,
        }
    }
}

fn extract_host(raw: &str) -> String {
    if raw.contains("://")
        && let Ok(parsed) = Url::parse(raw)
        && let Some(host) = parsed.host_str()
    {
        return host.to_string();
    }
    raw.to_string()
}

/// Performs a DNS lookup check for a `MonitorKind::Dns` monitor.
///
/// SECURITY: always uses a hardcoded PUBLIC resolver (Cloudflare), never
/// the host's `/etc/resolv.conf` — this avoids depending on whatever DNS
/// infrastructure happens to be configured on the worker's network, which
/// could be internal/split-horizon DNS.
///
/// TIMEOUT: the entire lookup is wrapped in `tokio::time::timeout` using
/// the monitor's configured timeout, so a slow/blackholed resolver cannot
/// hold a worker task past the monitor's budget.
pub async fn check_dns_outcome(monitor: &Monitor) -> OutcomeResult {
    let record_type_str = monitor
        .dns_record_type
        .as_deref()
        .ok_or("dns_record_type is missing for a Dns monitor")?;
    let record_type = parse_record_type(record_type_str)?;

    let resolver = build_public_resolver()?;
    let host = extract_host(&monitor.url);
    let started = Instant::now();

    let lookup_result = timeout(
        Duration::from_secs(monitor.timeout_seconds.into()),
        resolver.lookup(host.as_str(), record_type),
    )
    .await;

    let lookup = match lookup_result {
        Err(_) => {
            return Ok(DnsCheckOutcome::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                CheckErrorKind::Timeout,
                "DNS lookup timed out",
            ));
        }
        Ok(Err(NetError::Dns(DnsError::NoRecordsFound(_)))) => {
            return Ok(DnsCheckOutcome::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                CheckErrorKind::DnsError,
                format!("no {record_type_str} records found for '{host}'"),
            ));
        }
        Ok(Err(err)) => {
            return Ok(DnsCheckOutcome::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                CheckErrorKind::DnsError,
                format!("DNS lookup failed: {err}"),
            ));
        }
        Ok(Ok(lookup)) => lookup,
    };

    let latency_ms = started.elapsed().as_millis();

    let answers: Vec<String> = lookup
        .answers()
        .iter()
        .map(|record| rdata_to_string(&record.data))
        .collect();

    match evaluate_dns_answers(
        &answers,
        monitor.dns_expected_value.as_deref(),
        &host,
        record_type_str,
    ) {
        Ok(true) => Ok(DnsCheckOutcome::success(
            RegionCode::default_region(),
            latency_ms,
        )),
        Ok(false) => Ok(DnsCheckOutcome::failure(
            RegionCode::default_region(),
            latency_ms,
            CheckErrorKind::AssertionFailed,
            format!(
                "DNS answers [{}] did not contain expected value '{}'",
                answers.join(", "),
                monitor.dns_expected_value.as_deref().unwrap_or_default()
            ),
        )),
        Err(err) => Ok(DnsCheckOutcome::failure(
            RegionCode::default_region(),
            latency_ms,
            CheckErrorKind::InternalError,
            err,
        )),
    }
}

/// Pure evaluation of DNS answers against an expected record value.
/// Returns `Ok(true)` when the check passes, `Ok(false)` when the
/// expected value is set and not present, and `Err` for malformed input.
fn evaluate_dns_answers(
    answers: &[String],
    expected: Option<&str>,
    host: &str,
    record_type: &str,
) -> std::result::Result<bool, String> {
    if answers.is_empty() {
        return Ok(false);
    }

    if let Some(expected) = expected
        && !answers.iter().any(|answer| answer == expected)
    {
        return Ok(false);
    }

    let _ = (host, record_type);
    Ok(true)
}

/// Backwards-compatible DNS entry point used by the legacy sweep
/// scheduler (`scheduler.rs`).
pub async fn check_dns(monitor: &Monitor) -> Result {
    let outcome = check_dns_outcome(monitor).await?;
    Ok(outcome.result)
}

fn rdata_to_string(rdata: &RData) -> String {
    match rdata {
        RData::A(ip) => ip.to_string(),
        RData::AAAA(ip) => ip.to_string(),
        RData::CNAME(name) => name.to_string(),
        RData::TXT(txt) => txt.to_string(),
        RData::MX(mx) => mx.exchange.to_string(),
        RData::NS(ns) => ns.to_string(),
        other => other.record_type().to_string(),
    }
}

fn parse_record_type(value: &str) -> std::result::Result<RecordType, String> {
    match value.to_ascii_uppercase().as_str() {
        "A" => Ok(RecordType::A),
        "AAAA" => Ok(RecordType::AAAA),
        "CNAME" => Ok(RecordType::CNAME),
        "TXT" => Ok(RecordType::TXT),
        "MX" => Ok(RecordType::MX),
        "NS" => Ok(RecordType::NS),
        other => Err(format!("unsupported DNS record type: {other}")),
    }
}

/// Builds a resolver using a hardcoded public resolver (Cloudflare), never
/// the system's `/etc/resolv.conf`. See the module-level SECURITY note.
fn build_public_resolver()
-> std::result::Result<TokioResolver, Box<dyn std::error::Error + Send + Sync>> {
    let builder = Resolver::builder_with_config(
        ResolverConfig::udp_and_tcp(&CLOUDFLARE),
        TokioRuntimeProvider::default(),
    );

    let resolver = builder
        .build()
        .map_err(|err| format!("failed to build DNS resolver: {err}"))?;

    Ok(resolver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_record_types() {
        assert_eq!(parse_record_type("a").unwrap(), RecordType::A);
        assert_eq!(parse_record_type("TXT").unwrap(), RecordType::TXT);
        assert_eq!(parse_record_type("cname").unwrap(), RecordType::CNAME);
    }

    #[test]
    fn rejects_unknown_record_type() {
        assert!(parse_record_type("SRV").is_err());
    }

    #[test]
    fn expected_value_match_succeeds() {
        let answers = vec!["93.184.216.34".to_string()];
        assert_eq!(
            evaluate_dns_answers(&answers, Some("93.184.216.34"), "example.com", "A"),
            Ok(true)
        );
    }

    #[test]
    fn expected_value_mismatch_fails() {
        let answers = vec!["93.184.216.34".to_string()];
        assert_eq!(
            evaluate_dns_answers(&answers, Some("1.2.3.4"), "example.com", "A"),
            Ok(false)
        );
    }

    #[test]
    fn missing_expected_value_but_answers_present_succeeds() {
        let answers = vec!["93.184.216.34".to_string()];
        assert_eq!(
            evaluate_dns_answers(&answers, None, "example.com", "A"),
            Ok(true)
        );
    }

    #[test]
    fn empty_answers_fails() {
        let answers: Vec<String> = vec![];
        assert_eq!(
            evaluate_dns_answers(&answers, None, "example.com", "A"),
            Ok(false)
        );
    }
}
