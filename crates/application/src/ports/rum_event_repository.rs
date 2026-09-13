use std::error::Error;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{RumJsError, RumPageView, RumResourceError, RumSession};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Result of `RumEventRepository::upsert_session`.
///
/// `created = true` when the beacon started a *new* 30-minute session,
/// which is what gets billed under the `rum_session` PAYG operation.
/// `created = false` when the beacon merely extended an existing active
/// session (no new charge).
#[derive(Debug, Clone, PartialEq)]
pub struct UpsertedRumSession {
    pub session: RumSession,
    pub created: bool,
}

/// Persistence for RUM ingestion writes: sessions, page views, JS errors,
/// and resource errors.
///
/// All writes here are expected to already carry the correct `tenant_id`
/// and `site_id` (resolved server-side by `IngestRumEventUseCase` via
/// `RumSiteRepository::find_by_site_key` — never taken from client input),
/// so implementations do not need to re-derive or re-validate tenant
/// ownership; they persist what they're given.
#[async_trait]
pub trait RumEventRepository: Send + Sync {
    /// Finds an existing session by `(site_id, session_key)` and either:
    ///
    ///   * extends it (`created = false`) when `last_seen_at` is within
    ///     `RumSession::SESSION_WINDOW_MINUTES` of `now`, or
    ///   * starts a new session (`created = true`) when the stored one is
    ///     older than the window (or none exists).
    ///
    /// Implementations MUST perform this as a single atomic statement
    /// (`INSERT ... ON CONFLICT DO UPDATE ... WHERE`), not a separate
    /// find-then-write, since concurrent beacons from the same session
    /// (page view + errors fired close together) would otherwise race.
    async fn upsert_session(
        &self,
        session: &RumSession,
        now: DateTime<Utc>,
    ) -> RepoResult<UpsertedRumSession>;

    async fn insert_page_view(&self, page_view: &RumPageView) -> RepoResult<()>;

    async fn insert_js_error(&self, error: &RumJsError) -> RepoResult<()>;

    async fn insert_resource_error(&self, error: &RumResourceError) -> RepoResult<()>;
}

#[async_trait]
impl<T> RumEventRepository for std::sync::Arc<T>
where
    T: RumEventRepository + ?Sized,
{
    async fn upsert_session(
        &self,
        session: &RumSession,
        now: DateTime<Utc>,
    ) -> RepoResult<UpsertedRumSession> {
        (**self).upsert_session(session, now).await
    }

    async fn insert_page_view(&self, page_view: &RumPageView) -> RepoResult<()> {
        (**self).insert_page_view(page_view).await
    }

    async fn insert_js_error(&self, error: &RumJsError) -> RepoResult<()> {
        (**self).insert_js_error(error).await
    }

    async fn insert_resource_error(&self, error: &RumResourceError) -> RepoResult<()> {
        (**self).insert_resource_error(error).await
    }
}
