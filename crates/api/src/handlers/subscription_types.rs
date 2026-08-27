use serde::{Deserialize, Serialize};

/// H12 — Proration Service: request body for
/// `PATCH /api/v1/subscription/usage`.
///
/// The tenant selects a target plan tier; the actual usage snapshot
/// (monitors/interval_seconds/retention_days) applied to the
/// subscription is derived from `domain::entities::Plan::for_tier`, not
/// taken directly from the client — this keeps usage values tied to the
/// fixed plan catalog rather than letting a caller set arbitrary numbers.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSubscriptionUsageRequest {
    pub plan_tier: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateSubscriptionUsageResponse {
    pub plan_tier: String,
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
    pub pricing_version: u32,
}
