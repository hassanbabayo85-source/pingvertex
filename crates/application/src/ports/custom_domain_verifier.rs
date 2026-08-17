use async_trait::async_trait;

pub type VerifierResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Abstraction over DNS TXT-record based domain-ownership verification.
///
/// SECURITY: this is the ONLY mechanism that may confirm ownership of a
/// `custom_domain` attached to a status page. Implementations MUST
/// perform a live, real DNS lookup of the domain's TXT records — never
/// simulate/stub success outside of tests — since anything else would let
/// an attacker claim a domain they don't control and hijack its status
/// page traffic via host-header routing.
#[async_trait]
pub trait CustomDomainVerifier: Send + Sync {
    /// Looks up the TXT records for `domain` and returns `true` if any of
    /// them exactly equals `pingvertex-verify=<expected_token>`.
    ///
    /// Returns `Ok(false)` (not an error) for "domain has no matching TXT
    /// record yet" — that's an expected, normal state while the owner is
    /// still configuring DNS. Returns `Err` only for actual lookup
    /// failures (DNS resolution errors, timeouts, etc).
    async fn verify(&self, domain: &str, expected_token: &str) -> VerifierResult<bool>;
}
