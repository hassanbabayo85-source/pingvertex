use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ProxyCheckJobHttpResponse {
    pub id: String,
    pub user_id: String,
    pub created_at: String,
    pub status: String,
    pub total_count: u32,
    pub checked_count: u32,
    pub successful_count: u32,
    pub failed_count: u32,
    pub started_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub completed_at: Option<String>,
    pub failed_at: Option<String>,
    pub failure_code: Option<String>,
    pub canceled_at: Option<String>,
}
impl From<domain::entities::ProxyCheckJob> for ProxyCheckJobHttpResponse {
    fn from(job: domain::entities::ProxyCheckJob) -> Self {
        Self {
            id: job.id().to_string(),
            user_id: job.user_id().0.to_string(),
            created_at: job.created_at().to_rfc3339(),
            status: match job.status() {
                domain::ProxyCheckJobStatus::Pending => "pending".into(),
                domain::ProxyCheckJobStatus::Running => "running".into(),
                domain::ProxyCheckJobStatus::Completed => "completed".into(),
                domain::ProxyCheckJobStatus::Failed => "failed".into(),
                domain::ProxyCheckJobStatus::Canceled => "canceled".into(),
            },
            total_count: job.total_count(),
            checked_count: job.checked_count(),
            successful_count: job.successful_count(),
            failed_count: job.failed_count(),
            started_at: job.started_at().map(|d| d.to_rfc3339()),
            heartbeat_at: job.heartbeat_at().map(|d| d.to_rfc3339()),
            completed_at: job.completed_at().map(|d| d.to_rfc3339()),
            failed_at: job.failed_at().map(|d| d.to_rfc3339()),
            failure_code: job.failure_reason().map(|reason| {
                if reason.starts_with("PAYG balance exhausted") {
                    "PAYG_BALANCE_EXHAUSTED".to_owned()
                } else {
                    "JOB_FAILED".to_owned()
                }
            }),
            canceled_at: job.canceled_at().map(|d| d.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProxyCheckResultHttpResponse {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub scheme: domain::ProxyScheme,
    pub result: Option<domain::ProxyResult>,
}

impl From<domain::entities::ProxyEntry> for ProxyCheckResultHttpResponse {
    fn from(entry: domain::entities::ProxyEntry) -> Self {
        Self {
            id: entry.id().to_string(),
            host: entry.host().as_str().to_string(),
            port: entry.port(),
            scheme: entry.scheme().clone(),
            result: entry.result().cloned(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProxyCheckResultsHttpResponse {
    pub items: Vec<ProxyCheckResultHttpResponse>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
}

// ---------------------------------------------------------------------------
// Preview endpoint HTTP types
// ---------------------------------------------------------------------------

/// Response returned by `POST /api/v1/proxy-checks/preview`. Purely a
/// read-only dry run — creating this response does NOT touch the DB,
/// does NOT consume quota, and does NOT create a job.
///
/// SECURITY: `accepted` deliberately does NOT include username/password
/// values. Each entry carries `has_credentials: bool` instead, so that
/// the response can never become a place where credentials leak (into
/// logs, into a UI cache, into a screenshot, etc.). The frontend does
/// not need to display credentials; it only needs to know whether an
/// entry has them.
#[derive(Debug, Serialize)]
pub struct ProxyPreviewHttpResponse {
    /// Every non-empty, non-comment input unit (CSV line or JSON array
    /// element).
    pub total: usize,
    /// Accepted + duplicate — i.e. entries that passed shape parsing
    /// and domain validation.
    pub valid: usize,
    /// Shape-parse failures only (bad `host:port`, unknown scheme, etc.).
    pub invalid: usize,
    /// Number of entries that duplicated an earlier entry in the same batch.
    pub duplicate_count: usize,
    /// Total failures — shape-parse + domain validation.
    pub rejected_count: usize,
    pub accepted: Vec<ProxyEntryPreviewHttpResponse>,
    pub rejected: Vec<RejectedProxyHttp>,
    pub duplicates: Vec<DuplicateProxyHttp>,
}

/// One accepted proxy entry, without credentials.
///
/// `position` is the 1-based position of this entry in the original
/// input (CSV line number or JSON array index). The dashboard uses it to
/// map this credential-free summary back to the original parsed input,
/// which still carries the credentials needed for submission.
#[derive(Debug, Serialize)]
pub struct ProxyEntryPreviewHttpResponse {
    pub position: usize,
    pub host: String,
    pub port: u16,
    pub scheme: domain::ProxyScheme,
    pub has_credentials: bool,
}

/// One rejected input unit.
#[derive(Debug, Serialize)]
pub struct RejectedProxyHttp {
    /// 1-based CSV line number, or 1-based JSON array index.
    pub position: usize,
    /// Raw input (truncated to 256 bytes by the parser).
    pub raw: String,
    pub reason: String,
}

/// One entry that duplicated an earlier entry in the same batch.
#[derive(Debug, Serialize)]
pub struct DuplicateProxyHttp {
    pub position: usize,
    pub raw: String,
    /// Position of the FIRST entry that matched the same dedup key.
    pub first_seen_at: usize,
}
