use std::error::Error;

use async_trait::async_trait;
use domain::{RumJsError, RumPageView, RumResourceError, RumSession};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

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
    /// Finds an existing session by `(site_id, session_key)` and bumps its
    /// `last_seen_at`, or creates a new one if none exists yet.
    ///
    /// This must be a single atomic upsert at the storage layer (e.g. `ON
    /// CONFLICT ... DO UPDATE`), not a separate find-then-insert, since
    /// concurrent beacons from the same session (page view + errors fired
    /// close together) would otherwise race and could create duplicate
    /// session rows.
    async fn upsert_session(&self, session: &RumSession) -> RepoResult<RumSession>;

    async fn insert_page_view(&self, page_view: &RumPageView) -> RepoResult<()>;

    async fn insert_js_error(&self, error: &RumJsError) -> RepoResult<()>;

    async fn insert_resource_error(&self, error: &RumResourceError) -> RepoResult<()>;
}

#[async_trait]
impl<T> RumEventRepository for std::sync::Arc<T>
where
    T: RumEventRepository + ?Sized,
{
    async fn upsert_session(&self, session: &RumSession) -> RepoResult<RumSession> {
        (**self).upsert_session(session).await
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
