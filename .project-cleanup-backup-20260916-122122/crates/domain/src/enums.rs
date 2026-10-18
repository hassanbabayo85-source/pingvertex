use serde::{Deserialize, Serialize};
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    #[default]
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

/// Canonical check-error taxonomy shared by every active check worker
/// (HTTP, TCP, DNS, and future check types).
///
/// The wire/database serialization names are the ones in B18's spec:
/// `DNS_ERROR`, `CONNECTION_ERROR`, `TLS_ERROR`, `TIMEOUT`,
/// `HTTP_ERROR`, `ASSERTION_FAILED`, `INTERNAL_ERROR`, `UNKNOWN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckErrorKind {
    DnsError,
    ConnectionError,
    TlsError,
    Timeout,
    HttpError,
    AssertionFailed,
    InternalError,
    Unknown,
}
pub mod user_role;
pub use user_role::UserRole;
pub mod user_status;

// H14 — Help Center & Support:
pub mod help_article_status;
pub mod support_category;
pub mod support_ticket_status;
pub use help_article_status::HelpArticleStatus;
pub use support_category::SupportCategory;
pub use support_ticket_status::SupportTicketStatus;
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
pub mod proxy_check_outcome;
pub mod proxy_failure_kind;
pub mod proxy_scheme;
pub use proxy_check_outcome::ProxyCheckOutcome;
pub use proxy_failure_kind::ProxyFailureKind;
pub use proxy_scheme::ProxyScheme;
pub mod anonymity_level;
pub use anonymity_level::AnonymityLevel;
pub mod monitor_kind;
pub use monitor_kind::MonitorKind;
pub mod audit_action;
pub use audit_action::AuditAction;
pub mod api_key_scope;
pub use api_key_scope::ApiKeyScope;

// B11 — Plan / Subscription / Entitlement model:
pub mod plan_tier;
pub use plan_tier::PlanTier;

// B12 — Subscription Lifecycle:
pub mod subscription_status;
pub use subscription_status::SubscriptionStatus;

// B14 — Billing Provider Abstraction & Checkout:
pub mod checkout_status;
pub use checkout_status::CheckoutStatus;

// B15 — Billing Webhooks:
pub mod billing_webhook_event_type;
pub use billing_webhook_event_type::BillingWebhookEventType;

pub mod notification_outbox_status;
pub use notification_outbox_status::NotificationOutboxStatus;

pub mod notification_outbox_event_kind;
pub use notification_outbox_event_kind::NotificationOutboxEventKind;
