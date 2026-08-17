//! HTTP-facing DTOs for the "Proxy Check" feature.
//!
//! Mirrors `handlers::flow_types` — HTTP request/response shapes live in
//! `api::handlers`, never in the `application` crate.

use serde::Serialize;

// ============================= Response side =============================

/// Response for `POST /proxy-checks`.
///
/// Deliberately does NOT include the submitted entries or their
/// credentials — this is the response to *creating* the job, before any
/// entry has been checked. Entry-level detail (host/port/scheme/result,
/// but never credentials) is returned by `GET /proxy-checks/{id}`
/// instead, which is a separate, not-yet-built endpoint.
#[derive(Debug, Serialize)]
pub struct ProxyCheckJobHttpResponse {
    pub id: String,
    pub user_id: String,
    /// Reuses `domain::ProxyCheckJobStatus` directly — it already derives
    /// `Serialize`, mirroring how `flow_types::AssertionHttpResponse`
    /// reuses `domain::enums::ResponsePart`/`AssertionOperator` directly
    /// rather than defining a separate HTTP-only enum.
    pub status: domain::ProxyCheckJobStatus,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub total_count: u32,
    pub checked_count: u32,
}

impl From<domain::entities::ProxyCheckJob> for ProxyCheckJobHttpResponse {
    fn from(job: domain::entities::ProxyCheckJob) -> Self {
        ProxyCheckJobHttpResponse {
            id: job.id().to_string(),
            user_id: job.user_id().0.to_string(),
            status: job.status().clone(),
            created_at: job.created_at().to_rfc3339(),
            completed_at: job.completed_at().map(|dt| dt.to_rfc3339()),
            total_count: job.total_count(),
            checked_count: job.checked_count(),
        }
    }
}

/// Response for `GET /proxy-checks/{id}`.
#[derive(Debug, Serialize)]
pub struct ProxyCheckJobDetailHttpResponse {
    pub id: String,
    pub user_id: String,
    pub status: domain::ProxyCheckJobStatus,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub total_count: u32,
    pub checked_count: u32,
    pub entries: Vec<ProxyEntryHttpResponse>,
}

#[derive(Debug, Serialize)]
pub struct ProxyEntryHttpResponse {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub scheme: domain::ProxyScheme,
    pub result: Option<domain::ProxyResult>,
}

impl
    From<(
        domain::entities::ProxyCheckJob,
        Vec<domain::entities::ProxyEntry>,
    )> for ProxyCheckJobDetailHttpResponse
{
    fn from(
        (job, entries): (
            domain::entities::ProxyCheckJob,
            Vec<domain::entities::ProxyEntry>,
        ),
    ) -> Self {
        let entries = entries
            .into_iter()
            .map(|entry| ProxyEntryHttpResponse {
                id: entry.id().to_string(),
                host: entry.host().as_str().to_string(),
                port: entry.port(),
                scheme: entry.scheme().clone(),
                result: entry.result().cloned(),
            })
            .collect();

        ProxyCheckJobDetailHttpResponse {
            id: job.id().to_string(),
            user_id: job.user_id().0.to_string(),
            status: job.status().clone(),
            created_at: job.created_at().to_rfc3339(),
            completed_at: job.completed_at().map(|dt| dt.to_rfc3339()),
            total_count: job.total_count(),
            checked_count: job.checked_count(),
            entries,
        }
    }
}
