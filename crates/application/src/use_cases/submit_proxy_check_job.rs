use std::{error::Error, fmt};

use chrono::Utc;

use crate::{
    dto::CreateProxyCheckJobRequest,
    ports::{IdGenerator, ProxyCheckJobRepository},
};
use domain::{DomainError, ProxyCheckJob, ProxyCredentials, ProxyEntry, ProxyHost};

/// Hard cap on the number of proxy-check jobs a single user may submit
/// per calendar day (UTC).
///
/// Without this limit a single account could submit an unbounded number
/// of bulk proxy-check jobs — each carrying up to `MAX_ENTRIES_PER_JOB`
/// (500) entries — causing unbounded worker load. The count resets at
/// UTC midnight because `ProxyCheckJob::created_at` is stored in UTC and
/// `ProxyCheckJobRepository::find_jobs_by_user_id` has no date-range
/// filtering of its own (see `idx_proxy_check_jobs_user_id_created_at`,
/// which this use case's quota check relies on for performance), so
/// "today" is computed by comparing each job's `created_at` date against
/// `Utc::now()`'s date rather than by a dedicated query.
pub const MAX_JOBS_PER_USER_PER_DAY: usize = 5;

#[derive(Debug)]
pub enum SubmitProxyCheckJobError {
    /// The user has already submitted `MAX_JOBS_PER_USER_PER_DAY` jobs
    /// today (UTC).
    QuotaExceeded { limit: usize },
    /// The request itself was malformed in a way that never needs to
    /// reach the domain layer (e.g. an empty entry list).
    InvalidInput(String),
    /// A domain invariant was violated while constructing the job or one
    /// of its entries (e.g. too many entries, a disallowed proxy port, a
    /// private/reserved proxy host).
    DomainError(DomainError),
    /// The repository failed to read or persist data.
    RepoError(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for SubmitProxyCheckJobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuotaExceeded { limit } => write!(
                f,
                "daily proxy check job limit reached: at most {limit} jobs may be submitted per day"
            ),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::DomainError(err) => write!(f, "{err}"),
            Self::RepoError(err) => write!(f, "{err}"),
        }
    }
}

impl Error for SubmitProxyCheckJobError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::QuotaExceeded { .. } => None,
            Self::InvalidInput(_) => None,
            Self::DomainError(err) => Some(err),
            Self::RepoError(err) => Some(err.as_ref()),
        }
    }
}

/// Submits a new bulk proxy-check job on behalf of a user.
///
/// Takes `CreateProxyCheckJobRequest.entries` (already-parsed
/// `ProxyEntryInput`s — this use case does not parse raw CSV/JSON; that
/// is an API-layer concern handled by `parsing::proxy_list_parser`
/// before this use case is ever called), enforces the per-user daily
/// quota, builds the `ProxyCheckJob` and its `ProxyEntry` children as
/// domain entities, and persists them together via
/// `ProxyCheckJobRepository::save_job`.
///
/// Credential encryption is NOT performed here: `ProxyEntry` holds
/// plaintext `ProxyCredentials`, and encryption happens transparently at
/// the persistence boundary inside `ProxyCheckJobRepository::save_job`
/// (see `PostgresProxyCheckJobRepository`, which owns a
/// `ProxyCredentialCipher` for exactly this reason). This use case is
/// therefore not generic over a cipher and has only two dependencies.
pub struct SubmitProxyCheckJobUseCase<R, G> {
    repository: R,
    id_generator: G,
}

impl<R, G> SubmitProxyCheckJobUseCase<R, G>
where
    R: ProxyCheckJobRepository,
    G: IdGenerator,
{
    pub fn new(repository: R, id_generator: G) -> Self {
        Self {
            repository,
            id_generator,
        }
    }

    pub async fn execute(
        &self,
        request: CreateProxyCheckJobRequest,
    ) -> Result<ProxyCheckJob, SubmitProxyCheckJobError> {
        // Cheap, in-memory check done first — before any repository/database
        // call — so an empty entries list is rejected without spending a
        // database round-trip on it. `ProxyCheckJob::new` would also reject
        // an empty list (as `DomainError::EmptyProxyEntries`), but checking
        // here up front keeps `InvalidInput` available for request-shape
        // problems that never need to reach the domain layer at all.
        if request.entries.is_empty() {
            return Err(SubmitProxyCheckJobError::InvalidInput(
                "at least one proxy entry is required".to_string(),
            ));
        }

        // Per-user daily quota check. `find_jobs_by_user_id` has no
        // date-range filtering of its own, so "today" is computed here by
        // comparing each returned job's `created_at` date (UTC) against
        // today's date (UTC).
        let existing_jobs = self
            .repository
            .find_jobs_by_user_id(request.user_id)
            .await
            .map_err(SubmitProxyCheckJobError::RepoError)?;

        let today = Utc::now().date_naive();
        let jobs_submitted_today = existing_jobs
            .iter()
            .filter(|job| job.created_at().date_naive() == today)
            .count();

        if jobs_submitted_today >= MAX_JOBS_PER_USER_PER_DAY {
            return Err(SubmitProxyCheckJobError::QuotaExceeded {
                limit: MAX_JOBS_PER_USER_PER_DAY,
            });
        }

        let job_id = self.id_generator.next_proxy_check_job_id().await;

        // `ProxyCheckJob::new` enforces the remaining invariants
        // (non-zero entry count, `MAX_ENTRIES_PER_JOB` upper bound) that
        // are not already covered by the `InvalidInput` check above.
        let job = ProxyCheckJob::new(job_id, request.user_id, request.entries.len())
            .map_err(SubmitProxyCheckJobError::DomainError)?;

        let mut entries = Vec::with_capacity(request.entries.len());
        for input in &request.entries {
            let entry_id = self.id_generator.next_proxy_entry_id().await;

            let host = ProxyHost::new(input.host.clone())
                .map_err(SubmitProxyCheckJobError::DomainError)?;

            // `ProxyCredentials::new` rejects a username/password pair
            // that are both empty, so only construct it when at least one
            // of the two was actually supplied.
            let credentials = match (&input.username, &input.password) {
                (None, None) => None,
                (username, password) => Some(
                    ProxyCredentials::new(
                        username.clone().unwrap_or_default(),
                        password.clone().unwrap_or_default(),
                    )
                    .map_err(SubmitProxyCheckJobError::DomainError)?,
                ),
            };

            let entry = ProxyEntry::new(
                entry_id,
                job_id,
                host,
                input.port,
                input.scheme.clone(),
                credentials,
            )
            .map_err(SubmitProxyCheckJobError::DomainError)?;

            entries.push(entry);
        }

        self.repository
            .save_job(&job, &entries)
            .await
            .map_err(SubmitProxyCheckJobError::RepoError)?;

        Ok(job)
    }
}
