use std::net::IpAddr;

use application::ports::GeoIpLookup;
use maxminddb::{PathElement, Reader};

/// Reuses the exact same GeoIP database and lookup logic as
/// `crates/agent/src/checker.rs::lookup_country_code` — same library
/// (`maxminddb`), same database file (`data/geoip/GeoLite2-Country.mmdb`
/// by default), same decode path. This is intentional: the brief asks
/// for geo summaries only if EXISTING infrastructure supports it, and
/// explicitly says not to build a new geo-IP service — this type is
/// that existing infrastructure, ported into `infra` so the API server
/// (not just the check-running agent) can use it too.
pub struct MaxMindGeoIpLookup {
    reader: Reader<Vec<u8>>,
}

impl MaxMindGeoIpLookup {
    pub fn open(db_path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let reader = Reader::open_readfile(db_path)?;
        Ok(Self { reader })
    }
}

impl GeoIpLookup for MaxMindGeoIpLookup {
    /// Any failure (IP not found, decode error, private/reserved IP,
    /// etc.) degrades gracefully to `None` — a geo-IP miss must never
    /// fail RUM ingestion, it just means this session's country is
    /// unknown.
    fn lookup_country_code(&self, ip: IpAddr) -> Option<String> {
        self.reader
            .lookup(ip)
            .ok()
            .and_then(|result| {
                result
                    .decode_path::<String>(&[
                        PathElement::Key("country"),
                        PathElement::Key("iso_code"),
                    ])
                    .ok()
            })
            .flatten()
    }
}
