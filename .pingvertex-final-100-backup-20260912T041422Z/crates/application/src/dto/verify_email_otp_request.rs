use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct VerifyEmailOtpRequest {
    pub email: String,
    pub code: String,
}
