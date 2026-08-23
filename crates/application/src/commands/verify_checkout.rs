use domain::enums::CheckoutStatus;
use domain::shared::ids::{CheckoutSessionId, TenantId};

#[derive(Debug, Clone, Copy)]
pub struct VerifyCheckoutCommand {
    pub checkout_session_id: CheckoutSessionId,
    pub tenant_id: TenantId,
}

#[derive(Debug, Clone, Copy)]
pub struct VerifyCheckoutOutcome {
    pub checkout_status: CheckoutStatus,
}
