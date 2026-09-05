use crate::enums::ProxyScheme;
use crate::errors::DomainError;
use crate::shared::ids::{ProxyCheckJobId, ProxyEntryId};
use crate::value_objects::{ProxyCredentials, ProxyHost, ProxyResult, is_allowed_proxy_port};

/// A single proxy (IP:PORT, optionally credentialed) submitted as part of
/// a `ProxyCheckJob`, along with the result of checking it (once checked).
#[derive(Debug)]
pub struct ProxyEntry {
    id: ProxyEntryId,
    job_id: ProxyCheckJobId,
    host: ProxyHost,
    port: u16,
    scheme: ProxyScheme,
    credentials: Option<ProxyCredentials>,
    result: Option<ProxyResult>,
}

impl ProxyEntry {
    /// Creates a new, not-yet-checked proxy entry.
    pub fn new(
        id: ProxyEntryId,
        job_id: ProxyCheckJobId,
        host: ProxyHost,
        port: u16,
        scheme: ProxyScheme,
        credentials: Option<ProxyCredentials>,
    ) -> Result<Self, DomainError> {
        if !is_allowed_proxy_port(port) {
            return Err(DomainError::ProxyPortNotAllowed);
        }

        Ok(Self {
            id,
            job_id,
            host,
            port,
            scheme,
            credentials,
            result: None,
        })
    }

    /// Reconstructs a `ProxyEntry` from data already persisted in
    /// storage, including whatever `result` (if any) was recorded the
    /// last time it was checked.
    ///
    /// This exists solely for repository implementations
    /// (`ProxyCheckJobRepository::find_entries_by_job_id`) to rebuild an
    /// entry exactly as stored — `new()` cannot express this, since it
    /// always starts an entry at `result: None` by design (a freshly
    /// *created* entry, by definition, has not been checked yet).
    /// Application code that is not a repository should never call this
    /// directly; an entry's result should only ever be set through
    /// `set_result`.
    ///
    /// No validation is performed here (unlike `new()`'s port-allowlist
    /// check) because the values passed in already passed that
    /// validation once, when the entry was first created and saved.
    pub fn from_persisted(
        id: ProxyEntryId,
        job_id: ProxyCheckJobId,
        host: ProxyHost,
        port: u16,
        scheme: ProxyScheme,
        credentials: Option<ProxyCredentials>,
        result: Option<ProxyResult>,
    ) -> Self {
        Self {
            id,
            job_id,
            host,
            port,
            scheme,
            credentials,
            result,
        }
    }

    pub fn id(&self) -> ProxyEntryId {
        self.id
    }

    pub fn job_id(&self) -> ProxyCheckJobId {
        self.job_id
    }

    pub fn host(&self) -> &ProxyHost {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn scheme(&self) -> &ProxyScheme {
        &self.scheme
    }

    pub fn credentials(&self) -> Option<&ProxyCredentials> {
        self.credentials.as_ref()
    }

    pub fn result(&self) -> Option<&ProxyResult> {
        self.result.as_ref()
    }

    /// The only way to change `result` after construction. The worker
    /// calls this once it has finished checking this proxy entry.
    pub fn set_result(&mut self, result: ProxyResult) {
        self.result = Some(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> ProxyHost {
        ProxyHost::new("8.8.8.8").unwrap()
    }

    #[test]
    fn accepts_allowed_port() {
        let entry = ProxyEntry::new(
            ProxyEntryId::new(),
            ProxyCheckJobId::new(),
            host(),
            443,
            ProxyScheme::Https,
            None,
        )
        .unwrap();

        assert_eq!(entry.port(), 443);
        assert!(entry.result().is_none());
    }

    #[test]
    fn rejects_disallowed_port() {
        let result = ProxyEntry::new(
            ProxyEntryId::new(),
            ProxyCheckJobId::new(),
            host(),
            22,
            ProxyScheme::Http,
            None,
        );

        assert_eq!(result.err(), Some(DomainError::ProxyPortNotAllowed));
    }

    #[test]
    fn from_persisted_reconstructs_an_entry_with_a_result() {
        let id = ProxyEntryId::new();
        let job_id = ProxyCheckJobId::new();
        let result = ProxyResult {
            host: "8.8.8.8".to_string(),
            port: 8080,
            scheme: "http".to_string(),
            reachable: true,
            latency_ms: Some(120),
            anonymity: None,
            supports_https: true,
            country_code: Some("US".to_string()),
            error_code: None,
            error_message: None,
        };

        let entry = ProxyEntry::from_persisted(
            id,
            job_id,
            host(),
            8080,
            ProxyScheme::Http,
            None,
            Some(result),
        );

        assert_eq!(entry.id(), id);
        assert_eq!(entry.job_id(), job_id);
        assert_eq!(entry.port(), 8080);
        assert!(entry.result().is_some());
        assert!(entry.result().unwrap().reachable);
    }
}
