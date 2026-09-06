use serde::{Deserialize, Serialize};

/// H12 — Proration Service: request body for
/// `PATCH /api/v1/subscription/usage`.
///
/// SECURITY (P0): this request intentionally carries NO plan-tier or
/// usage fields. The usage snapshot applied to the subscription
/// (monitors/interval_seconds/retention_days) is always derived
/// server-side from the tenant's OWN authoritative
/// `Subscription::plan_tier` (see
/// `UpdateSubscriptionUsageUseCase::execute`) — never from anything the
/// client sends. A previous version of this struct accepted a
/// client-supplied `plan_tier` string, which let any caller holding the
/// `ManageBilling` permission overwrite their tenant's billing usage
/// snapshot with a cheaper plan's numbers, independent of what they
/// were actually subscribed to and paying for.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSubscriptionUsageRequest {}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateSubscriptionUsageResponse {
    pub plan_tier: String,
    pub monitors: u64,
    pub interval_seconds: u32,
    pub retention_days: u32,
    pub pricing_version: u32,
}
