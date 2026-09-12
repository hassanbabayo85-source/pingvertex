use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct InitiateCheckoutRequest {
    pub plan_tier: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitiateCheckoutResponse {
    pub checkout_session_id: String,
    pub checkout_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VerifyCheckoutRequest {
    pub checkout_session_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyCheckoutResponse {
    pub status: String,
}
