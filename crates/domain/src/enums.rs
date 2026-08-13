use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Head,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorType {
    None,
    Dns,
    Timeout,
    Connection,
    Ssl,
    Http4xx,
    Http5xx,
    Unknown,
}

pub mod user_role;
pub use user_role::UserRole;

pub mod user_status;
pub use user_status::UserStatus;
pub mod response_part;
pub use response_part::*;
pub mod assertion_operator;
pub use assertion_operator::*;
pub mod notification_channel_kind;
pub use notification_channel_kind::*;
