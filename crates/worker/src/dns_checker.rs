use std::time::Instant;

use domain::{CheckResult, Monitor, RegionCode};
use hickory_resolver::{
    Resolver, TokioResolver,
    config::{CLOUDFLARE, ResolverConfig},
    net::{DnsError, NetError, runtime::TokioRuntimeProvider},
    proto::rr::{RData, RecordType},
};

type Result = std::result::Result<CheckResult, Box<dyn std::error::Error + Send + Sync>>;

/// Performs a DNS lookup check for a `MonitorKind::Dns` monitor.
///
/// SECURITY: always uses a hardcoded PUBLIC resolver (Cloudflare), never
/// the host's `/etc/resolv.conf` — this avoids depending on whatever DNS
/// infrastructure happens to be configured on the worker's network, which
/// could be internal/split-horizon DNS.
pub async fn check_dns(monitor: &Monitor) -> Result {
    let record_type_str = monitor
        .dns_record_type
        .as_deref()
        .ok_or("dns_record_type is missing for a Dns monitor")?;
    let record_type = parse_record_type(record_type_str)?;

    let resolver = build_public_resolver()?;

    let started = Instant::now();

    let lookup = match resolver.lookup(monitor.url.as_str(), record_type).await {
        Ok(lookup) => lookup,
        // NXDOMAIN / no records of this type is a normal "check failed"
        // outcome for a monitor, not an internal error.
        Err(NetError::Dns(DnsError::NoRecordsFound(_))) => {
            return Ok(CheckResult::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                format!("no {record_type_str} records found for '{}'", monitor.url),
            ));
        }
        Err(err) => {
            return Ok(CheckResult::failure(
                RegionCode::default_region(),
                started.elapsed().as_millis(),
                format!("DNS lookup failed: {err}"),
            ));
        }
    };

    let latency_ms = started.elapsed().as_millis();

    let answers: Vec<String> = lookup
        .answers()
        .iter()
        .map(|record| rdata_to_string(&record.data))
        .collect();

    if let Some(expected) = &monitor.dns_expected_value
        && !answers.iter().any(|answer| answer == expected)
    {
        return Ok(CheckResult::failure(
            RegionCode::default_region(),
            latency_ms,
            format!(
                "DNS answers [{}] did not contain expected value '{expected}'",
                answers.join(", ")
            ),
        ));
    }

    Ok(CheckResult::success(
        RegionCode::default_region(),
        0,
        latency_ms,
    ))
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
}
