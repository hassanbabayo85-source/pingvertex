use crate::enums::PlanTier;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionPlanChangeStatus {
    Pending,
    Completed,
    Failed,
}
impl SubscriptionPlanChangeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionPlanChange {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub tenant_id: Uuid,
    pub old_plan: PlanTier,
    pub new_plan: PlanTier,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub calculated_amount: Decimal,
    pub currency: String,
    pub pricing_version: u32,
    pub status: SubscriptionPlanChangeStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}
