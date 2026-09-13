use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{RumSiteId, TenantId};

/// One day's worth of RUM events for a single site.
///
/// Written by the `RumAggregationWorker` after the raw rows for that day
/// fall outside the 7-day raw retention window. Read by the dashboard
/// when a query range reaches further back than 7 days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RumDailyAggregate {
    pub site_id: RumSiteId,
    pub tenant_id: TenantId,
    pub day: NaiveDate,
    pub total_sessions: i64,
    pub page_view_count: i64,
    pub js_error_count: i64,
    pub resource_error_count: i64,
    /// `NULL` when the day had no page views with a recorded `load_time_ms`.
    pub avg_load_time_ms: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
