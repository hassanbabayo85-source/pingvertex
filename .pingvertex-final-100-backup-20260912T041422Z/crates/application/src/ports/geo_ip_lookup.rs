use std::net::IpAddr;

/// Resolves an ISO 3166-1 alpha-2 country code for an IP address.
///
/// Synchronous (not `async_trait`) because a real implementation
/// (`infra::MaxMindGeoIpLookup`) is a pure in-memory database lookup, no
/// I/O — this mirrors how `crates/agent/src/checker.rs` already uses the
/// same underlying library for check regions.
///
/// SECURITY/PRIVACY: implementations must resolve ONLY a country-level
/// code, never city/region/lat-lon — see the migration comment on
/// `rum_sessions.country_code` and the Task D SDK's "no PII" rule this
/// supports (the SDK itself never touches geography at all; this is
/// populated server-side from the connection's IP, which the SDK never
/// sees or sends).
pub trait GeoIpLookup: Send + Sync {
    fn lookup_country_code(&self, ip: IpAddr) -> Option<String>;
}

impl<T> GeoIpLookup for std::sync::Arc<T>
where
    T: GeoIpLookup + ?Sized,
{
    fn lookup_country_code(&self, ip: IpAddr) -> Option<String> {
        (**self).lookup_country_code(ip)
    }
}
