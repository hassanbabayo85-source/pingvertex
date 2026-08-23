use domain::enums::PlanTier;
use domain::shared::ids::{CheckoutSessionId, TenantId};

#[derive(Debug, Clone, Copy)]
pub struct InitiateCheckoutCommand {
    pub tenant_id: TenantId,
    pub plan_tier: PlanTier,
}

#[derive(Debug, Clone)]
pub struct InitiateCheckoutOutcome {
    pub checkout_session_id: CheckoutSessionId,
    pub checkout_url: String,
}
