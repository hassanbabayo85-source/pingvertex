#![allow(dead_code)]
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Role {
    Member,
    Admin,
    Owner,
    Auditor,
    Viewer,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    pub email_verified: bool,
    pub tenant_id: String,
    pub role: Role,
    pub totp_enabled: bool,
    #[serde(default)]
    pub platform_admin: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthState {
    Unknown,
    Authenticated(User),
    Unauthenticated,
    TwoFactorRequired { pending_token: String },
}

#[derive(Clone, Copy)]
pub struct AuthContext {
    pub state: RwSignal<AuthState>,
}

pub fn provide_auth_context() {
    let state = RwSignal::new(AuthState::Unknown);
    provide_context(AuthContext { state });
}

pub fn use_auth() -> AuthContext {
    use_context::<AuthContext>().expect("AuthContext not provided")
}

impl AuthContext {
    pub fn set_state(&self, new_state: AuthState) {
        self.state.set(new_state);
    }
}
