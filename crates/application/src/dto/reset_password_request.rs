use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

/// PART 7 — manual Debug that redacts the reset token and new password.
impl std::fmt::Debug for ResetPasswordRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResetPasswordRequest")
            .field("token", &"[REDACTED]")
            .field("new_password", &"[REDACTED]")
            .finish()
    }
}
