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
pub mod extract_source;
pub use extract_source::ExtractSource;
pub mod proxy_check_job_status;
pub use proxy_check_job_status::ProxyCheckJobStatus;
pub mod proxy_scheme;
pub use proxy_scheme::ProxyScheme;
pub mod anonymity_level;
pub use anonymity_level::AnonymityLevel;
