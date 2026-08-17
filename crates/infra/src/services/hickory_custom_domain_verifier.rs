use application::ports::{CustomDomainVerifier, VerifierResult};
use async_trait::async_trait;
use hickory_resolver::{
    Resolver, TokioResolver,
    config::{CLOUDFLARE, ResolverConfig},
    net::{DnsError, NetError, runtime::TokioRuntimeProvider},
    proto::rr::RData,
};

/// DNS TXT-record based domain-ownership verifier.
///
/// SECURITY: this performs a REAL, live DNS lookup — it is the sole
/// mechanism by which `custom_domain` ownership is established before
/// host-header routing is allowed to use it. See the SECURITY note on
/// `CustomDomainVerifier` in the application port.
pub struct HickoryCustomDomainVerifier {
    resolver: TokioResolver,
}

impl HickoryCustomDomainVerifier {
    /// Builds a verifier using the system's configured DNS resolver
    /// (`/etc/resolv.conf` on Linux), falling back to Cloudflare's public
    /// resolver if system config can't be read (e.g. in some containers).
    pub fn new() -> Self {
        let builder = Resolver::builder_tokio().unwrap_or_else(|_| {
            Resolver::builder_with_config(
                ResolverConfig::udp_and_tcp(&CLOUDFLARE),
                TokioRuntimeProvider::default(),
            )
        });

        // `build()` only fails if the resolver configuration itself is
        // invalid (e.g. zero configured name servers) — not a possible
        // outcome for either `builder_tokio()`'s system config or the
        // hardcoded Cloudflare fallback above, so this is not a
        // reachable-in-practice panic surface.
        let resolver = builder
            .build()
            .expect("hickory resolver configuration is always valid here");

        Self { resolver }
    }
}

impl Default for HickoryCustomDomainVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CustomDomainVerifier for HickoryCustomDomainVerifier {
    async fn verify(&self, domain: &str, expected_token: &str) -> VerifierResult<bool> {
        let expected = format!("pingvertex-verify={expected_token}");

        let lookup = match self.resolver.txt_lookup(domain.to_string()).await {
            Ok(lookup) => lookup,
            // NXDOMAIN / no TXT records present yet is a normal "not
            // verified yet" state, not an error — the owner may simply
            // not have published the record yet.
            Err(NetError::Dns(DnsError::NoRecordsFound(_))) => return Ok(false),
            Err(err) => return Err(Box::new(err)),
        };

        let matched = lookup.answers().iter().any(|record| match &record.data {
            RData::TXT(txt) => txt.to_string() == expected,
            _ => false,
        });

        Ok(matched)
    }
}
