use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
}

/// Manual Debug that redacts the session JWT.
impl std::fmt::Debug for AuthResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthResponse")
            .field("access_token", &"[REDACTED]")
            .finish()
    }
}
