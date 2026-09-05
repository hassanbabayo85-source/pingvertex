#!/usr/bin/env bash
set -euo pipefail

# Worker 4 — PAYG Domain + Metering + Pricing
# Generated from the inspected PingVertex source tree.
# Run from the project root.

echo "[Worker 4] Applying PAYG domain, metering, pricing, security and UI changes..."

mkdir -p "$(dirname 'apps/leptos-dashboard/src/api/mod.rs')"
cat <<'EOF_WORKER4' > 'apps/leptos-dashboard/src/api/mod.rs'
pub mod account;
pub mod advanced_monitoring;
pub mod billing;
pub mod client;
pub mod developer;
pub mod monitoring;
pub mod rum;
pub mod status_pages;
pub mod support;
pub mod team;

pub mod proxy_checks;

pub mod wallet;

EOF_WORKER4

mkdir -p "$(dirname 'apps/leptos-dashboard/src/api/wallet.rs')"
cat <<'EOF_WORKER4' > 'apps/leptos-dashboard/src/api/wallet.rs'
use crate::api::client::{self, ApiError};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Wallet {
    pub wallet_id: String,
    pub currency: String,
    pub balance_units: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WalletTransaction {
    pub id: String,
    pub transaction_type: String,
    pub amount_units: i64,
    pub balance_after_units: i64,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: String,
}

pub async fn get() -> Result<Wallet, ApiError> {
    client::get_json("/wallet").await
}
pub async fn transactions() -> Result<Vec<WalletTransaction>, ApiError> {
    client::get_json("/wallet/transactions").await
}

EOF_WORKER4

mkdir -p "$(dirname 'apps/leptos-dashboard/src/components/navigation.rs')"
cat <<'EOF_WORKER4' > 'apps/leptos-dashboard/src/components/navigation.rs'
//! Navigation model for the authenticated app shell.
//!
//! This module is intentionally data-only: it describes which product areas
//! exist, which route each one points at, and whether that route is
//! registered yet. `components::layout::Layout` renders this data. Keeping
//! the data separate from the render code means Path 10 (or any future
//! integration pass) can add a future navigation item in one place, in
//! `routes/mod.rs`-adjacent order.
//! without touching rendering logic.
//!
//! IMPORTANT: adding an item here does NOT register a route. A route must
//! exist in `routes/mod.rs` (owned by Path 10) before its `NavStatus` is
//! flipped to `Available`, otherwise the link would 404 or fall through to
//! `NotFoundPage`.

/// Whether a nav target is safe to click today.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NavStatus {
    /// The route is registered in `routes/mod.rs` and the page exists.
    Available,
}

#[derive(Clone, Copy)]
pub struct NavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub status: NavStatus,
}

#[derive(Clone, Copy)]
pub struct NavGroup {
    pub label: &'static str,
    pub items: &'static [NavItem],
}

// Single source of truth for the sidebar. Update the `status` field (only)
// once the corresponding route lands in `routes/mod.rs` — do not add new
// hrefs speculatively; every href here must correspond to a page a Path has
// actually delivered or is actively delivering.
pub const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        label: "OVERVIEW",
        items: &[NavItem {
            label: "Dashboard",
            href: "/",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "TOOLS",
        items: &[NavItem {
            label: "Proxy Checker",
            href: "/proxy-check",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "MONITORING",
        items: &[
            NavItem {
                label: "Monitors",
                href: "/monitors",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Incidents",
                href: "/monitors",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Maintenance windows",
                href: "/maintenance",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Escalation policies",
                href: "/escalation-policies",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "USER EXPERIENCE",
        items: &[
            NavItem {
                label: "Real User Monitoring",
                href: "/rum",
                status: NavStatus::Available,
            },
            NavItem {
                label: "RUM sites",
                href: "/rum/sites",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "PUBLIC",
        items: &[NavItem {
            label: "Status pages",
            href: "/status-pages",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "TEAM",
        items: &[
            NavItem {
                label: "Members",
                href: "/team",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Invitations",
                href: "/team/invitations",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "DEVELOPER",
        items: &[
            NavItem {
                label: "API keys",
                href: "/developer/api-keys",
                status: NavStatus::Available,
            },
            NavItem {
                label: "API flows",
                href: "/developer/api-flows",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "BILLING",
        items: &[
            NavItem {
                label: "Overview",
                href: "/billing",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Plan & usage",
                href: "/billing/plan",
                status: NavStatus::Available,
            },
            NavItem {
                label: "PAYG wallet",
                href: "/billing/wallet",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Add-ons",
                href: "/billing/add-ons",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "SUPPORT",
        items: &[
            NavItem {
                label: "Help center",
                href: "/help",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Support tickets",
                href: "/support",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "SETTINGS",
        items: &[
            NavItem {
                label: "Account",
                href: "/settings/account",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Security",
                href: "/settings/security",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Notifications",
                href: "/settings/notifications",
                status: NavStatus::Available,
            },
        ],
    },
];

EOF_WORKER4

mkdir -p "$(dirname 'apps/leptos-dashboard/src/pages/mod.rs')"
cat <<'EOF_WORKER4' > 'apps/leptos-dashboard/src/pages/mod.rs'
pub mod account;
pub mod add_ons;
pub mod api_flows;
pub mod api_keys;
pub mod billing;
pub mod dashboard_overview;
pub mod escalation;
pub mod forgot_password;
pub mod help_center;
pub mod invitations;
pub mod login;
pub mod logout;
pub mod maintenance;
pub mod monitor_detail;
pub mod monitor_form;
pub mod monitors;
pub mod not_found;
pub mod notifications;
pub mod pricing;
pub mod register;
pub mod reset_password;
pub mod rum_dashboard;
pub mod rum_sites;
pub mod security;
pub mod status;
pub mod status_pages;
pub mod support;
pub mod team;
pub mod verify_email;

pub mod proxy_checks;

pub mod wallet;

EOF_WORKER4

mkdir -p "$(dirname 'apps/leptos-dashboard/src/pages/wallet.rs')"
cat <<'EOF_WORKER4' > 'apps/leptos-dashboard/src/pages/wallet.rs'
use crate::api::wallet;
use leptos::prelude::*;

fn micro_usd(units: i64) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let abs = units.unsigned_abs();
    format!("{sign}${}.{:06}", abs / 1_000_000, abs % 1_000_000)
}

#[component]
pub fn WalletPage() -> impl IntoView {
    let wallet = RwSignal::new(None::<wallet::Wallet>);
    let transactions = RwSignal::new(Vec::<wallet::WalletTransaction>::new());
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            match wallet::get().await {
                Ok(value) => wallet.set(Some(value)),
                Err(err) => error.set(Some(err.to_string())),
            }
            match wallet::transactions().await {
                Ok(value) => transactions.set(value),
                Err(err) => error.set(Some(err.to_string())),
            }
        });
    });

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"PAYG"</span>
                    <h1>"Wallet"</h1>
                </div>
            </header>

            {move || error.get().map(|e| view! { <div class="error" role="alert">{e}</div> })}

            <section class="card">
                <div class="section-heading">
                    <div>
                        <h2>"Available balance"</h2>
                        <p class="muted">"PAYG charges are calculated server-side. Prices are never accepted from the browser."</p>
                    </div>
                </div>
                {move || match wallet.get() {
                    Some(w) => view! {
                        <div class="overview-cards">
                            <div class="card">
                                <span class="card-label">"Balance"</span>
                                <span class="card-value">{micro_usd(w.balance_units)}</span>
                            </div>
                            <div class="card">
                                <span class="card-label">"Currency"</span>
                                <span class="card-value">{w.currency}</span>
                            </div>
                        </div>
                    }.into_any(),
                    None => view! { <p class="loading">"Loading wallet…"</p> }.into_any(),
                }}
            </section>

            <section class="card">
                <div class="section-heading">
                    <h2>"Recent transactions"</h2>
                    <span class="muted">"Immutable ledger"</span>
                </div>
                {move || if transactions.get().is_empty() {
                    view! { <p class="muted">"No transactions yet."</p> }.into_any()
                } else {
                    view! {
                        <div class="table-shell">
                            <table class="data-table">
                                <thead><tr><th>"Type"</th><th>"Amount"</th><th>"Balance"</th><th>"Date"</th></tr></thead>
                                <tbody>
                                    {transactions.get().into_iter().map(|t| view! {
                                        <tr>
                                            <td>{t.transaction_type}</td>
                                            <td>{micro_usd(t.amount_units)}</td>
                                            <td>{micro_usd(t.balance_after_units)}</td>
                                            <td>{t.created_at}</td>
                                        </tr>
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="card">
                <h2>"How PAYG works"</h2>
                <p class="muted">"Usage is reserved before a billable operation, captured only after success, and protected against duplicate requests and concurrent overspending."</p>
            </section>
        </div>
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'apps/leptos-dashboard/src/routes/mod.rs')"
cat <<'EOF_WORKER4' > 'apps/leptos-dashboard/src/routes/mod.rs'
use crate::auth::context::provide_auth_context;
use crate::components::require_auth::RequireAuth;
use crate::pages::account::AccountPage;
use crate::pages::add_ons::AddOnsPage;
use crate::pages::api_flows::ApiFlowsPage;
use crate::pages::api_keys::ApiKeysPage;
use crate::pages::billing::BillingPage;
use crate::pages::dashboard_overview::DashboardOverviewPage;
use crate::pages::escalation::EscalationPage;
use crate::pages::forgot_password::ForgotPasswordPage;
use crate::pages::help_center::HelpCenterPage;
use crate::pages::invitations::InvitationsPage;
use crate::pages::login::LoginPage;
use crate::pages::logout::LogoutPage;
use crate::pages::maintenance::MaintenancePage;
use crate::pages::monitor_detail::MonitorDetailPage;
use crate::pages::monitor_form::MonitorFormPage;
use crate::pages::monitors::MonitorsPage;
use crate::pages::not_found::NotFoundPage;
use crate::pages::notifications::NotificationsPage;
use crate::pages::pricing::PricingPage;
use crate::pages::proxy_checks::ProxyChecksPage;
use crate::pages::register::RegisterPage;
use crate::pages::reset_password::ResetPasswordPage;
use crate::pages::rum_dashboard::RumDashboardPage;
use crate::pages::rum_sites::RumSitesPage;
use crate::pages::security::SecurityPage;
use crate::pages::status::{StatusPage, SubdomainStatusPage};
use crate::pages::status_pages::StatusPagesPage;
use crate::pages::support::SupportPage;
use crate::pages::team::TeamPage;
use crate::pages::verify_email::VerifyEmailPage;
use crate::pages::wallet::WalletPage;
use leptos::prelude::*;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    provide_auth_context();

    view! {
        <Router>
            <Routes fallback=|| view! { <NotFoundPage/> }>
                <Route path=path!("/login") view=LoginPage/>
                <Route path=path!("/register") view=RegisterPage/>
                <Route path=path!("/forgot-password") view=ForgotPasswordPage/>
                <Route path=path!("/reset-password") view=ResetPasswordPage/>
                <Route path=path!("/logout") view=LogoutPage/>
                <Route path=path!("/status") view=StatusPage/>
                <Route path=path!("/status/:subdomain") view=SubdomainStatusPage/>
                <Route path=path!("/verify-email") view=VerifyEmailPage/>
                <ParentRoute path=path!("/") view=RequireAuth>
                    <Route path=path!("") view=DashboardOverviewPage/>
                    <Route path=path!("monitors") view=MonitorsPage/>
                    <Route path=path!("monitors/new") view=MonitorFormPage/>
                    <Route path=path!("monitors/:id") view=MonitorDetailPage/>
                    <Route path=path!("monitors/:id/edit") view=MonitorFormPage/>
                    <Route path=path!("proxy-check") view=ProxyChecksPage/>
                    <Route path=path!("proxy-checks") view=ProxyChecksPage/>
                    <Route path=path!("rum") view=RumDashboardPage/>
                    <Route path=path!("rum/sites") view=RumSitesPage/>
                    <Route path=path!("status-pages") view=StatusPagesPage/>
                    <Route path=path!("team") view=TeamPage/>
                    <Route path=path!("team/invitations") view=InvitationsPage/>
                    <Route path=path!("developer/api-keys") view=ApiKeysPage/>
                    <Route path=path!("developer/api-flows") view=ApiFlowsPage/>
                    <Route path=path!("billing") view=BillingPage/>
                    <Route path=path!("billing/wallet") view=WalletPage/>
                    <Route path=path!("billing/plan") view=PricingPage/>
                    <Route path=path!("billing/add-ons") view=AddOnsPage/>
                    <Route path=path!("help") view=HelpCenterPage/>
                    <Route path=path!("support") view=SupportPage/>
                    <Route path=path!("maintenance") view=MaintenancePage/>
                    <Route path=path!("escalation-policies") view=EscalationPage/>
                    <Route path=path!("settings/account") view=AccountPage/>
                    <Route path=path!("settings/security") view=SecurityPage/>
                    <Route path=path!("settings/notifications") view=NotificationsPage/>
                </ParentRoute>
            </Routes>
        </Router>
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/api/src/error.rs')"
cat <<'EOF_WORKER4' > 'crates/api/src/error.rs'
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::error;

#[derive(Debug, Serialize)]
struct ErrorBody {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

/// Unified API error.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Forbidden(String),
    Unauthorized(String),
    TooManyRequests(String),
    PaymentRequired { message: String, code: &'static str },
    Internal(String),
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    /// 403 Forbidden — the caller is authenticated but their role lacks
    /// the permission required for this action (e.g. `Role::Auditor`
    /// calling an endpoint gated on `Permission::ManageApiKeys`).
    /// Distinct from `UNAUTHORIZED`, which means "not authenticated at
    /// all" and is handled upstream by the auth middleware.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    /// 401 Unauthorized — used for auth-flow failures that are NOT about
    /// missing/invalid session tokens (that case is handled upstream by
    /// `jwt_auth` returning a bare `StatusCode`, before an `ApiError` is
    /// ever constructed). This variant covers e.g. an invalid/expired
    /// pending 2FA token, an invalid TOTP/backup code, or a failed
    /// password step-up re-authentication check.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// 429 Too Many Requests — used when a caller-specific quota (not the
    /// IP-based `GovernorLayer` rate limiter, which axum never sees as an
    /// `ApiError`) has been exceeded, e.g. the daily proxy-check job quota.
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::TooManyRequests(message.into())
    }

    /// Generic internal error.
    ///
    /// ONLY use this when `message` is already known to be safe to show a
    /// client (a static, hand-written string with no interpolated
    /// database/driver/library error text). If you are wrapping an
    /// underlying `Error`, use [`ApiError::internal_with_source`] instead
    /// — that constructor logs the real cause and returns a generic
    /// message to the caller.
    pub fn payment_required(message: impl Into<String>, code: &'static str) -> Self {
        Self::PaymentRequired { message: message.into(), code }
    }

    pub fn from_payg_error(err: application::ChargeForUsageError) -> Self {
        match err {
            application::ChargeForUsageError::InsufficientBalance => {
                Self::payment_required("PAYG balance is insufficient", "PAYG_INSUFFICIENT_BALANCE")
            }
            application::ChargeForUsageError::InvalidInput(message) => Self::bad_request(message),
            application::ChargeForUsageError::ReservationInProgress => {
                Self::conflict("PAYG request is already in progress")
            }
            application::ChargeForUsageError::OperationFailed(message) => {
                Self::internal_with_source("metered operation failed", message)
            }
            application::ChargeForUsageError::NoPricing
            | application::ChargeForUsageError::InvalidPricing(_) => {
                Self::internal("PAYG pricing is unavailable")
            }
            application::ChargeForUsageError::Repository(err)
            | application::ChargeForUsageError::CaptureFailed(err) => {
                Self::internal_with_source("PAYG operation failed", err)
            }
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Preferred constructor for internal errors that wrap an underlying
    /// cause (database errors, I/O errors, etc).
    ///
    /// The full error (`err`) is logged server-side via `tracing::error!`
    /// together with `context`, so operators retain complete diagnostic
    /// information. The HTTP response sent to the client contains ONLY
    /// the generic `context` string — never the raw `{err}` — so
    /// internal details such as table/column names or database driver
    /// messages never leak to an untrusted caller. This closes an
    /// information-disclosure gap identified in the 2026-08-11 security
    /// audit, where 500 responses previously echoed raw backend errors.
    pub fn internal_with_source(context: &str, err: impl std::fmt::Display) -> Self {
        error!(error = %err, "{context}");
        Self::Internal(context.to_string())
    }

    /// Maps a use-case error to `403 Forbidden` when it is (or wraps) a
    /// [`domain::shared::types::PermissionDenied`], otherwise falls back to
    /// [`ApiError::internal_with_source`].
    ///
    /// Use cases enforce authorization by calling `AuthenticatedUser::require`,
    /// which returns `PermissionDenied` on a boxed `dyn Error`. Handlers must
    /// route that through here (or an equivalent explicit check) rather than
    /// letting it fall into a generic 500 — a permission failure is a client
    /// error, and treating it as an opaque internal error also makes such
    /// bypasses harder to notice in logs/monitoring.
    pub fn forbidden_or_internal(
        context: &str,
        err: Box<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        if let Some(denied) = err.downcast_ref::<domain::shared::types::PermissionDenied>() {
            return Self::forbidden(denied.to_string());
        }
        Self::internal_with_source(context, err)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::PaymentRequired { .. } => StatusCode::PAYMENT_REQUIRED,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::BadRequest(message)
            | Self::Conflict(message)
            | Self::NotFound(message)
            | Self::Forbidden(message)
            | Self::Unauthorized(message)
            | Self::TooManyRequests(message)
            | Self::Internal(message) => message.clone(),
            Self::PaymentRequired { message, .. } => message.clone(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = match &self {
            ApiError::PaymentRequired { code, .. } => Some(*code),
            _ => None,
        };
        let body = Json(ErrorBody {
            message: self.message(),
            code,
        });

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Misalin "kuskure" mai kama da wanda sqlx/postgres ke iya bayarwa —
    /// yana ƙunshe da sunan table/column, wanda BA A SO ya bayyana ga
    /// client.
    struct FakeDbError(&'static str);

    impl std::fmt::Display for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    #[test]
    fn internal_with_source_never_leaks_raw_error_text_to_client_message() {
        let sensitive =
            FakeDbError("duplicate key value violates unique constraint \"monitors_url_idx\"");

        let err = ApiError::internal_with_source("failed to create monitor", sensitive);

        match &err {
            ApiError::Internal(message) => {
                assert_eq!(message, "failed to create monitor");
                assert!(
                    !message.contains("unique constraint"),
                    "client-facing message must not contain raw DB error text, got: {message}"
                );
                assert!(
                    !message.contains("monitors_url_idx"),
                    "client-facing message must not contain table/column names, got: {message}"
                );
            }
            other => panic!("expected ApiError::Internal, got {other:?}"),
        }
    }

    #[test]
    fn internal_with_source_response_body_is_generic() {
        let sensitive = FakeDbError("relation \"monitors\" does not exist at line 42");

        let err = ApiError::internal_with_source("failed to fetch monitor", sensitive);
        let status = err.status_code();
        let message = err.message();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(message, "failed to fetch monitor");
        assert!(!message.contains("relation"));
        assert!(!message.contains("line 42"));
    }

    #[test]
    fn internal_static_message_is_used_verbatim() {
        let err = ApiError::internal("failed to set cookie");
        assert_eq!(err.message(), "failed to set cookie");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn bad_request_and_not_found_still_carry_their_own_message() {
        assert_eq!(
            ApiError::bad_request("invalid uuid").message(),
            "invalid uuid"
        );
        assert_eq!(
            ApiError::not_found("monitor not found").message(),
            "monitor not found"
        );
        assert_eq!(
            ApiError::bad_request("x").status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::not_found("x").status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn forbidden_carries_its_own_message_and_status() {
        assert_eq!(
            ApiError::forbidden("you do not have permission to do this").message(),
            "you do not have permission to do this"
        );
        assert_eq!(
            ApiError::forbidden("x").status_code(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn payment_required_uses_402_and_application_code() {
        let err = ApiError::payment_required("PAYG balance is insufficient", "PAYG_INSUFFICIENT_BALANCE");
        assert_eq!(err.status_code(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(err.message(), "PAYG balance is insufficient");
    }

    #[test]
    fn too_many_requests_carries_its_own_message() {
        assert_eq!(
            ApiError::too_many_requests("daily limit reached").message(),
            "daily limit reached"
        );
        assert_eq!(
            ApiError::too_many_requests("x").status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/api/src/main.rs')"
cat <<'EOF_WORKER4' > 'crates/api/src/main.rs'
use std::net::SocketAddr;
use std::sync::Arc;

use api::{
    auth_state::{AuthState, NoopPendingAuthCache},
    config::Config,
    metrics::init_metrics,
    mtls::{MtlsPaths, build_mtls_config},
    router::{build_internal_agent_router, build_router},
    state::{AppState, NoopLogoStorage, NoopStatusPageCache},
};
use application::ports::{
    EmailVerificationMailer, IdTokenVerifier, PasswordResetMailer, PaymentProvider, StatusPageCache,
};
use axum_server::tls_rustls::RustlsAcceptor;
use axum_server_mtls::MtlsAcceptor;
use infra::services::MaxMindGeoIpLookup;
use infra::{
    AesGcmProxyCredentialCipher, Argon2PasswordHasher, BillingConfig, DisabledBillingProvider,
    DisabledEmailVerificationMailer, DisabledIdTokenVerifier, DisabledPasswordResetMailer,
    FakeBillingProvider, HickoryCustomDomainVerifier, JsonWebTokenService, NotificationDispatcher,
    OidcIdTokenVerifier, PostgresAddOnSubscriptionRepository, PostgresAgentLeaseRepository,
    PostgresApiFlowRepository, PostgresApiKeyRepository, PostgresAuditLogEntryRepository,
    PostgresBackupCodeRepository, PostgresBillingAuditLogRepository,
    PostgresBillingWebhookIdempotencyRepository, PostgresCheckRepository,
    PostgresCheckoutSessionRepository, PostgresDashboardRepository,
    PostgresDiagnosticReportRepository, PostgresEmailVerificationTokenRepository,
    PostgresEscalationPolicyRepository, PostgresHealthCheck, PostgresHelpArticleRepository,
    PostgresHelpCategoryRepository, PostgresIncidentRepository,
    PostgresMaintenanceWindowRepository, PostgresMonitorRepository,
    PostgresNotificationChannelRepository, PostgresPricingConfigRepository,
    PostgresPAYGPricingRepository, PostgresUsageEventRepository,
    PostgresProxyCheckJobRepository, PostgresRegistrationTransaction,
    PostgresResetPasswordTokenRepository, PostgresRumAnalyticsRepository,
    PostgresRumEventRepository, PostgresRumSiteRepository, PostgresStatusPageRepository,
    PostgresSubscriptionRepository, PostgresSubscriptionUsagePeriodRepository,
    PostgresSubscriptionUsageTransaction, PostgresSupportTicketRepository,
    PostgresTenantInvitationRepository, PostgresTenantMembershipRepository,
    PostgresTenantRepository, PostgresUserRepository, PostgresWalletRepository,
    RedisPendingAuthCache, RedisRateLimiter, RedisStatusPageCache, S3LogoStorage, SmtpConfig,
    SmtpEmailVerificationMailer, SmtpPasswordResetMailer, TwilioPricingService, UuidIdGenerator,
    create_pool,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // P0 hardening: refuse to boot in production if the private-IP SSRF
    // escape hatch is enabled.
    shared::ssrf::validate_production_ssrf_config()?;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=trace"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install rustls default crypto provider");

    init_metrics();

    let config = Config::from_env();

    // Session invalidation (logout/password changes) and TOTP's pending
    // authentication state are security-sensitive. In production they must
    // have a real shared cache; the test/development no-op implementation
    // would otherwise make revocation silently ineffective.
    let environment = std::env::var("ENVIRONMENT").unwrap_or_default();
    if matches!(
        environment.to_ascii_lowercase().as_str(),
        "production" | "prod"
    ) && config.redis_url.is_none()
    {
        return Err(
            "REDIS_URL must be configured in production for session invalidation and TOTP".into(),
        );
    }

    let pool = create_pool(&config.database_url).await?;

    let billing_config = BillingConfig::try_from_env()?;
    let billing_provider: Arc<dyn PaymentProvider> = match billing_config.provider.as_str() {
        "fake" => Arc::new(FakeBillingProvider),
        "disabled" => Arc::new(DisabledBillingProvider),
        other => panic!("unsupported billing provider: {other}"),
    };

    let status_page_cache: Arc<dyn StatusPageCache> = match &config.redis_url {
        Some(redis_url) => Arc::new(RedisStatusPageCache::new(redis_url)?),
        None => {
            tracing::warn!("REDIS_URL not configured — status page responses will not be cached");
            Arc::new(NoopStatusPageCache)
        }
    };

    let proxy_credential_cipher = Arc::new(AesGcmProxyCredentialCipher::try_from_env()?);
    let secret_cipher: Arc<dyn application::ports::SecretCipher> = proxy_credential_cipher.clone();
    let proxy_check_job_repository =
        PostgresProxyCheckJobRepository::new(pool.clone(), proxy_credential_cipher);

    let pending_auth_cache: Arc<dyn application::ports::PendingAuthCache> = match &config.redis_url
    {
        Some(redis_url) => Arc::new(RedisPendingAuthCache::new(redis_url)?),
        None => {
            tracing::warn!(
                "REDIS_URL not configured — TOTP 2FA enrollment/login AND B6 password-reset \
                 session invalidation will not work (pending secrets/tokens and session-\
                 invalidation markers have nowhere to be stored)"
            );
            Arc::new(NoopPendingAuthCache)
        }
    };

    let logo_storage: Arc<dyn application::ports::LogoStorage> = match S3LogoStorage::try_from_env()
    {
        Ok(storage) => Arc::new(storage),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "S3 logo storage not configured; status-page logo uploads will be disabled"
            );
            Arc::new(NoopLogoStorage)
        }
    };

    let audit_log_entry_repository = PostgresAuditLogEntryRepository::new(pool.clone());

    let secret_cipher_for_auth = secret_cipher.clone();

    // Shared distributed rate limiter. Constructed once and cloned into
    // both `AppState` (RUM ingestion) and `AuthState` (auth endpoints)
    // below — `RedisRateLimiter` is `Clone` (cheap: it only wraps a
    // `redis::Client`, which is itself a lightweight connection-info
    // handle, not an open connection) and both states key their own
    // requests under distinct namespaces ("rum_ingest" vs
    // "forgot_password"/"resend_verification"), so sharing one instance
    // is equivalent to constructing two and cannot cross-contaminate
    // either's counters.
    let redis_rate_limiter = RedisRateLimiter::new(
        &std::env::var("REDIS_URL")
            .map_err(|_| anyhow::anyhow!("REDIS_URL is required for rate limiting"))?,
    )?;

    let mut app_state = AppState::new_with_feature_repos(
        PostgresMonitorRepository::new(pool.clone()),
        PostgresCheckRepository::new(pool.clone()),
        PostgresDiagnosticReportRepository::new(pool.clone()),
        PostgresIncidentRepository::new(pool.clone()),
        PostgresNotificationChannelRepository::new(pool.clone()),
        NotificationDispatcher::from_env(secret_cipher.clone()),
        PostgresApiFlowRepository::new(pool.clone()),
        proxy_check_job_repository,
        PostgresMaintenanceWindowRepository::new(pool.clone()),
        PostgresEscalationPolicyRepository::new(pool.clone()),
        PostgresStatusPageRepository::new(pool.clone()),
        HickoryCustomDomainVerifier::new(),
        status_page_cache,
        logo_storage,
        UuidIdGenerator::new(),
        secret_cipher,
        audit_log_entry_repository,
    )
    .with_api_key_repository(PostgresApiKeyRepository::new(pool.clone()))
    .with_subscription_repository(PostgresSubscriptionRepository::new(pool.clone()))
    .with_checkout_session_repository(PostgresCheckoutSessionRepository::new(pool.clone()))
    .with_payment_provider(billing_provider)
    .with_billing_webhook_idempotency_repository(PostgresBillingWebhookIdempotencyRepository::new(
        pool.clone(),
    ))
    .with_billing_audit_log_repository(PostgresBillingAuditLogRepository::new(pool.clone()))
    .with_agent_lease_repository(PostgresAgentLeaseRepository::new(pool.clone()))
    .with_health_check_port(PostgresHealthCheck::new(pool.clone()))
    .with_dashboard_repository(PostgresDashboardRepository::new(pool.clone()))
    .with_pricing_config_repository(PostgresPricingConfigRepository::new(pool.clone()))
    .with_add_on_subscription_repository(PostgresAddOnSubscriptionRepository::new(pool.clone()))
    .with_sms_provider_pricing(TwilioPricingService::new())
    .with_subscription_usage_period_repository(PostgresSubscriptionUsagePeriodRepository::new(
        pool.clone(),
    ))
    .with_subscription_usage_transaction(PostgresSubscriptionUsageTransaction::new(pool.clone()))
    .with_help_category_repository(PostgresHelpCategoryRepository::new(pool.clone()))
    .with_help_article_repository(PostgresHelpArticleRepository::new(pool.clone()))
    .with_support_ticket_repository(PostgresSupportTicketRepository::new(pool.clone()))
    .with_rum_site_repository(PostgresRumSiteRepository::new(pool.clone()))
    .with_rum_event_repository(PostgresRumEventRepository::new(pool.clone()))
    .with_rum_analytics_repository(PostgresRumAnalyticsRepository::new(pool.clone()))
    .with_rate_limiter(redis_rate_limiter.clone())
    .with_wallet_repository(PostgresWalletRepository::new(pool.clone()))
    .with_payg_pricing_repository(PostgresPAYGPricingRepository::new(pool.clone()))
    .with_usage_event_repository(PostgresUsageEventRepository::new(pool.clone()));

    // Geo-IP lookup for RUM sessions is best-effort: reuses the same
    // GeoLite2 database `crates/agent` already requires, but unlike the
    // agent (where a missing DB is a fatal startup error), a missing/
    // unreadable DB here must NOT stop the API server from starting —
    // RUM ingestion is fully functional without it, just with
    // `country_code = None` on every session (see `NoopGeoIpLookup` in
    // `state.rs`). Defaults to the same path `crates/agent` defaults to,
    // so a single `GEOIP_DB_PATH` env var (or the same default file
    // location) covers both processes.
    let geoip_db_path = std::env::var("GEOIP_DB_PATH")
        .unwrap_or_else(|_| "data/geoip/GeoLite2-Country.mmdb".to_string());
    match MaxMindGeoIpLookup::open(&geoip_db_path) {
        Ok(geo_ip_lookup) => {
            app_state = app_state.with_geo_ip_lookup(geo_ip_lookup);
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                path = %geoip_db_path,
                "RUM geo-IP database unavailable; RUM sessions will have no country_code until this is fixed"
            );
        }
    }

    let app_state = match billing_config.webhook_secret {
        Some(secret) => app_state.with_billing_webhook_secret(secret),
        None => app_state,
    };

    let tenant_repository = PostgresTenantRepository::new(pool.clone());
    let tenant_membership_repository = PostgresTenantMembershipRepository::new(pool.clone());
    let registration_transaction = PostgresRegistrationTransaction::new(pool.clone());

    let email_verification_token_repository =
        PostgresEmailVerificationTokenRepository::new(pool.clone());

    let email_verification_mailer: Arc<dyn EmailVerificationMailer> = match SmtpConfig::try_from_env(
    ) {
        Ok(smtp_config) => match SmtpEmailVerificationMailer::new(smtp_config) {
            Ok(mailer) => Arc::new(mailer),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "failed to build verification email mailer; verification emails will be disabled"
                );
                Arc::new(DisabledEmailVerificationMailer::new(err.to_string()))
            }
        },
        Err(err) => {
            tracing::warn!(
                error = %err,
                "SMTP not configured; verification emails will be disabled"
            );
            Arc::new(DisabledEmailVerificationMailer::new(err.to_string()))
        }
    };

    let reset_password_token_repository = PostgresResetPasswordTokenRepository::new(pool.clone());

    let password_reset_mailer: Arc<dyn PasswordResetMailer> = match SmtpConfig::try_from_env() {
        Ok(smtp_config) => match SmtpPasswordResetMailer::new(smtp_config) {
            Ok(mailer) => Arc::new(mailer),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "failed to build password reset mailer; password reset emails will be disabled"
                );
                Arc::new(DisabledPasswordResetMailer::new(err.to_string()))
            }
        },
        Err(err) => {
            tracing::warn!(
                error = %err,
                "SMTP not configured; password reset emails will be disabled"
            );
            Arc::new(DisabledPasswordResetMailer::new(err.to_string()))
        }
    };

    let invitation_repository = PostgresTenantInvitationRepository::new(pool.clone());

    // OAuth — Continue with Google / Apple. Both are optional: a
    // deployment that only wants email/password auth doesn't need to
    // set these. Same "fail loud, not silently disabled" shape as the
    // SMTP mailers above — a missing client ID yields a
    // `DisabledIdTokenVerifier` that clearly refuses every request
    // instead of pretending to work.
    let google_id_token_verifier: Arc<dyn IdTokenVerifier> = match std::env::var("GOOGLE_CLIENT_ID")
    {
        Ok(client_id) if !client_id.trim().is_empty() => {
            Arc::new(OidcIdTokenVerifier::google(client_id))
        }
        _ => {
            tracing::warn!("GOOGLE_CLIENT_ID not set; Google sign-in will be disabled");
            Arc::new(DisabledIdTokenVerifier::new("Google"))
        }
    };

    let apple_id_token_verifier: Arc<dyn IdTokenVerifier> = match std::env::var("APPLE_CLIENT_ID") {
        Ok(client_id) if !client_id.trim().is_empty() => {
            Arc::new(OidcIdTokenVerifier::apple(client_id))
        }
        _ => {
            tracing::warn!("APPLE_CLIENT_ID not set; Apple sign-in will be disabled");
            Arc::new(DisabledIdTokenVerifier::new("Apple"))
        }
    };

    let auth_state = AuthState::new(
        PostgresUserRepository::new(pool.clone()),
        Argon2PasswordHasher,
        JsonWebTokenService::new(config.jwt_secret.clone()),
        UuidIdGenerator::new(),
        tenant_repository,
        tenant_membership_repository,
        registration_transaction,
    )
    .with_totp_support(
        secret_cipher_for_auth,
        pending_auth_cache,
        PostgresBackupCodeRepository::new(pool.clone()),
    )
    .with_rate_limiter(redis_rate_limiter)
    .with_email_verification(
        email_verification_token_repository,
        email_verification_mailer,
    )
    .with_password_reset(reset_password_token_repository, password_reset_mailer)
    .with_invitation_repository(invitation_repository)
    .with_oauth(google_id_token_verifier, apple_id_token_verifier);

    let plain_app = build_router(app_state.clone(), auth_state);
    let internal_agent_app = build_internal_agent_router(app_state);

    let plain_addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let mtls_addr = SocketAddr::from(([0, 0, 0, 0], config.mtls_port));

    let mtls_config = build_mtls_config(MtlsPaths {
        ca_cert: &config.mtls_ca_cert_path,
        server_cert: &config.mtls_server_cert_path,
        server_key: &config.mtls_server_key_path,
    })?;

    tracing::info!("Listening on http://{}", plain_addr);
    tracing::info!(
        "Listening (mTLS, internal agents only) on https://{}",
        mtls_addr
    );

    let plain_listener = tokio::net::TcpListener::bind(plain_addr).await?;

    let plain_server = axum::serve(
        plain_listener,
        plain_app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal());

    let mtls_acceptor = MtlsAcceptor::new(RustlsAcceptor::new(mtls_config));

    let mtls_handle = axum_server::Handle::new();
    let shutdown_handle = mtls_handle.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown signal received, draining mTLS listener");
        shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(25)));
    });

    let mtls_server = axum_server::bind(mtls_addr)
        .acceptor(mtls_acceptor)
        .handle(mtls_handle)
        .serve(internal_agent_app.into_make_service());

    tokio::try_join!(
        async {
            plain_server
                .await
                .map_err(Box::<dyn std::error::Error>::from)
        },
        async {
            mtls_server
                .await
                .map_err(Box::<dyn std::error::Error>::from)
        },
    )?;

    tracing::info!("api shut down cleanly");

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/api/src/state.rs')"
cat <<'EOF_WORKER4' > 'crates/api/src/state.rs'
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use application::ports::{
    AddOnSubscriptionRepository, AgentLeaseRepository, ApiFlowRepository, ApiKeyRepository,
    AuditLogEntryRepository, BillingAuditLogRepository, BillingWebhookIdempotencyRepository,
    CheckRepository, CheckoutDetails, CheckoutRequest, CheckoutSessionRepository,
    CustomDomainVerifier, DashboardRepository, DiagnosticReportRepository,
    EscalationPolicyRepository, GeoIpLookup, HealthCheckPort, HealthCheckResult,
    HelpArticleRepository, HelpCategoryRepository, IncidentRepository, LogoStorage,
    MaintenanceWindowRepository, MonitorRepository, NotificationChannelRepository,
    NotificationService, PasswordHasher, PaymentProvider, PaymentVerification,
    PricingConfigRepository, ProxyCheckJobRepository, RateLimiter, RateLimiterResult,
    RumAnalyticsRepository, RumEventRepository, RumSiteRepository, SecretCipher,
    SmsProviderPricing, StatusPageCache, StatusPageRepository, SubscriptionRepository,
    SubscriptionUsagePeriodRepository, SubscriptionUsageTransaction, SupportTicketRepository,
    PAYGPricingRepository, UsageEventRepository, WalletRepository,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PricingAuditLogEntry;
use domain::Subscription;
use domain::entities::{ApiKey, StatusPage};
use domain::pricing::PricingConfig;
use domain::shared::ids::{ApiKeyId, CheckoutSessionId, MonitorId, StatusPageId, TenantId, UserId};
use domain::{CheckoutSession, EscalationPolicy, MaintenanceWindow};
use infra::{Argon2PasswordHasher, UuidIdGenerator};

use application::dto::DashboardOverview;

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Per-token rate limiter for `GET /public/heartbeat/{token}`. Keyed by the
/// raw token string; value is `(window_started_at, requests_in_window)`.
pub type HeartbeatRateLimiter = Arc<tokio::sync::Mutex<HashMap<String, (Instant, u32)>>>;

fn new_heartbeat_rate_limiter() -> HeartbeatRateLimiter {
    Arc::new(tokio::sync::Mutex::new(HashMap::new()))
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub monitor_repository: Arc<dyn MonitorRepository>,
    pub check_repository: Arc<dyn CheckRepository>,
    pub diagnostic_report_repository: Arc<dyn DiagnosticReportRepository>,
    pub incident_repository: Arc<dyn IncidentRepository>,
    pub notification_channel_repository: Arc<dyn NotificationChannelRepository>,
    pub notification_service: Arc<dyn NotificationService>,
    pub api_flow_repository: Arc<dyn ApiFlowRepository>,
    pub proxy_check_job_repository: Arc<dyn ProxyCheckJobRepository>,
    pub maintenance_window_repository: Arc<dyn MaintenanceWindowRepository>,
    pub escalation_policy_repository: Arc<dyn EscalationPolicyRepository>,
    pub status_page_repository: Arc<dyn StatusPageRepository>,
    pub custom_domain_verifier: Arc<dyn CustomDomainVerifier>,
    pub status_page_cache: Arc<dyn StatusPageCache>,
    pub logo_storage: Arc<dyn LogoStorage>,
    pub secret_cipher: Arc<dyn SecretCipher>,
    pub id_generator: UuidIdGenerator,
    pub heartbeat_rate_limiter: HeartbeatRateLimiter,
    pub audit_log_entry_repository: Arc<dyn AuditLogEntryRepository>,
    pub api_key_repository: Arc<dyn ApiKeyRepository>,
    pub password_hasher: Arc<dyn PasswordHasher>,
    pub subscription_repository: Arc<dyn SubscriptionRepository>,
    pub payment_provider: Arc<dyn PaymentProvider>,
    pub checkout_session_repository: Arc<dyn CheckoutSessionRepository>,
    pub billing_webhook_secret: Option<Arc<str>>,
    pub billing_webhook_idempotency_repository: Arc<dyn BillingWebhookIdempotencyRepository>,
    pub billing_audit_log_repository: Arc<dyn BillingAuditLogRepository>,
    pub pricing_config_repository: Arc<dyn PricingConfigRepository>,
    pub agent_lease_repository: Option<Arc<dyn AgentLeaseRepository>>,
    /// B34: backs `GET /ready`. Defaults to `NoopHealthCheckPort` (always
    /// healthy) until `with_health_check_port` substitutes a real check —
    /// see `main.rs`, which wires `infra::PostgresHealthCheck`.
    pub health_check_port: Arc<dyn HealthCheckPort>,
    /// H6 — Dashboard Overview Aggregated Backend API:
    /// Tenant-scoped, efficient aggregate queries for the dashboard.
    /// Defaults to `NoopDashboardRepository` (returns empty) until
    /// `with_dashboard_repository` substitutes the real Postgres implementation.
    pub dashboard_repository: Arc<dyn DashboardRepository>,
    /// H12 — Proration Service.
    pub subscription_usage_period_repository: Arc<dyn SubscriptionUsagePeriodRepository>,
    /// H12 — Proration Service.
    pub subscription_usage_transaction: Arc<dyn SubscriptionUsageTransaction>,
    pub add_on_subscription_repository: Arc<dyn AddOnSubscriptionRepository>,
    pub sms_provider_pricing: Arc<dyn SmsProviderPricing>,
    pub help_category_repository: Arc<dyn HelpCategoryRepository>,
    pub help_article_repository: Arc<dyn HelpArticleRepository>,
    pub support_ticket_repository: Arc<dyn SupportTicketRepository>,

    // RUM (Real User Monitoring):
    pub rum_site_repository: Arc<dyn RumSiteRepository>,
    pub rum_event_repository: Arc<dyn RumEventRepository>,
    /// Read-only aggregation queries backing the RUM dashboard (Task F).
    /// Defaults to a no-op (all-zero/empty) implementation until
    /// `.with_rum_analytics_repository(...)` wires a real one (e.g.
    /// `infra::PostgresRumAnalyticsRepository`).
    pub rum_analytics_repository: Arc<dyn RumAnalyticsRepository>,
    /// Server-side, country-only geo-IP lookup used by RUM ingestion to
    /// populate `RumSession.country_code` from the connection's IP — the
    /// browser SDK never sends geography (see Task D). Defaults to a
    /// no-op (always `None`) until `.with_geo_ip_lookup(...)` wires a
    /// real one (e.g. `infra::MaxMindGeoIpLookup`).
    pub geo_ip_lookup: Arc<dyn GeoIpLookup>,
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub wallet_repository: Arc<dyn WalletRepository>,
    pub payg_pricing_repository: Arc<dyn PAYGPricingRepository>,
    pub usage_event_repository: Arc<dyn UsageEventRepository>,
}

/// Default `DashboardRepository` used until a real one is substituted in.
#[derive(Clone, Default)]
struct NoopDashboardRepository;

#[async_trait]
impl DashboardRepository for NoopDashboardRepository {
    async fn get_overview(&self, _tenant_id: TenantId) -> RepoResult<DashboardOverview> {
        Ok(DashboardOverview {
            monitor_summary: application::dto::MonitorSummary {
                total: 0,
                up: 0,
                down: 0,
                disabled: 0,
            },
            monitors: Vec::new(),
            incident_summary: application::dto::IncidentSummaryData {
                open_count: 0,
                recent: Vec::new(),
            },
            performance: application::dto::DashboardPerformance {
                uptime_percentage: None,
                avg_latency_ms: None,
                trend: Vec::new(),
            },
        })
    }
}

/// Default `ApiKeyRepository` used until a real one is substituted in.
#[derive(Clone, Default)]
struct NoopApiKeyRepository;

#[async_trait]
impl ApiKeyRepository for NoopApiKeyRepository {
    async fn create(&self, _api_key: &ApiKey) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: ApiKeyId) -> RepoResult<Option<ApiKey>> {
        Ok(None)
    }

    async fn find_by_prefix(&self, _key_prefix: &str) -> RepoResult<Option<ApiKey>> {
        Ok(None)
    }

    async fn list_by_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<ApiKey>> {
        Ok(vec![])
    }

    async fn delete(&self, _id: ApiKeyId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn delete_for_tenant(&self, _id: ApiKeyId, _tenant_id: TenantId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn touch_last_used(&self, _id: ApiKeyId, _at: DateTime<Utc>) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopMaintenanceWindowRepository;

#[async_trait]
impl MaintenanceWindowRepository for NoopMaintenanceWindowRepository {
    async fn create(&self, _window: &MaintenanceWindow) -> RepoResult<()> {
        Ok(())
    }

    async fn list_active_by_monitor(
        &self,
        _monitor_id: MonitorId,
        _now: DateTime<Utc>,
    ) -> RepoResult<Vec<MaintenanceWindow>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
pub struct NoopLogoStorage;

#[async_trait]
impl LogoStorage for NoopLogoStorage {
    async fn store_logo(&self, _logo: &domain::LogoImage) -> RepoResult<String> {
        Err(std::io::Error::other("logo storage is not configured").into())
    }
}

#[derive(Clone, Default)]
struct NoopEscalationPolicyRepository;

#[async_trait]
impl EscalationPolicyRepository for NoopEscalationPolicyRepository {
    async fn create(&self, _policy: &EscalationPolicy) -> RepoResult<()> {
        Ok(())
    }

    async fn update(&self, _policy: &EscalationPolicy) -> RepoResult<()> {
        Ok(())
    }

    async fn find_effective(
        &self,
        _monitor_id: MonitorId,
        _tenant_id: TenantId,
    ) -> RepoResult<Option<EscalationPolicy>> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
pub struct NoopStatusPageRepository;

#[async_trait]
impl StatusPageRepository for NoopStatusPageRepository {
    async fn save(&self, _status_page: &StatusPage) -> RepoResult<()> {
        Ok(())
    }

    async fn update(&self, _status_page: &StatusPage) -> RepoResult<()> {
        Ok(())
    }

    async fn delete_for_user(&self, _id: StatusPageId, _user_id: UserId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn find_by_id_for_user(
        &self,
        _id: StatusPageId,
        _user_id: UserId,
    ) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_all_for_user(&self, _user_id: UserId) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }

    async fn find_by_id(&self, _id: StatusPageId) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn delete(&self, _id: StatusPageId) -> RepoResult<bool> {
        Ok(false)
    }

    async fn find_all_for_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }

    async fn find_by_subdomain(&self, _subdomain: &str) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_by_verified_custom_domain(
        &self,
        _custom_domain: &str,
    ) -> RepoResult<Option<StatusPage>> {
        Ok(None)
    }

    async fn find_pending_custom_domains(&self) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }

    async fn find_all_referencing_monitor(
        &self,
        _monitor_id: MonitorId,
    ) -> RepoResult<Vec<StatusPage>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
pub struct NoopCustomDomainVerifier;

#[async_trait]
impl CustomDomainVerifier for NoopCustomDomainVerifier {
    async fn verify(&self, _domain: &str, _expected_token: &str) -> RepoResult<bool> {
        Ok(false)
    }
}

#[derive(Clone, Default)]
pub struct NoopStatusPageCache;

#[async_trait]
impl StatusPageCache for NoopStatusPageCache {
    async fn get(&self, _key: &str) -> RepoResult<Option<String>> {
        Ok(None)
    }

    async fn set(&self, _key: &str, _value: &str, _ttl_seconds: u64) -> RepoResult<()> {
        Ok(())
    }

    async fn invalidate(&self, _key: &str) -> RepoResult<()> {
        Ok(())
    }
}

/// B34: default `HealthCheckPort` used until `with_health_check_port`
/// substitutes a real one. Always reports healthy — matches the pattern
/// every other Noop* default in this file follows (safe placeholder,
/// never the production path once `main.rs` wires the real dependency).
#[derive(Clone, Default)]
pub struct NoopHealthCheckPort;

#[async_trait]
impl HealthCheckPort for NoopHealthCheckPort {
    async fn check(&self) -> HealthCheckResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionRepository;

#[async_trait]
impl SubscriptionRepository for NoopSubscriptionRepository {
    async fn create(&self, _subscription: &Subscription) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_tenant_id(&self, _tenant_id: TenantId) -> RepoResult<Option<Subscription>> {
        Ok(None)
    }

    async fn update(&self, _subscription: &Subscription) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopPaymentProvider;

#[async_trait]
impl PaymentProvider for NoopPaymentProvider {
    fn provider_name(&self) -> &'static str {
        "noop"
    }

    async fn create_checkout(&self, _request: &CheckoutRequest) -> RepoResult<CheckoutDetails> {
        Err(std::io::Error::other("billing provider is not configured").into())
    }

    async fn verify_payment(&self, _provider_checkout_id: &str) -> RepoResult<PaymentVerification> {
        Err(std::io::Error::other("billing provider is not configured").into())
    }
}

#[derive(Clone, Default)]
struct NoopCheckoutSessionRepository;

#[async_trait]
impl CheckoutSessionRepository for NoopCheckoutSessionRepository {
    async fn create(&self, _session: &CheckoutSession) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: CheckoutSessionId) -> RepoResult<Option<CheckoutSession>> {
        Ok(None)
    }

    async fn update(&self, _session: &CheckoutSession) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopBillingWebhookIdempotencyRepository;

#[async_trait]
impl BillingWebhookIdempotencyRepository for NoopBillingWebhookIdempotencyRepository {
    async fn try_insert(&self, _provider_event_id: &str) -> RepoResult<bool> {
        Ok(true)
    }
}

#[derive(Clone, Default)]
struct NoopBillingAuditLogRepository;

#[async_trait]
impl BillingAuditLogRepository for NoopBillingAuditLogRepository {
    async fn create(&self, _entry: &domain::BillingAuditLogEntry) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopPricingConfigRepository;

#[async_trait]
impl PricingConfigRepository for NoopPricingConfigRepository {
    async fn find_active_config(&self) -> RepoResult<Option<PricingConfig>> {
        Ok(None)
    }

    async fn find_by_version(&self, _version: u32) -> RepoResult<Option<PricingConfig>> {
        Ok(None)
    }

    async fn create_version(
        &self,
        _config: &PricingConfig,
        _reason: Option<String>,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn record_audit_log(&self, _entry: &PricingAuditLogEntry) -> RepoResult<()> {
        Ok(())
    }

    async fn list_versions(&self) -> RepoResult<Vec<PricingConfig>> {
        Ok(vec![])
    }

    async fn list_audit_logs(&self) -> RepoResult<Vec<PricingAuditLogEntry>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionUsagePeriodRepository;

#[async_trait]
impl SubscriptionUsagePeriodRepository for NoopSubscriptionUsagePeriodRepository {
    async fn create_period(
        &self,
        _period: &domain::entities::SubscriptionUsagePeriod,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn close_period(
        &self,
        _period_id: domain::shared::ids::SubscriptionUsagePeriodId,
        _end_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        Ok(())
    }

    async fn find_open_period(
        &self,
        _subscription_id: domain::shared::ids::SubscriptionId,
    ) -> RepoResult<Option<domain::entities::SubscriptionUsagePeriod>> {
        Ok(None)
    }

    async fn list_periods_for_cycle(
        &self,
        _subscription_id: domain::shared::ids::SubscriptionId,
        _cycle_start: DateTime<Utc>,
        _cycle_end: DateTime<Utc>,
    ) -> RepoResult<Vec<domain::entities::SubscriptionUsagePeriod>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSubscriptionUsageTransaction;

#[async_trait]
impl SubscriptionUsageTransaction for NoopSubscriptionUsageTransaction {
    async fn change_usage(
        &self,
        _subscription: &Subscription,
        _previous_open_period_id: Option<domain::shared::ids::SubscriptionUsagePeriodId>,
        _new_period: &domain::entities::SubscriptionUsagePeriod,
    ) -> RepoResult<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct NoopAddOnSubscriptionRepository;
#[async_trait]
impl AddOnSubscriptionRepository for NoopAddOnSubscriptionRepository {
    async fn create(&self, _: &domain::AddOnSubscription) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::AddOnSubscriptionId,
    ) -> RepoResult<Option<domain::AddOnSubscription>> {
        Ok(None)
    }
    async fn list_active_by_tenant(
        &self,
        _: TenantId,
    ) -> RepoResult<Vec<domain::AddOnSubscription>> {
        Ok(vec![])
    }
    async fn list_by_tenant_and_type(
        &self,
        _: TenantId,
        _: domain::AddOnType,
    ) -> RepoResult<Vec<domain::AddOnSubscription>> {
        Ok(vec![])
    }
}
#[derive(Clone, Default)]
struct NoopSmsProviderPricing;

#[derive(Clone, Default)]
struct NoopHelpCategoryRepository;

#[async_trait]
impl HelpCategoryRepository for NoopHelpCategoryRepository {
    async fn create(&self, _: &domain::entities::HelpCategory) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_id(
        &self,
        _: domain::shared::ids::HelpCategoryId,
    ) -> RepoResult<Option<domain::entities::HelpCategory>> {
        Ok(None)
    }
    async fn find_by_slug(&self, _: &str) -> RepoResult<Option<domain::entities::HelpCategory>> {
        Ok(None)
    }
    async fn list_all(&self) -> RepoResult<Vec<domain::entities::HelpCategory>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopHelpArticleRepository;

#[async_trait]
impl HelpArticleRepository for NoopHelpArticleRepository {
    async fn create(&self, _: &domain::entities::HelpArticle) -> RepoResult<()> {
        Ok(())
    }
    async fn find_by_slug(&self, _: &str) -> RepoResult<Option<domain::entities::HelpArticle>> {
        Ok(None)
    }
    async fn list_by_category(
        &self,
        _: domain::shared::ids::HelpCategoryId,
        _: bool,
    ) -> RepoResult<Vec<domain::entities::HelpArticle>> {
        Ok(vec![])
    }
    async fn list_published(&self) -> RepoResult<Vec<domain::entities::HelpArticle>> {
        Ok(vec![])
    }
    async fn search(&self, _: &str) -> RepoResult<Vec<domain::entities::HelpArticle>> {
        Ok(vec![])
    }
}

#[derive(Clone, Default)]
struct NoopSupportTicketRepository;

#[async_trait]
impl SupportTicketRepository for NoopSupportTicketRepository {
    async fn create_ticket(&self, _: &domain::entities::SupportTicket) -> RepoResult<()> {
        Ok(())
    }
    async fn find_ticket(
        &self,
        _: domain::shared::ids::SupportTicketId,
    ) -> RepoResult<Option<domain::entities::SupportTicket>> {
        Ok(None)
    }
    async fn list_user_tickets(
        &self,
        _: domain::shared::ids::TenantId,
        _: domain::shared::ids::UserId,
    ) -> RepoResult<Vec<domain::entities::SupportTicket>> {
        Ok(vec![])
    }
    async fn list_tenant_tickets(
        &self,
        _: domain::shared::ids::TenantId,
    ) -> RepoResult<Vec<domain::entities::SupportTicket>> {
        Ok(vec![])
    }
    async fn create_message(&self, _: &domain::entities::SupportMessage) -> RepoResult<()> {
        Ok(())
    }
    async fn list_messages(
        &self,
        _: domain::shared::ids::SupportTicketId,
    ) -> RepoResult<Vec<domain::entities::SupportMessage>> {
        Ok(vec![])
    }
    async fn update_ticket_status(
        &self,
        _: domain::shared::ids::SupportTicketId,
        _: domain::enums::SupportTicketStatus,
    ) -> RepoResult<()> {
        Ok(())
    }
}
#[async_trait]
impl SmsProviderPricing for NoopSmsProviderPricing {
    async fn outbound_sms_unit_price(
        &self,
        _: &str,
    ) -> Result<(rust_decimal::Decimal, String), Box<dyn std::error::Error + Send + Sync>> {
        Err(std::io::Error::other("SMS provider pricing is not configured").into())
    }
}

/// Default RUM site repository until a real one (e.g.
/// `infra::PostgresRumSiteRepository`) is wired via
/// `AppState::with_rum_site_repository`. Always reports no sites found —
/// safe because `IngestRumEventUseCase` treats "unknown site" as a plain
/// rejection, never a panic or a bypass.
#[derive(Clone, Default)]
struct NoopRumSiteRepository;

#[async_trait]
impl RumSiteRepository for NoopRumSiteRepository {
    async fn create(&self, _site: &domain::RumSite) -> RepoResult<()> {
        Ok(())
    }

    async fn find_by_site_key(&self, _site_key: &str) -> RepoResult<Option<domain::RumSite>> {
        Ok(None)
    }

    async fn find_by_id(
        &self,
        _id: domain::shared::ids::RumSiteId,
        _tenant_id: TenantId,
    ) -> RepoResult<Option<domain::RumSite>> {
        Ok(None)
    }

    async fn list_by_tenant(&self, _tenant_id: TenantId) -> RepoResult<Vec<domain::RumSite>> {
        Ok(vec![])
    }
}

/// Default RUM event repository until a real one (e.g.
/// `infra::PostgresRumEventRepository`) is wired via
/// `AppState::with_rum_event_repository`. Writes are silently accepted
/// and discarded — acceptable only because, paired with
/// `NoopRumSiteRepository` above, `find_by_site_key` always returns
/// `None`, so `IngestRumEventUseCase` never actually reaches these calls
/// in the unconfigured default state.
#[derive(Clone, Default)]
struct NoopRumEventRepository;

#[async_trait]
impl RumEventRepository for NoopRumEventRepository {
    async fn upsert_session(&self, session: &domain::RumSession) -> RepoResult<domain::RumSession> {
        Ok(session.clone())
    }

    async fn insert_page_view(&self, _page_view: &domain::RumPageView) -> RepoResult<()> {
        Ok(())
    }

    async fn insert_js_error(&self, _error: &domain::RumJsError) -> RepoResult<()> {
        Ok(())
    }

    async fn insert_resource_error(&self, _error: &domain::RumResourceError) -> RepoResult<()> {
        Ok(())
    }
}

/// Default rate limiter until a real distributed one (e.g.
/// `infra::RedisRateLimiter`) is wired via `AppState::with_rate_limiter`.
/// Allows every request. Intended only for tests/dev; production wiring
/// in `main.rs` MUST call `.with_rate_limiter(...)` before serving
/// traffic, since RUM ingestion has no other request-volume control of
/// its own besides the IP-based governor layer on the public router.
#[derive(Clone, Default)]
struct NoopRateLimiter;

#[async_trait]
impl RateLimiter for NoopRateLimiter {
    async fn check_and_record(
        &self,
        _namespace: &str,
        _key: &str,
        _max_requests: u32,
        _window_seconds: u64,
    ) -> RateLimiterResult<bool> {
        Ok(true)
    }
}

/// Default RUM analytics repository until a real one (e.g.
/// `infra::PostgresRumAnalyticsRepository`) is wired via
/// `AppState::with_rum_analytics_repository`. Returns all-zero/empty
/// results rather than an error, so an unconfigured dashboard shows
/// "no data yet" instead of a 500.
#[derive(Clone, Default)]
struct NoopRumAnalyticsRepository;

#[async_trait]
impl RumAnalyticsRepository for NoopRumAnalyticsRepository {
    async fn overview(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
    ) -> RepoResult<application::dto::RumOverview> {
        Ok(application::dto::RumOverview {
            page_view_count: 0,
            session_count: 0,
            avg_load_time_ms: None,
            p95_load_time_ms: None,
            p75_lcp_ms: None,
            p75_inp_ms: None,
            p75_cls: None,
            js_error_count: 0,
            resource_error_count: 0,
            error_rate: None,
        })
    }

    async fn slow_pages(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
        _limit: i64,
    ) -> RepoResult<Vec<application::dto::PagePerformanceRow>> {
        Ok(vec![])
    }

    async fn error_breakdown(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
        _top_n: i64,
    ) -> RepoResult<application::dto::RumErrorBreakdown> {
        Ok(application::dto::RumErrorBreakdown {
            js_error_count: 0,
            resource_error_count: 0,
            page_view_count: 0,
            error_rate: None,
            top_js_errors: vec![],
            top_resource_errors: vec![],
        })
    }

    async fn browser_device_breakdown(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
    ) -> RepoResult<application::dto::BrowserDeviceBreakdown> {
        Ok(application::dto::BrowserDeviceBreakdown {
            browsers: vec![],
            devices: vec![],
        })
    }

    async fn geo_breakdown(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
        _limit: i64,
    ) -> RepoResult<Vec<application::dto::GeoBreakdownRow>> {
        Ok(vec![])
    }

    async fn performance_trend(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
    ) -> RepoResult<Vec<application::dto::PerformanceTrendPoint>> {
        Ok(vec![])
    }

    async fn session_stats(
        &self,
        _tenant_id: TenantId,
        _site_id: domain::shared::ids::RumSiteId,
        _range: application::dto::RumTimeRange,
    ) -> RepoResult<application::dto::SessionStats> {
        Ok(application::dto::SessionStats {
            session_count: 0,
            avg_page_views_per_session: None,
            avg_session_duration_secs: None,
        })
    }
}

/// Default geo-IP lookup until a real one (e.g.
/// `infra::MaxMindGeoIpLookup`) is wired via
/// `AppState::with_geo_ip_lookup`. Always reports "unknown" — RUM
/// ingestion must work identically whether or not geo-IP is configured,
/// just with `country_code = None` on every session until it is.
#[derive(Clone, Default)]
struct NoopGeoIpLookup;

impl GeoIpLookup for NoopGeoIpLookup {
    fn lookup_country_code(&self, _ip: std::net::IpAddr) -> Option<String> {
        None
    }
}

#[derive(Clone, Default)]
struct NoopWalletRepository;

#[async_trait]
impl WalletRepository for NoopWalletRepository {
    async fn get_or_create(
        &self,
        tenant_id: uuid::Uuid,
        currency: &str,
    ) -> RepoResult<domain::Wallet> {
        Ok(domain::Wallet::new(
            uuid::Uuid::new_v4(),
            tenant_id,
            currency,
        ))
    }
    async fn list_transactions(
        &self,
        _tenant_id: uuid::Uuid,
        _limit: u32,
    ) -> RepoResult<Vec<domain::WalletTransaction>> {
        Ok(vec![])
    }
    async fn reserve(
        &self,
        _tenant_id: uuid::Uuid,
        _amount_units: i64,
        _operation: &str,
        _request_id: &str,
        _expires_at: DateTime<Utc>,
    ) -> RepoResult<application::ports::ReservationOutcome> {
        Err("wallet repository is not configured".into())
    }
    async fn capture(&self, _reservation_id: uuid::Uuid) -> RepoResult<domain::WalletTransaction> {
        Err("wallet repository is not configured".into())
    }
    async fn release(&self, _reservation_id: uuid::Uuid) -> RepoResult<()> {
        Err("wallet repository is not configured".into())
    }
    async fn credit(
        &self,
        _tenant_id: uuid::Uuid,
        _amount_units: i64,
        _transaction_type: domain::WalletTransactionType,
        _idempotency_key: &str,
        _description: Option<&str>,
    ) -> RepoResult<domain::WalletTransaction> {
        Err("wallet repository is not configured".into())
    }
}


#[derive(Clone, Default)]
struct NoopPAYGPricingRepository;
#[async_trait]
impl PAYGPricingRepository for NoopPAYGPricingRepository {
    async fn find_active(&self, _operation: &str) -> RepoResult<Option<domain::PAYGPricing>> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
struct NoopUsageEventRepository;
#[async_trait]
impl UsageEventRepository for NoopUsageEventRepository {
    async fn find_by_request(
        &self, _tenant_id: uuid::Uuid, _request_id: &str,
    ) -> RepoResult<Option<domain::UsageEvent>> {
        Ok(None)
    }
    async fn create(&self, event: &domain::UsageEvent) -> RepoResult<domain::UsageEvent> {
        Ok(event.clone())
    }
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new<M, C, D, I, N, S, A, P, AL>(
        monitor_repository: M,
        check_repository: C,
        diagnostic_report_repository: D,
        incident_repository: I,
        notification_channel_repository: N,
        notification_service: S,
        api_flow_repository: A,
        proxy_check_job_repository: P,
        id_generator: UuidIdGenerator,
        secret_cipher: Arc<dyn SecretCipher>,
        audit_log_entry_repository: AL,
    ) -> Self
    where
        M: MonitorRepository + 'static,
        C: CheckRepository + 'static,
        D: DiagnosticReportRepository + 'static,
        I: IncidentRepository + 'static,
        N: NotificationChannelRepository + 'static,
        S: NotificationService + 'static,
        A: ApiFlowRepository + 'static,
        P: ProxyCheckJobRepository + 'static,
        AL: AuditLogEntryRepository + 'static,
    {
        Self {
            monitor_repository: Arc::new(monitor_repository),
            check_repository: Arc::new(check_repository),
            diagnostic_report_repository: Arc::new(diagnostic_report_repository),
            incident_repository: Arc::new(incident_repository),
            notification_channel_repository: Arc::new(notification_channel_repository),
            notification_service: Arc::new(notification_service),
            api_flow_repository: Arc::new(api_flow_repository),
            proxy_check_job_repository: Arc::new(proxy_check_job_repository),
            maintenance_window_repository: Arc::new(NoopMaintenanceWindowRepository),
            escalation_policy_repository: Arc::new(NoopEscalationPolicyRepository),
            status_page_repository: Arc::new(NoopStatusPageRepository),
            custom_domain_verifier: Arc::new(NoopCustomDomainVerifier),
            status_page_cache: Arc::new(NoopStatusPageCache),
            logo_storage: Arc::new(NoopLogoStorage),
            secret_cipher,
            id_generator,
            heartbeat_rate_limiter: new_heartbeat_rate_limiter(),
            audit_log_entry_repository: Arc::new(audit_log_entry_repository),
            api_key_repository: Arc::new(NoopApiKeyRepository),
            password_hasher: Arc::new(Argon2PasswordHasher),
            subscription_repository: Arc::new(NoopSubscriptionRepository),
            payment_provider: Arc::new(NoopPaymentProvider),
            checkout_session_repository: Arc::new(NoopCheckoutSessionRepository),
            billing_webhook_secret: None,
            billing_webhook_idempotency_repository: Arc::new(
                NoopBillingWebhookIdempotencyRepository,
            ),
            billing_audit_log_repository: Arc::new(NoopBillingAuditLogRepository),
            pricing_config_repository: Arc::new(NoopPricingConfigRepository),
            agent_lease_repository: None,
            health_check_port: Arc::new(NoopHealthCheckPort),
            dashboard_repository: Arc::new(NoopDashboardRepository),
            subscription_usage_period_repository: Arc::new(NoopSubscriptionUsagePeriodRepository),
            subscription_usage_transaction: Arc::new(NoopSubscriptionUsageTransaction),
            add_on_subscription_repository: Arc::new(NoopAddOnSubscriptionRepository),
            sms_provider_pricing: Arc::new(NoopSmsProviderPricing),
            help_category_repository: Arc::new(NoopHelpCategoryRepository),
            help_article_repository: Arc::new(NoopHelpArticleRepository),
            support_ticket_repository: Arc::new(NoopSupportTicketRepository),
            rum_site_repository: Arc::new(NoopRumSiteRepository),
            rum_event_repository: Arc::new(NoopRumEventRepository),
            rum_analytics_repository: Arc::new(NoopRumAnalyticsRepository),
            geo_ip_lookup: Arc::new(NoopGeoIpLookup),
            rate_limiter: Arc::new(NoopRateLimiter),
            wallet_repository: Arc::new(NoopWalletRepository),
            payg_pricing_repository: Arc::new(NoopPAYGPricingRepository),
            usage_event_repository: Arc::new(NoopUsageEventRepository),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_feature_repos<M, C, D, I, N, S, A, P, MW, EP, SP, CDV, AL>(
        monitor_repository: M,
        check_repository: C,
        diagnostic_report_repository: D,
        incident_repository: I,
        notification_channel_repository: N,
        notification_service: S,
        api_flow_repository: A,
        proxy_check_job_repository: P,
        maintenance_window_repository: MW,
        escalation_policy_repository: EP,
        status_page_repository: SP,
        custom_domain_verifier: CDV,
        status_page_cache: Arc<dyn StatusPageCache>,
        logo_storage: Arc<dyn LogoStorage>,
        id_generator: UuidIdGenerator,
        secret_cipher: Arc<dyn SecretCipher>,
        audit_log_entry_repository: AL,
    ) -> Self
    where
        M: MonitorRepository + 'static,
        C: CheckRepository + 'static,
        D: DiagnosticReportRepository + 'static,
        I: IncidentRepository + 'static,
        N: NotificationChannelRepository + 'static,
        S: NotificationService + 'static,
        A: ApiFlowRepository + 'static,
        P: ProxyCheckJobRepository + 'static,
        MW: MaintenanceWindowRepository + 'static,
        EP: EscalationPolicyRepository + 'static,
        SP: StatusPageRepository + 'static,
        CDV: CustomDomainVerifier + 'static,
        AL: AuditLogEntryRepository + 'static,
    {
        Self {
            monitor_repository: Arc::new(monitor_repository),
            check_repository: Arc::new(check_repository),
            diagnostic_report_repository: Arc::new(diagnostic_report_repository),
            incident_repository: Arc::new(incident_repository),
            notification_channel_repository: Arc::new(notification_channel_repository),
            notification_service: Arc::new(notification_service),
            api_flow_repository: Arc::new(api_flow_repository),
            proxy_check_job_repository: Arc::new(proxy_check_job_repository),
            maintenance_window_repository: Arc::new(maintenance_window_repository),
            escalation_policy_repository: Arc::new(escalation_policy_repository),
            status_page_repository: Arc::new(status_page_repository),
            custom_domain_verifier: Arc::new(custom_domain_verifier),
            status_page_cache,
            logo_storage,
            secret_cipher,
            id_generator,
            heartbeat_rate_limiter: new_heartbeat_rate_limiter(),
            audit_log_entry_repository: Arc::new(audit_log_entry_repository),
            api_key_repository: Arc::new(NoopApiKeyRepository),
            password_hasher: Arc::new(Argon2PasswordHasher),
            subscription_repository: Arc::new(NoopSubscriptionRepository),
            payment_provider: Arc::new(NoopPaymentProvider),
            checkout_session_repository: Arc::new(NoopCheckoutSessionRepository),
            billing_webhook_secret: None,
            billing_webhook_idempotency_repository: Arc::new(
                NoopBillingWebhookIdempotencyRepository,
            ),
            billing_audit_log_repository: Arc::new(NoopBillingAuditLogRepository),
            pricing_config_repository: Arc::new(NoopPricingConfigRepository),
            agent_lease_repository: None,
            health_check_port: Arc::new(NoopHealthCheckPort),
            dashboard_repository: Arc::new(NoopDashboardRepository),
            subscription_usage_period_repository: Arc::new(NoopSubscriptionUsagePeriodRepository),
            subscription_usage_transaction: Arc::new(NoopSubscriptionUsageTransaction),
            add_on_subscription_repository: Arc::new(NoopAddOnSubscriptionRepository),
            sms_provider_pricing: Arc::new(NoopSmsProviderPricing),
            help_category_repository: Arc::new(NoopHelpCategoryRepository),
            help_article_repository: Arc::new(NoopHelpArticleRepository),
            support_ticket_repository: Arc::new(NoopSupportTicketRepository),
            rum_site_repository: Arc::new(NoopRumSiteRepository),
            rum_event_repository: Arc::new(NoopRumEventRepository),
            rum_analytics_repository: Arc::new(NoopRumAnalyticsRepository),
            geo_ip_lookup: Arc::new(NoopGeoIpLookup),
            rate_limiter: Arc::new(NoopRateLimiter),
            wallet_repository: Arc::new(NoopWalletRepository),
            payg_pricing_repository: Arc::new(NoopPAYGPricingRepository),
            usage_event_repository: Arc::new(NoopUsageEventRepository),
        }
    }

    pub fn with_api_key_repository<R>(mut self, api_key_repository: R) -> Self
    where
        R: ApiKeyRepository + 'static,
    {
        self.api_key_repository = Arc::new(api_key_repository);
        self
    }

    /// Substitutes a real `StatusPageRepository` in place of the
    /// `NoopStatusPageRepository` default. Mirrors `with_api_key_repository`.
    pub fn with_status_page_repository<R>(mut self, status_page_repository: R) -> Self
    where
        R: StatusPageRepository + 'static,
    {
        self.status_page_repository = Arc::new(status_page_repository);
        self
    }

    pub fn with_subscription_repository<R>(mut self, subscription_repository: R) -> Self
    where
        R: SubscriptionRepository + 'static,
    {
        self.subscription_repository = Arc::new(subscription_repository);
        self
    }

    pub fn with_payment_provider<P>(mut self, payment_provider: P) -> Self
    where
        P: PaymentProvider + 'static,
    {
        self.payment_provider = Arc::new(payment_provider);
        self
    }

    pub fn with_checkout_session_repository<R>(mut self, checkout_session_repository: R) -> Self
    where
        R: CheckoutSessionRepository + 'static,
    {
        self.checkout_session_repository = Arc::new(checkout_session_repository);
        self
    }

    pub fn with_billing_webhook_secret<S: Into<String>>(mut self, secret: S) -> Self {
        self.billing_webhook_secret = Some(Arc::from(secret.into()));
        self
    }

    pub fn with_billing_webhook_idempotency_repository<R>(mut self, repo: R) -> Self
    where
        R: BillingWebhookIdempotencyRepository + 'static,
    {
        self.billing_webhook_idempotency_repository = Arc::new(repo);
        self
    }

    pub fn with_billing_audit_log_repository<R>(mut self, repo: R) -> Self
    where
        R: BillingAuditLogRepository + 'static,
    {
        self.billing_audit_log_repository = Arc::new(repo);
        self
    }

    pub fn with_agent_lease_repository<R>(mut self, agent_lease_repository: R) -> Self
    where
        R: AgentLeaseRepository + 'static,
    {
        self.agent_lease_repository = Some(Arc::new(agent_lease_repository));
        self
    }

    /// B34: substitutes a real `HealthCheckPort` (see
    /// `infra::PostgresHealthCheck`) in place of the always-healthy
    /// `NoopHealthCheckPort` default, backing `GET /ready`.
    pub fn with_health_check_port<H>(mut self, health_check_port: H) -> Self
    where
        H: HealthCheckPort + 'static,
    {
        self.health_check_port = Arc::new(health_check_port);
        self
    }

    /// H6 — Dashboard Overview:
    /// Substitutes a real `DashboardRepository` (e.g. `PostgresDashboardRepository`)
    /// in place of the no-op default.
    pub fn with_pricing_config_repository<R>(mut self, pricing_config_repository: R) -> Self
    where
        R: PricingConfigRepository + 'static,
    {
        self.pricing_config_repository = Arc::new(pricing_config_repository);
        self
    }

    pub fn with_dashboard_repository<R>(mut self, dashboard_repository: R) -> Self
    where
        R: DashboardRepository + 'static,
    {
        self.dashboard_repository = Arc::new(dashboard_repository);
        self
    }

    /// H12 — Proration Service.
    pub fn with_subscription_usage_period_repository<R>(mut self, repo: R) -> Self
    where
        R: SubscriptionUsagePeriodRepository + 'static,
    {
        self.subscription_usage_period_repository = Arc::new(repo);
        self
    }

    /// H12 — Proration Service.
    pub fn with_add_on_subscription_repository<R>(mut self, repo: R) -> Self
    where
        R: AddOnSubscriptionRepository + 'static,
    {
        self.add_on_subscription_repository = Arc::new(repo);
        self
    }

    pub fn with_sms_provider_pricing<P>(mut self, provider: P) -> Self
    where
        P: SmsProviderPricing + 'static,
    {
        self.sms_provider_pricing = Arc::new(provider);
        self
    }

    pub fn with_subscription_usage_transaction<T>(mut self, transaction: T) -> Self
    where
        T: SubscriptionUsageTransaction + 'static,
    {
        self.subscription_usage_transaction = Arc::new(transaction);
        self
    }

    pub fn with_help_category_repository<R>(mut self, repo: R) -> Self
    where
        R: HelpCategoryRepository + 'static,
    {
        self.help_category_repository = Arc::new(repo);
        self
    }

    pub fn with_help_article_repository<R>(mut self, repo: R) -> Self
    where
        R: HelpArticleRepository + 'static,
    {
        self.help_article_repository = Arc::new(repo);
        self
    }

    pub fn with_support_ticket_repository<R>(mut self, repo: R) -> Self
    where
        R: SupportTicketRepository + 'static,
    {
        self.support_ticket_repository = Arc::new(repo);
        self
    }

    // RUM (Real User Monitoring):
    pub fn with_rum_site_repository<R>(mut self, repo: R) -> Self
    where
        R: RumSiteRepository + 'static,
    {
        self.rum_site_repository = Arc::new(repo);
        self
    }

    pub fn with_rum_event_repository<R>(mut self, repo: R) -> Self
    where
        R: RumEventRepository + 'static,
    {
        self.rum_event_repository = Arc::new(repo);
        self
    }

    pub fn with_rate_limiter<R>(mut self, rate_limiter: R) -> Self
    where
        R: RateLimiter + 'static,
    {
        self.rate_limiter = Arc::new(rate_limiter);
        self
    }

    pub fn with_wallet_repository<R>(mut self, repository: R) -> Self
    where
        R: WalletRepository + 'static,
    {
        self.wallet_repository = Arc::new(repository);
        self
    }

    pub fn with_payg_pricing_repository<R>(mut self, repository: R) -> Self
    where
        R: PAYGPricingRepository + 'static,
    {
        self.payg_pricing_repository = Arc::new(repository);
        self
    }

    pub fn with_usage_event_repository<R>(mut self, repository: R) -> Self
    where
        R: UsageEventRepository + 'static,
    {
        self.usage_event_repository = Arc::new(repository);
        self
    }

    pub fn with_rum_analytics_repository<R>(mut self, repo: R) -> Self
    where
        R: RumAnalyticsRepository + 'static,
    {
        self.rum_analytics_repository = Arc::new(repo);
        self
    }

    pub fn with_geo_ip_lookup<G>(mut self, geo_ip_lookup: G) -> Self
    where
        G: GeoIpLookup + 'static,
    {
        self.geo_ip_lookup = Arc::new(geo_ip_lookup);
        self
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/dto/mod.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/dto/mod.rs'
pub mod api_flow_step_input;
pub mod assertion_input;
pub mod auth_response;
pub mod change_password_request;
pub mod create_api_flow_request;
pub mod create_monitor_request;
pub mod get_monitor_response;
pub mod list_monitors_response;
pub mod login_request;
pub mod register_request;
pub mod update_monitor_request;
pub mod variable_extraction_input;
pub use api_flow_step_input::ApiFlowStepInput;
pub use assertion_input::AssertionInput;
pub use auth_response::AuthResponse;
pub use change_password_request::ChangePasswordRequest;
pub use create_api_flow_request::CreateApiFlowRequest;
pub use create_monitor_request::CreateMonitorRequest;
pub use get_monitor_response::GetMonitorResponse;
pub use list_monitors_response::ListMonitorsResponse;
pub use login_request::LoginRequest;
pub use register_request::RegisterRequest;
pub use update_monitor_request::UpdateMonitorRequest;
pub use variable_extraction_input::{ExtractSourceInput, VariableExtractionInput};
pub mod proxy_entry_input;
pub use proxy_entry_input::ProxyEntryInput;
pub mod create_proxy_check_job_request;
pub use create_proxy_check_job_request::CreateProxyCheckJobRequest;
pub mod check_with_diagnostic_report;
pub use check_with_diagnostic_report::CheckWithDiagnosticReport;
pub mod create_status_page_request;
pub use create_status_page_request::CreateStatusPageRequest;

pub mod pending_two_factor_response;
pub use pending_two_factor_response::{LoginOutcome, PendingTwoFactorResponse};
pub mod enable_totp_response;
pub use enable_totp_response::EnableTotpResponse;
pub mod confirm_totp_setup_request;
pub use confirm_totp_setup_request::{ConfirmTotpSetupRequest, ConfirmTotpSetupResponse};
pub mod verify_totp_request;
pub use verify_totp_request::VerifyTotpRequest;
pub mod disable_totp_request;
pub use disable_totp_request::DisableTotpRequest;

pub mod resend_verification_request;
pub mod verify_email_request;
pub use resend_verification_request::ResendVerificationRequest;
pub use verify_email_request::VerifyEmailRequest;

pub mod forgot_password_request;
pub mod reset_password_request;
pub use forgot_password_request::ForgotPasswordRequest;
pub use reset_password_request::ResetPasswordRequest;

// OAuth — Continue with Google / Apple:
pub mod oauth_login_request;
pub use oauth_login_request::OAuthLoginRequest;

// H6 — Dashboard Overview Aggregated Backend API:
pub mod dashboard_overview;
pub use dashboard_overview::{
    DashboardOverview, DashboardPerformance, DashboardTrendPoint, IncidentSummary,
    IncidentSummaryData, MonitorOverviewItem, MonitorSummary,
};

// RUM (Real User Monitoring):
pub mod rum_analytics;
pub mod rum_ingest_request;
pub use rum_analytics::{
    BrowserBreakdownRow, BrowserDeviceBreakdown, DeviceBreakdownRow, GeoBreakdownRow,
    PagePerformanceRow, PerformanceTrendPoint, RumErrorBreakdown, RumOverview, RumTimeRange,
    SessionStats, TopJsErrorRow, TopResourceErrorRow,
};
pub use rum_ingest_request::{
    RumIngestJsError, RumIngestPageView, RumIngestRequest, RumIngestResourceError,
};

pub mod verify_email_otp_request;
pub use verify_email_otp_request::VerifyEmailOtpRequest;

pub mod payg;
pub use payg::MeterUsageRequest;

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/dto/payg.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/dto/payg.rs'
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API input deliberately contains no price, total, currency, or unit_price.
/// `deny_unknown_fields` makes attempts to smuggle client-controlled financial
/// values fail deserialization instead of silently ignoring them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeterUsageRequest {
    pub operation: String,
    pub units: i64,
    pub request_id: Uuid,
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/ports/mod.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/ports/mod.rs'
pub mod api_flow_repository;
pub mod check_repository;
pub mod diagnostic_report_repository;
pub mod http_client;
pub mod id_generator;
pub mod incident_repository;
pub mod monitor_repository;
pub mod notification_channel_repository;
pub mod notification_outbox_repository;
pub mod notification_outbox_sender;
pub mod notification_service;
pub mod password_hasher;
pub mod proxy_check_job_repository;
pub mod proxy_credential_cipher;
pub mod secret_cipher;
pub mod subscriber_repository;
pub mod token_service;
pub mod user_repository;

// Sabbin ports:
pub mod api_key_repository;
pub mod audit_log_entry_repository;
pub mod backup_code_repository;
pub mod custom_domain_verifier;
pub mod escalation_policy_repository;
pub mod maintenance_window_repository;
pub mod pending_auth_cache;
pub mod rate_limiter;
pub mod registration_transaction;
pub mod status_page_cache;
pub mod status_page_repository;
pub mod tenant_membership_repository;
pub mod tenant_repository;

// B5 — Email verification:
pub mod email_verification_mailer;
pub mod email_verification_token_repository;

// OAuth — Continue with Google / Apple:
pub mod id_token_verifier;

// B9 — Tenant invitations:
pub mod tenant_invitation_repository;

// B34 — Public Endpoint Security & Efficiency:
pub mod health_check;

// H6 — Dashboard Overview:
pub mod dashboard_repository;

// H14 — Help Center & Support:
pub mod help_article_repository;
pub mod help_category_repository;
pub mod support_ticket_repository;

pub use api_flow_repository::ApiFlowRepository;
pub use check_repository::CheckRepository;
pub use diagnostic_report_repository::DiagnosticReportRepository;
pub use http_client::*;
pub use id_generator::*;
pub use incident_repository::IncidentRepository;
// B13 — Atomic Quota Enforcement: re-export QuotaCheckOutcome alongside
// MonitorRepository so callers can `use application::ports::{MonitorRepository, QuotaCheckOutcome};`
pub use monitor_repository::{MonitorRepository, QuotaCheckOutcome};
pub use notification_channel_repository::NotificationChannelRepository;
pub use notification_outbox_repository::NotificationOutboxRepository;
pub use notification_outbox_sender::NotificationOutboxSender;
pub use notification_service::{NotificationService, NotifyResult};
pub use password_hasher::PasswordHasher;
pub use proxy_check_job_repository::{ProxyCheckJobRepository, ProxyResultStatusFilter};
pub use proxy_credential_cipher::{
    EncryptedProxyCredentials, ProxyCredentialCipher, ProxyCredentialCipherError,
};
pub use secret_cipher::{SecretCipher, SecretCipherError};
pub use subscriber_repository::SubscriberRepository;
pub use token_service::{TokenService, VerifiedToken};
pub use user_repository::UserRepository;

// Sabbin re-exports:
pub use api_key_repository::ApiKeyRepository;
pub use audit_log_entry_repository::AuditLogEntryRepository;
pub use backup_code_repository::BackupCodeRepository;
pub use custom_domain_verifier::{CustomDomainVerifier, VerifierResult};
pub use escalation_policy_repository::EscalationPolicyRepository;
pub use maintenance_window_repository::MaintenanceWindowRepository;
pub use pending_auth_cache::{PendingAuthCache, PendingAuthCacheResult};
pub use rate_limiter::{RateLimiter, RateLimiterResult};
pub use registration_transaction::{RegistrationTransaction, RegistrationTransactionResult};
// B34 — Public Endpoint Security & Efficiency: PUBLIC_STATUS_LIST_CACHE_KEY
// is the shared cache key for the public status *list* endpoint, used by
// both the API (get_public_status) and the worker (IncidentManager).
pub use status_page_cache::{CacheResult, PUBLIC_STATUS_LIST_CACHE_KEY, StatusPageCache};
pub use status_page_repository::StatusPageRepository;
pub use tenant_membership_repository::{LastOwnerGuardOutcome, TenantMembershipRepository};
pub use tenant_repository::TenantRepository;

pub mod logo_storage;
pub use logo_storage::{LogoStorage, StorageResult};

// B5 — Email verification re-exports:
pub use email_verification_mailer::{EmailVerificationMailer, MailerResult};
pub use email_verification_token_repository::EmailVerificationTokenRepository;

// OAuth re-exports:
pub use id_token_verifier::{IdTokenVerifier, OidcVerifierResult, VerifiedIdentity};

// B6 — Password reset:
pub mod password_reset_mailer;
pub mod reset_password_token_repository;
pub use password_reset_mailer::{PasswordResetMailer, PasswordResetMailerResult};
pub use reset_password_token_repository::ResetPasswordTokenRepository;

// B9 — Tenant invitation re-export:
pub use tenant_invitation_repository::TenantInvitationRepository;

// B11 — Plan / Subscription / Entitlement model:
pub mod subscription_repository;
pub use subscription_repository::SubscriptionRepository;

// B14 — Billing Provider Abstraction & Checkout:
pub mod checkout_session_repository;
pub mod payment_provider;
pub use checkout_session_repository::CheckoutSessionRepository;
pub use payment_provider::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentProviderResult, PaymentVerification,
};

// B15 — Billing Webhooks:
pub mod billing_audit_log_repository;
pub mod billing_webhook_idempotency_repository;
pub use billing_audit_log_repository::BillingAuditLogRepository;
pub use billing_webhook_idempotency_repository::BillingWebhookIdempotencyRepository;

// B17 — Durable Scheduler:
pub mod scheduling_repository;
pub use scheduling_repository::SchedulingRepository;

// B20 — Agent Execution & mTLS:
pub mod agent_lease_repository;
pub use agent_lease_repository::{
    AgentLease, AgentLeaseRepository, RepoResult, SubmitAgentResultOutcome,
};

// B22 — History Retention Enforcement:
pub mod history_retention_repository;
pub use history_retention_repository::HistoryRetentionRepository;

// B34 — Public Endpoint Security & Efficiency:
pub use health_check::{HealthCheckPort, HealthCheckResult};

// H6 — Dashboard Overview:
pub use dashboard_repository::DashboardRepository;
pub mod pricing_config_repository;
pub use pricing_config_repository::PricingConfigRepository;

// H12 — Proration Service:
pub mod subscription_usage_period_repository;
pub mod subscription_usage_transaction;
pub use subscription_usage_period_repository::SubscriptionUsagePeriodRepository;
pub use subscription_usage_transaction::{
    SubscriptionUsageTransaction, SubscriptionUsageTransactionResult,
};

pub mod add_on_subscription_repository;
pub use add_on_subscription_repository::AddOnSubscriptionRepository;

pub mod sms_provider_pricing;
pub use sms_provider_pricing::{SmsPricingResult, SmsProviderPricing};

// H14 — Help Center & Support:
pub use help_article_repository::HelpArticleRepository;
pub use help_category_repository::HelpCategoryRepository;
pub use support_ticket_repository::SupportTicketRepository;

// RUM (Real User Monitoring):
pub mod geo_ip_lookup;
pub mod rum_analytics_repository;
pub mod rum_event_repository;
pub mod rum_site_repository;
pub use geo_ip_lookup::GeoIpLookup;
pub use rum_analytics_repository::RumAnalyticsRepository;
pub use rum_event_repository::RumEventRepository;
pub use rum_site_repository::RumSiteRepository;

pub mod wallet_repository;
pub use wallet_repository::{WalletRepoResult, WalletRepository};

pub mod payg_pricing_repository;
pub use payg_pricing_repository::{PAYGPricingRepository, PaygPricingRepoResult};
pub mod usage_event_repository;
pub use usage_event_repository::{UsageEventRepoResult, UsageEventRepository};

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/ports/payg_pricing_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/ports/payg_pricing_repository.rs'
use async_trait::async_trait;
use domain::PAYGPricing;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub type PaygPricingRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait PAYGPricingRepository: Send + Sync {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>>;
}

#[async_trait]
impl<T> PAYGPricingRepository for Arc<T>
where
    T: PAYGPricingRepository + Send + Sync + ?Sized,
{
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        (**self).find_active(operation).await
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/ports/usage_event_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/ports/usage_event_repository.rs'
use async_trait::async_trait;
use domain::UsageEvent;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub type UsageEventRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait UsageEventRepository: Send + Sync {
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>>;

    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent>;
}

#[async_trait]
impl<T> UsageEventRepository for Arc<T>
where
    T: UsageEventRepository + Send + Sync + ?Sized,
{
    async fn find_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> UsageEventRepoResult<Option<UsageEvent>> {
        (**self).find_by_request(tenant_id, request_id).await
    }

    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        (**self).create(event).await
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/ports/wallet_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/ports/wallet_repository.rs'
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{CreditReservation, Wallet, WalletTransaction, WalletTransactionType};
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub type WalletRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationOutcome {
    pub reservation: CreditReservation,
    pub created: bool,
}

#[async_trait]
pub trait WalletRepository: Send + Sync {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet>;
    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>>;

    /// Atomically checks available credits and creates (or returns) the
    /// idempotent reservation. Implementations MUST lock the wallet row and
    /// perform the uniqueness check and insert in the same transaction.
    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome>;

    /// Captures a pending reservation and writes the immutable debit ledger
    /// entry atomically. Idempotent for an already-captured reservation.
    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction>;

    /// Releases a pending reservation without charging the wallet.
    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()>;

    /// Credits the wallet atomically. Used only by trusted payment/webhook
    /// flows; callers must provide an idempotency key.
    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction>;
}

#[async_trait]
impl<T> WalletRepository for Arc<T>
where
    T: WalletRepository + Send + Sync + ?Sized,
{
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        (**self).get_or_create(tenant_id, currency).await
    }
    async fn list_transactions(&self, tenant_id: Uuid, limit: u32) -> WalletRepoResult<Vec<WalletTransaction>> {
        (**self).list_transactions(tenant_id, limit).await
    }
    async fn reserve(&self, tenant_id: Uuid, amount_units: i64, operation: &str, request_id: &str, expires_at: DateTime<Utc>) -> WalletRepoResult<ReservationOutcome> {
        (**self).reserve(tenant_id, amount_units, operation, request_id, expires_at).await
    }
    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction> {
        (**self).capture(reservation_id).await
    }
    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        (**self).release(reservation_id).await
    }
    async fn credit(&self, tenant_id: Uuid, amount_units: i64, transaction_type: WalletTransactionType, idempotency_key: &str, description: Option<&str>) -> WalletRepoResult<WalletTransaction> {
        (**self).credit(tenant_id, amount_units, transaction_type, idempotency_key, description).await
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/use_cases/charge_for_usage.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/use_cases/charge_for_usage.rs'
use crate::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use chrono::{Duration, Utc};
use domain::{CreditReservation, UsageEvent, WalletTransaction};
use std::{error::Error, fmt, future::Future, sync::Arc, time::Duration as StdDuration};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeForUsageInput {
    pub tenant_id: Uuid,
    pub operation: String,
    pub units: i64,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeForUsageResult {
    pub usage_event: UsageEvent,
    pub transaction: Option<WalletTransaction>,
}

#[derive(Debug)]
pub enum ChargeForUsageError {
    InvalidInput(&'static str),
    NoPricing,
    InvalidPricing(&'static str),
    InsufficientBalance,
    OperationFailed(String),
    Repository(Box<dyn Error + Send + Sync>),
    ReservationInProgress,
    CaptureFailed(Box<dyn Error + Send + Sync>),
}
impl fmt::Display for ChargeForUsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(m) => write!(f, "{m}"),
            Self::NoPricing => write!(f, "no active PAYG pricing found"),
            Self::InvalidPricing(m) => write!(f, "{m}"),
            Self::InsufficientBalance => write!(f, "PAYG balance is insufficient"),
            Self::OperationFailed(m) => write!(f, "metered operation failed: {m}"),
            Self::Repository(_) | Self::CaptureFailed(_) => write!(f, "PAYG repository operation failed"),
            Self::ReservationInProgress => write!(f, "PAYG request is already in progress"),
        }
    }
}
impl ChargeForUsageError {
    pub fn app_code(&self) -> Option<&'static str> {
        match self {
            Self::InsufficientBalance => Some("PAYG_INSUFFICIENT_BALANCE"),
            _ => None,
        }
    }
}

impl Error for ChargeForUsageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(e) | Self::CaptureFailed(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

pub struct ChargeForUsageUseCase {
    wallet_repository: Arc<dyn WalletRepository>,
    pricing_repository: Arc<dyn PAYGPricingRepository>,
    usage_event_repository: Arc<dyn UsageEventRepository>,
}
impl ChargeForUsageUseCase {
    pub fn new(
        wallet_repository: Arc<dyn WalletRepository>,
        pricing_repository: Arc<dyn PAYGPricingRepository>,
        usage_event_repository: Arc<dyn UsageEventRepository>,
    ) -> Self {
        Self { wallet_repository, pricing_repository, usage_event_repository }
    }

    pub async fn execute<F, Fut, E>(
        &self,
        input: ChargeForUsageInput,
        operation: F,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), E>>,
        E: fmt::Display,
    {
        validate_input(&input)?;

        // Fast idempotency path: a completed request always returns its
        // original result and never calls the operation again.
        if let Some(event) = self.usage_event_repository
            .find_by_request(input.tenant_id, &input.request_id)
            .await
            .map_err(ChargeForUsageError::Repository)? {
            return Ok(ChargeForUsageResult { usage_event: event, transaction: None });
        }

        let pricing = self.pricing_repository.find_active(&input.operation)
            .await.map_err(ChargeForUsageError::Repository)?
            .ok_or(ChargeForUsageError::NoPricing)?;
        if pricing.unit_price <= 0 {
            return Err(ChargeForUsageError::InvalidPricing("active PAYG unit price must be positive"));
        }

        let charge_units = input.units.checked_mul(pricing.unit_price)
            .ok_or(ChargeForUsageError::InvalidPricing("PAYG charge overflow"))?;

        let expires_at = Utc::now() + Duration::seconds(60);
        let reservation_outcome = self.wallet_repository
            .reserve(input.tenant_id, charge_units, &input.operation, &input.request_id, expires_at)
            .await
            .map_err(|e| {
                if e.to_string() == "insufficient wallet balance" {
                    ChargeForUsageError::InsufficientBalance
                } else {
                    ChargeForUsageError::Repository(e)
                }
            })?;

        if !reservation_outcome.created {
            // Another request with the same idempotency key owns the
            // operation. Never execute the billable operation twice.
            if reservation_outcome.reservation.status == CreditReservation::CAPTURED {
                let event = self.recover_captured_event(&input, &pricing.version, charge_units, &pricing.currency, &pricing.unit_price).await?;
                return Ok(ChargeForUsageResult { usage_event: event, transaction: None });
            }
            return self.wait_for_previous_result(&input, charge_units, pricing.unit_price, pricing.version, &pricing.currency).await;
        }

        let reservation = reservation_outcome.reservation;
        if let Err(err) = operation().await {
            let _ = self.wallet_repository.release(reservation.id).await;
            let event = self.record_event(&input, 0, pricing.unit_price, 0, pricing.version, &pricing.currency, "FAILED").await?;
            return Err(ChargeForUsageError::OperationFailed(format!("{err}; usage_event={}", event.id)));
        }

        let transaction = match self.wallet_repository.capture(reservation.id).await {
            Ok(tx) => tx,
            Err(err) => {
                return Err(ChargeForUsageError::CaptureFailed(err));
            }
        };

        let event = self.record_event(
            &input, input.units, pricing.unit_price, charge_units, pricing.version, &pricing.currency, "CAPTURED"
        ).await?;

        Ok(ChargeForUsageResult { usage_event: event, transaction: Some(transaction) })
    }

    async fn record_event(
        &self, input: &ChargeForUsageInput, units_billed: i64, unit_price: i64,
        charge_units: i64, pricing_version: i64, currency: &str, status: &str,
    ) -> Result<UsageEvent, ChargeForUsageError> {
        self.usage_event_repository.create(&UsageEvent {
            id: Uuid::new_v4(),
            tenant_id: input.tenant_id,
            operation: input.operation.clone(),
            units_requested: input.units,
            units_billed,
            unit_price,
            charge_units,
            currency: currency.to_owned(),
            pricing_version,
            request_id: input.request_id.clone(),
            status: status.to_owned(),
            created_at: Utc::now(),
        }).await.map_err(ChargeForUsageError::Repository)
    }

    async fn recover_captured_event(
        &self, input: &ChargeForUsageInput, pricing_version: &i64, charge_units: i64,
        currency: &str, unit_price: &i64,
    ) -> Result<UsageEvent, ChargeForUsageError> {
        if let Some(event) = self.usage_event_repository
            .find_by_request(input.tenant_id, &input.request_id).await
            .map_err(ChargeForUsageError::Repository)? {
            return Ok(event);
        }
        self.record_event(input, input.units, *unit_price, charge_units, *pricing_version, currency, "CAPTURED").await
    }

    async fn wait_for_previous_result(
        &self, input: &ChargeForUsageInput, charge_units: i64, unit_price: i64,
        pricing_version: i64, currency: &str,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError> {
        for _ in 0..100 {
            if let Some(event) = self.usage_event_repository
                .find_by_request(input.tenant_id, &input.request_id).await
                .map_err(ChargeForUsageError::Repository)? {
                return Ok(ChargeForUsageResult { usage_event: event, transaction: None });
            }
            tokio::time::sleep(StdDuration::from_millis(50)).await;
        }
        let _ = (charge_units, unit_price, pricing_version, currency);
        Err(ChargeForUsageError::ReservationInProgress)
    }
}

fn validate_input(input: &ChargeForUsageInput) -> Result<(), ChargeForUsageError> {
    if input.units <= 0 { return Err(ChargeForUsageError::InvalidInput("units must be positive")); }
    if input.operation.trim().is_empty() { return Err(ChargeForUsageError::InvalidInput("operation is required")); }
    if input.request_id.trim().is_empty() { return Err(ChargeForUsageError::InvalidInput("request_id is required")); }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
    use domain::PAYGPricing;
    use infra::{InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository};
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

    fn setup(unit_price: i64, version: i64) -> (
        Arc<dyn WalletRepository>, Arc<dyn PAYGPricingRepository>,
        Arc<dyn UsageEventRepository>, Uuid
    ) {
        let wallet = Arc::new(InMemoryWalletRepository::default());
        let pricing = Arc::new(InMemoryPAYGPricingRepository::new(vec![PAYGPricing {
            id: Uuid::new_v4(), version, operation: "proxy_check".into(),
            unit_name: "check".into(), unit_price, currency: "USD".into(),
            effective_from: Utc::now() - Duration::seconds(1), effective_until: None, active: true,
        }]));
        let usage = Arc::new(InMemoryUsageEventRepository::default());
        let tenant = Uuid::new_v4();
        (wallet, pricing, usage, tenant)
    }

    #[tokio::test]
    async fn duplicate_request_returns_previous_result_without_running_operation_twice() {
        let (wallet, pricing, usage, tenant) = setup(10, 7);
        wallet.credit(tenant, 100, domain::WalletTransactionType::TopUp, "topup-1", None).await.unwrap();
        let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
        let calls = Arc::new(AtomicUsize::new(0));

        let input = ChargeForUsageInput {
            tenant_id: tenant, operation: "proxy_check".into(), units: 2, request_id: "req-1".into()
        };
        let c = calls.clone();
        let first = uc.execute(input.clone(), move || {
            let c = c.clone();
            async move { c.fetch_add(1, Ordering::SeqCst); Ok::<_, &'static str>(()) }
        }).await.unwrap();
        let c = calls.clone();
        let second = uc.execute(input, move || {
            let c = c.clone();
            async move { c.fetch_add(1, Ordering::SeqCst); Ok::<_, &'static str>(()) }
        }).await.unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(first.usage_event, second.usage_event);
        assert_eq!(second.usage_event.pricing_version, 7);
        assert_eq!(second.usage_event.charge_units, 20);
    }

    #[tokio::test]
    async fn concurrent_different_requests_never_overspend() {
        let (wallet, pricing, usage, tenant) = setup(60, 3);
        wallet.credit(tenant, 100, domain::WalletTransactionType::TopUp, "topup-2", None).await.unwrap();
        let uc = Arc::new(ChargeForUsageUseCase::new(wallet.clone(), pricing, usage));
        let mut tasks = Vec::new();
        for i in 0..10 {
            let uc = uc.clone();
            tasks.push(tokio::spawn(async move {
                let input = ChargeForUsageInput {
                    tenant_id: tenant, operation: "proxy_check".into(), units: 1,
                    request_id: format!("concurrent-{i}"),
                };
                uc.execute(input, || async { Ok::<_, &'static str>(()) }).await
            }));
        }
        let mut success = 0;
        for task in tasks {
            if task.await.unwrap().is_ok() { success += 1; }
        }
        assert_eq!(success, 1);
        let final_wallet = wallet.get_or_create(tenant, "USD").await.unwrap();
        assert_eq!(final_wallet.balance_units, 40);
    }

    #[tokio::test]
    async fn insufficient_balance_is_rejected_and_wallet_never_goes_negative() {
        let (wallet, pricing, usage, tenant) = setup(60, 2);
        wallet.credit(tenant, 50, domain::WalletTransactionType::TopUp, "topup-3", None).await.unwrap();
        let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);
        let result = uc.execute(ChargeForUsageInput {
            tenant_id: tenant, operation: "proxy_check".into(), units: 1, request_id: "insufficient".into()
        }, || async { Ok::<_, &'static str>(()) }).await;
        assert!(matches!(result, Err(ChargeForUsageError::InsufficientBalance)));
        assert_eq!(wallet.get_or_create(tenant, "USD").await.unwrap().balance_units, 50);
    }

    #[tokio::test]
    async fn failed_operation_releases_reservation_and_does_not_debit() {
        let (wallet, pricing, usage, tenant) = setup(25, 9);
        wallet.credit(tenant, 100, domain::WalletTransactionType::TopUp, "topup-4", None).await.unwrap();
        let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
        let result = uc.execute(ChargeForUsageInput {
            tenant_id: tenant, operation: "proxy_check".into(), units: 2, request_id: "failed-op".into()
        }, || async { Err::<(), _>("downstream failed") }).await;
        assert!(matches!(result, Err(ChargeForUsageError::OperationFailed(_))));
        assert_eq!(wallet.get_or_create(tenant, "USD").await.unwrap().balance_units, 100);
        let event = usage.find_by_request(tenant, "failed-op").await.unwrap().unwrap();
        assert_eq!(event.status, "FAILED");
        assert_eq!(event.units_billed, 0);
    }

    #[test]
    fn client_payload_cannot_supply_financial_values() {
        let json = r#"{"operation":"proxy_check","units":1,"request_id":"00000000-0000-0000-0000-000000000001","price":1,"total":1,"unit_price":1}"#;
        assert!(serde_json::from_str::<crate::dto::MeterUsageRequest>(json).is_err());
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/use_cases/meter_usage.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/use_cases/meter_usage.rs'
use super::charge_for_usage::{ChargeForUsageError, ChargeForUsageInput, ChargeForUsageResult, ChargeForUsageUseCase};
use crate::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterUsageInput {
    pub tenant_id: Uuid,
    pub operation: String,
    pub units: i64,
    pub request_id: String,
}

pub struct MeterUsageUseCase {
    charge: ChargeForUsageUseCase,
}
impl MeterUsageUseCase {
    pub fn new(
        wallet_repository: Arc<dyn WalletRepository>,
        pricing_repository: Arc<dyn PAYGPricingRepository>,
        usage_event_repository: Arc<dyn UsageEventRepository>,
    ) -> Self {
        Self { charge: ChargeForUsageUseCase::new(wallet_repository, pricing_repository, usage_event_repository) }
    }

    /// Authentication/tenant identification belongs to the API/auth boundary;
    /// this use case accepts only the authenticated tenant id and never trusts
    /// a client-supplied price or total.
    pub async fn execute<F, Fut, E>(
        &self,
        input: MeterUsageInput,
        operation: F,
    ) -> Result<ChargeForUsageResult, ChargeForUsageError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), E>>,
        E: fmt::Display,
    {
        self.charge.execute(ChargeForUsageInput {
            tenant_id: input.tenant_id,
            operation: input.operation,
            units: input.units,
            request_id: input.request_id,
        }, operation).await
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/application/src/use_cases/mod.rs')"
cat <<'EOF_WORKER4' > 'crates/application/src/use_cases/mod.rs'
#![allow(ambiguous_glob_reexports)]
pub mod accept_invitation;
pub mod acknowledge_incident;
pub mod add_member;
pub mod begin_grace_period;
pub mod billing_support;
pub mod billing_webhook_support;
pub mod cancel_subscription;
pub mod change_member_role;
pub mod change_password;
pub mod confirm_totp_setup;
pub mod create_api_flow;
pub mod create_api_key;
pub mod create_email_verification_token;
pub mod create_escalation_policy;
pub mod create_invitation;
pub mod create_maintenance_window;
pub mod create_monitor;
pub mod create_notification_channel;
pub mod create_status_page;
pub mod delete_api_flow;
pub mod delete_api_key;
pub mod delete_monitor;
pub mod delete_notification_channel;
pub mod delete_status_page;
pub mod disable_totp;
pub mod email_verification_support;
pub mod enable_totp;
pub mod expire_subscription;
pub mod finalize_scheduled_cancellation;
pub mod get_dashboard_overview;
pub mod get_monitor;
pub mod get_proxy_check_job;
pub mod initiate_checkout;
pub mod invitation_common;
pub mod invitation_support;
pub mod list_api_keys;
pub mod list_checks;
pub mod list_incidents;
pub mod list_invitations;
pub mod list_members;
pub mod list_monitor_checks;
pub mod list_monitors;
pub mod list_notification_channels;
pub mod list_status_pages;
pub mod login_user;
pub mod membership_common;
pub mod oauth_login_user;
pub mod process_billing_webhook;
pub mod register_user;
pub mod remove_custom_domain;
pub mod remove_member;
pub mod request_custom_domain;
pub mod request_password_reset;
pub mod resend_verification;
pub mod reset_password;
pub mod reset_password_support;
pub mod resolve_entitlements;
pub mod revoke_invitation;
pub mod send_test_notification;
pub mod session_invalidation;
pub mod submit_proxy_check_job;
pub mod subscription_lifecycle_support;
pub mod totp_support;
pub mod update_api_flow;
pub mod update_monitor;
pub mod update_monitor_enabled;
pub mod update_status_page;
pub mod upload_status_page_logo;
pub mod verify_checkout;
pub mod verify_email;
pub mod verify_totp_and_login;

// H14 — Help Center & Support:
pub mod create_support_ticket;
pub mod get_help_article;
pub mod get_support_ticket;
pub mod list_help_categories;
pub mod list_my_support_tickets;
pub mod reply_to_support_ticket;
pub mod search_help_articles;
pub mod update_support_ticket_status;

pub use accept_invitation::*;
pub use acknowledge_incident::*;
pub use add_member::*;
pub use begin_grace_period::*;
pub use billing_support::*;
pub use billing_webhook_support::*;
pub use cancel_subscription::*;
pub use change_member_role::*;
pub use change_password::*;
pub use confirm_totp_setup::*;
pub use create_api_flow::*;
pub use create_api_key::*;
pub use create_email_verification_token::*;
pub use create_escalation_policy::*;
pub use create_invitation::*;
pub use create_maintenance_window::*;
pub use create_monitor::*;
pub use create_notification_channel::*;
pub use create_status_page::*;
pub use delete_api_flow::*;
pub use delete_api_key::*;
pub use delete_monitor::*;
pub use delete_notification_channel::*;
pub use delete_status_page::*;
pub use disable_totp::*;
pub use email_verification_support::*;
pub use enable_totp::*;
pub use expire_subscription::*;
pub use finalize_scheduled_cancellation::*;
pub use get_dashboard_overview::*;
pub use get_monitor::*;
pub use get_proxy_check_job::*;
pub use initiate_checkout::*;
pub use invitation_common::*;
pub use invitation_support::*;
pub use list_api_keys::*;
pub use list_checks::*;
pub use list_incidents::*;
pub use list_invitations::*;
pub use list_members::*;
pub use list_monitor_checks::*;
pub use list_monitors::*;
pub use list_notification_channels::*;
pub use list_status_pages::*;
pub use login_user::*;
pub use membership_common::*;
pub use oauth_login_user::*;
pub use process_billing_webhook::*;
pub use register_user::*;
pub use remove_custom_domain::*;
pub use remove_member::*;
pub use request_custom_domain::*;
pub use request_password_reset::*;
pub use resend_verification::*;
pub use reset_password::*;
pub use reset_password_support::*;
pub use resolve_entitlements::*;
pub use revoke_invitation::*;
pub use send_test_notification::*;
pub use session_invalidation::*;
pub use submit_proxy_check_job::*;
pub use subscription_lifecycle_support::*;
pub use update_api_flow::*;
pub use update_monitor::*;
pub use update_monitor_enabled::*;
pub use update_status_page::*;
pub use upload_status_page_logo::*;
pub use verify_checkout::*;
pub use verify_email::*;
pub use verify_totp_and_login::*;
pub mod calculate_pricing_quote;

// H12 — Proration Service:
pub mod calculate_subscription_cycle_cost;
pub mod update_subscription_usage;
pub use calculate_subscription_cycle_cost::*;
pub use update_subscription_usage::*;

pub mod add_on_subscription;

// H14 — Help Center & Support:
pub use create_support_ticket::{CreateSupportTicketError, CreateSupportTicketUseCase};
pub use get_help_article::GetHelpArticleUseCase;
pub use get_support_ticket::GetSupportTicketUseCase;
pub use list_help_categories::ListHelpCategoriesUseCase;
pub use list_my_support_tickets::ListMySupportTicketsUseCase;
pub use reply_to_support_ticket::ReplyToSupportTicketUseCase;
pub use search_help_articles::SearchHelpArticlesUseCase;
pub use update_support_ticket_status::UpdateSupportTicketStatusUseCase;

// RUM (Real User Monitoring):
pub mod ingest_rum_event;
pub mod manage_rum_sites;
pub mod rum_dashboard_queries;
pub use ingest_rum_event::{IngestRumEventError, IngestRumEventUseCase};
pub use manage_rum_sites::{CreateRumSiteError, CreateRumSiteUseCase, ListRumSitesUseCase};
pub use rum_dashboard_queries::{RumDashboardError, RumDashboardQueriesUseCase};

pub mod verify_email_otp;
pub use verify_email_otp::{VerifyEmailOtpError, VerifyEmailOtpUseCase};

pub mod cancel_proxy_check_job;
pub mod list_proxy_check_results;
pub use cancel_proxy_check_job::*;
pub use list_proxy_check_results::*;

pub mod charge_for_usage;
pub mod meter_usage;
pub use charge_for_usage::{ChargeForUsageError, ChargeForUsageInput, ChargeForUsageResult, ChargeForUsageUseCase};
pub use meter_usage::{MeterUsageInput, MeterUsageUseCase};

EOF_WORKER4

mkdir -p "$(dirname 'crates/domain/src/entities/invoice.rs')"
cat <<'EOF_WORKER4' > 'crates/domain/src/entities/invoice.rs'
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Invoice domain shape only. Invoice generation/settlement belongs to Worker 6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub number: String,
    pub status: String,
    pub currency: String,
    pub subtotal_units: i64,
    pub total_units: i64,
    pub issued_at: Option<DateTime<Utc>>,
    pub due_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/domain/src/entities/mod.rs')"
cat <<'EOF_WORKER4' > 'crates/domain/src/entities/mod.rs'
pub mod assertion;
pub use assertion::*;
pub mod api_flow_step;
pub use api_flow_step::ApiFlowStep;
pub mod api_flow;
pub use api_flow::ApiFlow;
pub mod proxy_entry;
pub use proxy_entry::ProxyEntry;
pub mod proxy_check_job;
pub use proxy_check_job::ProxyCheckJob;
pub mod diagnostic_report;
pub use diagnostic_report::*;

// Sabbin modules:
pub mod escalation_policy;
pub use escalation_policy::{EscalationPolicy, EscalationStep};

mod maintenance_window;
pub use maintenance_window::MaintenanceWindow;

mod status_page;
pub use status_page::*;

mod tenant;
pub use tenant::Tenant;

mod tenant_membership;
pub use tenant_membership::TenantMembership;

mod audit_log_entry;
pub use audit_log_entry::AuditLogEntry;

mod api_key;
pub use api_key::{ApiKey, CreateApiKey};

mod backup_code;
pub use backup_code::BackupCode;

// B5 — Email Verification:
mod email_verification_token;
pub use email_verification_token::EmailVerificationToken;

// B6 — Password reset:
mod reset_password_token;
pub use reset_password_token::ResetPasswordToken;

// B9 — Tenant invitations:
mod tenant_invitation;
pub use tenant_invitation::TenantInvitation;

// B11 — Plan / Subscription / Entitlement model:
mod plan;
pub use plan::Plan;

mod subscription;
pub use subscription::Subscription;

// B14 — Billing Provider Abstraction & Checkout:
mod checkout_session;
pub use checkout_session::CheckoutSession;

// B15 — Billing Webhooks:
pub mod billing_audit_log_entry;
pub mod billing_webhook_event;
pub use billing_audit_log_entry::BillingAuditLogEntry;
pub use billing_webhook_event::BillingWebhookEvent;

// H9 — Pricing Audit Log:
mod pricing_audit_log_entry;
pub use pricing_audit_log_entry::PricingAuditLogEntry;

mod notification_outbox_entry;
pub use notification_outbox_entry::NotificationOutboxEntry;

// H12 — Proration Service:
mod subscription_usage_period;
pub use subscription_usage_period::SubscriptionUsagePeriod;

// Gyara: an manta da bayyana wadannan modules
mod check;
pub use check::Check;
mod check_result;
pub use check_result::CheckResult;
mod monitor;
pub use monitor::*;
mod monitor_builder;
pub use monitor_builder::MonitorBuilder;
mod user;
pub use user::User;
mod user_builder;
pub use user_builder::UserBuilder;

// Gyara: an manta da bayyana wadannan modules
mod incident;
pub use incident::*;
mod notification_channel;
pub use notification_channel::{CreateEmailChannel, CreateWebhookChannel, NotificationChannel};
mod subscriber;
pub use subscriber::*;

// H14 — Help Center & Support:
mod help_article;
mod help_category;
mod support_message;
mod support_ticket;

pub use help_article::{CreateHelpArticle, HelpArticle};
pub use help_category::{CreateHelpCategory, HelpCategory};
pub use support_message::{CreateSupportMessage, SupportMessage};
pub use support_ticket::{CreateSupportTicket, SupportTicket};

mod add_on_subscription;
pub use add_on_subscription::{AddOnStatus, AddOnSubscription, AddOnType};

// RUM (Real User Monitoring):
mod rum_site;
pub use rum_site::RumSite;
mod rum_session;
pub use rum_session::{DeviceType, RumSession};
mod rum_page_view;
pub use rum_page_view::{RumPageView, RumVitals};
mod rum_js_error;
pub use rum_js_error::RumJsError;
mod rum_resource_error;
pub use rum_resource_error::{RumResourceError, RumResourceType};

pub mod wallet;
pub use wallet::{CreditReservation, Wallet, WalletTransaction, WalletTransactionType};

mod payg;
pub use payg::{PAYGPricing, Payment, UsageEvent};
mod invoice;
pub use invoice::Invoice;

EOF_WORKER4

mkdir -p "$(dirname 'crates/domain/src/entities/payg.rs')"
cat <<'EOF_WORKER4' > 'crates/domain/src/entities/payg.rs'
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Immutable, server-controlled PAYG price for one billable unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PAYGPricing {
    pub id: Uuid,
    pub version: i64,
    pub operation: String,
    pub unit_name: String,
    pub unit_price: i64,
    pub currency: String,
    pub effective_from: DateTime<Utc>,
    pub effective_until: Option<DateTime<Utc>>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEvent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub operation: String,
    pub units_requested: i64,
    pub units_billed: i64,
    pub unit_price: i64,
    pub charge_units: i64,
    pub currency: String,
    pub pricing_version: i64,
    pub request_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub amount_units: i64,
    pub currency: String,
    pub status: String,
    pub provider: String,
    pub provider_payment_id: Option<String>,
    pub idempotency_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/domain/src/entities/wallet.rs')"
cat <<'EOF_WORKER4' > 'crates/domain/src/entities/wallet.rs'
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalletTransactionType {
    TopUp,
    Debit,
    Refund,
    Adjustment,
    PromotionalCredit,
    Expiration,
}
impl WalletTransactionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopUp => "TOP_UP", Self::Debit => "DEBIT", Self::Refund => "REFUND",
            Self::Adjustment => "ADJUSTMENT", Self::PromotionalCredit => "PROMOTIONAL_CREDIT",
            Self::Expiration => "EXPIRATION",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wallet {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub currency: String,
    pub balance_units: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Wallet {
    pub fn new(id: Uuid, tenant_id: Uuid, currency: impl Into<String>) -> Self {
        let now = Utc::now();
        Self { id, tenant_id, currency: currency.into(), balance_units: 0, created_at: now, updated_at: now }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletTransaction {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub wallet_id: Uuid,
    pub transaction_type: WalletTransactionType,
    pub amount_units: i64,
    pub balance_after_units: i64,
    pub idempotency_key: String,
    pub reference_type: Option<String>,
    pub reference_id: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreditReservation {
    pub id: Uuid,
    pub wallet_id: Uuid,
    pub amount_units: i64,
    pub status: String,
    pub operation: String,
    pub request_id: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl CreditReservation {
    pub const PENDING: &'static str = "PENDING";
    pub const CAPTURED: &'static str = "CAPTURED";
    pub const RELEASED: &'static str = "RELEASED";
    pub const EXPIRED: &'static str = "EXPIRED";
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/infra/src/repositories/in_memory_payg_pricing_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/infra/src/repositories/in_memory_payg_pricing_repository.rs'
use application::ports::{PAYGPricingRepository, PaygPricingRepoResult};
use async_trait::async_trait;
use domain::PAYGPricing;
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct InMemoryPAYGPricingRepository {
    rows: Arc<RwLock<Vec<PAYGPricing>>>,
}
impl InMemoryPAYGPricingRepository {
    pub fn new(rows: Vec<PAYGPricing>) -> Self { Self { rows: Arc::new(RwLock::new(rows)) } }
    pub fn insert(&self, row: PAYGPricing) { self.rows.write().unwrap().push(row); }
}
#[async_trait]
impl PAYGPricingRepository for InMemoryPAYGPricingRepository {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let now = chrono::Utc::now();
        let row = self.rows.read().unwrap().iter()
            .filter(|p| p.operation == operation && p.active && p.effective_from <= now)
            .filter(|p| p.effective_until.map(|v| v > now).unwrap_or(true))
            .max_by_key(|p| p.version)
            .cloned();
        Ok(row)
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/infra/src/repositories/in_memory_usage_event_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/infra/src/repositories/in_memory_usage_event_repository.rs'
use application::ports::{UsageEventRepoResult, UsageEventRepository};
use async_trait::async_trait;
use domain::UsageEvent;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct InMemoryUsageEventRepository {
    rows: Arc<Mutex<HashMap<(Uuid, String), UsageEvent>>>,
}
#[async_trait]
impl UsageEventRepository for InMemoryUsageEventRepository {
    async fn find_by_request(&self, tenant_id: Uuid, request_id: &str) -> UsageEventRepoResult<Option<UsageEvent>> {
        Ok(self.rows.lock().unwrap().get(&(tenant_id, request_id.to_owned())).cloned())
    }
    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        let mut rows = self.rows.lock().unwrap();
        let key = (event.tenant_id, event.request_id.clone());
        if let Some(existing) = rows.get(&key) { return Ok(existing.clone()); }
        rows.insert(key, event.clone());
        Ok(event.clone())
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/infra/src/repositories/in_memory_wallet_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/infra/src/repositories/in_memory_wallet_repository.rs'
use application::ports::{ReservationOutcome, WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{CreditReservation, Wallet, WalletTransaction, WalletTransactionType};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

type WalletTransactions = Vec<WalletTransaction>;
type WalletReservations = HashMap<Uuid, CreditReservation>;
type WalletState = HashMap<Uuid, (Wallet, WalletTransactions, WalletReservations)>;

#[derive(Clone, Default)]
pub struct InMemoryWalletRepository {
    inner: Arc<Mutex<WalletState>>,
}

#[async_trait]
impl WalletRepository for InMemoryWalletRepository {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        let mut g = self.inner.lock().unwrap();
        let (wallet, _, _) = g.entry(tenant_id).or_insert_with(|| {
            (
                Wallet::new(Uuid::new_v4(), tenant_id, currency),
                Vec::new(),
                HashMap::new(),
            )
        });
        Ok(wallet.clone())
    }

    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>> {
        let g = self.inner.lock().unwrap();
        Ok(g.get(&tenant_id)
            .map(|(_, txs, _)| {
                txs.iter()
                    .rev()
                    .take(limit.min(100) as usize)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        if amount_units <= 0 {
            return Err("reservation amount must be positive".into());
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, _, reservations) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        let reserved_units = reservations
            .values()
            .filter(|r| r.status == CreditReservation::PENDING && r.expires_at > Utc::now())
            .try_fold(0_i64, |total, r| total.checked_add(r.amount_units))
            .ok_or("wallet reservation accounting overflow")?;
        if let Some(existing) = reservations
            .values()
            .find(|r| r.request_id == request_id)
        {
            return Ok(ReservationOutcome { reservation: existing.clone(), created: false });
        }
        let available_units = wallet.balance_units.checked_sub(reserved_units)
            .ok_or("wallet reservation accounting overflow")?;
        if available_units < amount_units {
            return Err("insufficient wallet balance".into());
        }
        let r = CreditReservation {
            id: Uuid::new_v4(),
            wallet_id: wallet.id,
            amount_units,
            status: CreditReservation::PENDING.to_owned(),
            operation: operation.to_owned(),
            request_id: request_id.to_owned(),
            expires_at,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        reservations.insert(r.id, r.clone());
        Ok(ReservationOutcome { reservation: r, created: true })
    }

    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction> {
        let mut g = self.inner.lock().unwrap();
        for (wallet, txs, reservations) in g.values_mut() {
            if let Some(r) = reservations.get_mut(&reservation_id) {
                if r.status == CreditReservation::CAPTURED {
                    return txs
                        .iter()
                        .find(|t| t.reference_id.as_deref() == Some(&reservation_id.to_string()))
                        .cloned()
                        .ok_or_else(|| "captured reservation ledger entry missing".into());
                }
                if r.status != CreditReservation::PENDING {
                    return Err("reservation is not capturable".into());
                }
                if wallet.balance_units < r.amount_units {
                    return Err("insufficient wallet balance".into());
                }
                wallet.balance_units -= r.amount_units;
                wallet.updated_at = Utc::now();
                r.status = CreditReservation::CAPTURED.to_owned();
                r.updated_at = Utc::now();
                let tx = WalletTransaction {
                    id: Uuid::new_v4(),
                    tenant_id: wallet.tenant_id,
                    wallet_id: wallet.id,
                    transaction_type: WalletTransactionType::Debit,
                    amount_units: -r.amount_units,
                    balance_after_units: wallet.balance_units,
                    idempotency_key: format!("reservation:{}", reservation_id),
                    reference_type: Some("credit_reservation".to_owned()),
                    reference_id: Some(reservation_id.to_string()),
                    description: Some(r.operation.clone()),
                    created_at: Utc::now(),
                };
                txs.push(tx.clone());
                return Ok(tx);
            }
        }
        Err("reservation not found".into())
    }

    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        let mut g = self.inner.lock().unwrap();
        for (_, _, reservations) in g.values_mut() {
            if let Some(r) = reservations.get_mut(&reservation_id) {
                if r.status == CreditReservation::PENDING {
                    r.status = CreditReservation::RELEASED.to_owned();
                    r.updated_at = Utc::now();
                }
                return Ok(());
            }
        }
        Err("reservation not found".into())
    }

    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if amount_units <= 0 {
            return Err("credit amount must be positive".into());
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, txs, _) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        if let Some(existing) = txs
            .iter()
            .find(|t| t.idempotency_key == idempotency_key)
        {
            return Ok(existing.clone());
        }
        wallet.balance_units = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;
        wallet.updated_at = Utc::now();
        let tx = WalletTransaction {
            id: Uuid::new_v4(),
            tenant_id: wallet.tenant_id,
            wallet_id: wallet.id,
            transaction_type,
            amount_units,
            balance_after_units: wallet.balance_units,
            idempotency_key: idempotency_key.to_owned(),
            reference_type: None,
            reference_id: None,
            description: description.map(str::to_owned),
            created_at: Utc::now(),
        };
        txs.push(tx.clone());
        Ok(tx)
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/infra/src/repositories/mod.rs')"
cat <<'EOF_WORKER4' > 'crates/infra/src/repositories/mod.rs'
pub mod in_memory_api_flow_repository;
pub mod in_memory_api_key_repository;
pub mod in_memory_audit_log_entry_repository;
pub mod in_memory_email_verification_token_repository;
pub mod in_memory_escalation_policy_repository;
pub mod in_memory_maintenance_window_repository;
pub mod in_memory_monitor_repository;
pub mod in_memory_notification_channel_repository;
pub mod in_memory_notification_outbox_repository;
pub mod in_memory_proxy_check_job_repository;
pub mod in_memory_registration_transaction;
pub mod in_memory_reset_password_token_repository;
pub mod in_memory_status_page_repository;
pub mod in_memory_subscriber_repository;
pub mod in_memory_tenant_invitation_repository;
pub mod in_memory_tenant_membership_repository;
pub mod in_memory_tenant_repository;
pub mod in_memory_user_repository;
pub mod postgres_api_flow_repository;
pub mod postgres_api_key_repository;
pub mod postgres_audit_log_entry_repository;
pub mod postgres_backup_code_repository;
pub mod postgres_check_repository;
pub mod postgres_diagnostic_report_repository;
pub mod postgres_email_verification_token_repository;
pub mod postgres_escalation_policy_repository;
pub mod postgres_incident_repository;
pub mod postgres_maintenance_window_repository;
pub mod postgres_monitor_repository;
pub mod postgres_notification_channel_repository;
pub mod postgres_notification_outbox_repository;
pub mod postgres_proxy_check_job_repository;
pub mod postgres_registration_transaction;
pub mod postgres_reset_password_token_repository;
pub mod postgres_status_page_repository;
pub mod postgres_subscriber_repository;
pub mod postgres_tenant_invitation_repository;
pub mod postgres_tenant_membership_repository;
pub mod postgres_tenant_repository;
pub mod postgres_user_repository;
// H6 — Dashboard Overview:
pub mod postgres_dashboard_repository;

pub use in_memory_api_flow_repository::*;
pub use in_memory_api_key_repository::InMemoryApiKeyRepository;
pub use in_memory_audit_log_entry_repository::InMemoryAuditLogEntryRepository;
pub use in_memory_email_verification_token_repository::InMemoryEmailVerificationTokenRepository;
pub use in_memory_escalation_policy_repository::*;
pub use in_memory_maintenance_window_repository::*;
pub use in_memory_monitor_repository::*;
pub use in_memory_notification_channel_repository::*;
pub use in_memory_notification_outbox_repository::InMemoryNotificationOutboxRepository;
pub use in_memory_proxy_check_job_repository::*;
pub use in_memory_registration_transaction::InMemoryRegistrationTransaction;
pub use in_memory_reset_password_token_repository::InMemoryResetPasswordTokenRepository;
pub use in_memory_status_page_repository::*;
pub use in_memory_subscriber_repository::*;
pub use in_memory_tenant_invitation_repository::InMemoryTenantInvitationRepository;
pub use in_memory_tenant_membership_repository::InMemoryTenantMembershipRepository;
pub use in_memory_tenant_repository::InMemoryTenantRepository;
pub use in_memory_user_repository::InMemoryUserRepository;
pub use postgres_api_flow_repository::*;
pub use postgres_api_key_repository::PostgresApiKeyRepository;
pub use postgres_audit_log_entry_repository::PostgresAuditLogEntryRepository;
pub use postgres_backup_code_repository::*;
pub use postgres_check_repository::*;
pub use postgres_diagnostic_report_repository::PostgresDiagnosticReportRepository;
pub use postgres_email_verification_token_repository::PostgresEmailVerificationTokenRepository;
pub use postgres_escalation_policy_repository::PostgresEscalationPolicyRepository;
pub use postgres_incident_repository::*;
pub use postgres_maintenance_window_repository::PostgresMaintenanceWindowRepository;
pub use postgres_monitor_repository::*;
pub use postgres_notification_channel_repository::*;
pub use postgres_notification_outbox_repository::PostgresNotificationOutboxRepository;
pub use postgres_proxy_check_job_repository::PostgresProxyCheckJobRepository;
pub use postgres_registration_transaction::PostgresRegistrationTransaction;
pub use postgres_reset_password_token_repository::PostgresResetPasswordTokenRepository;
pub use postgres_status_page_repository::PostgresStatusPageRepository;
pub use postgres_subscriber_repository::PostgresSubscriberRepository;
pub use postgres_tenant_invitation_repository::PostgresTenantInvitationRepository;
pub use postgres_tenant_membership_repository::PostgresTenantMembershipRepository;
pub use postgres_tenant_repository::PostgresTenantRepository;
pub use postgres_user_repository::*;
// H6 — Dashboard Overview:
pub use postgres_dashboard_repository::PostgresDashboardRepository;

// B11 — Plan / Subscription / Entitlement model:
pub mod in_memory_subscription_repository;
pub mod postgres_subscription_repository;

pub use in_memory_subscription_repository::InMemorySubscriptionRepository;
pub use postgres_subscription_repository::PostgresSubscriptionRepository;

// B14 — Billing Provider Abstraction & Checkout:
pub mod in_memory_checkout_session_repository;
pub mod postgres_checkout_session_repository;

pub use in_memory_checkout_session_repository::InMemoryCheckoutSessionRepository;
pub use postgres_checkout_session_repository::PostgresCheckoutSessionRepository;

// B15 — Billing Webhooks:
pub mod in_memory_billing_audit_log_repository;
pub mod in_memory_billing_webhook_idempotency_repository;
pub mod postgres_billing_audit_log_repository;
pub mod postgres_billing_webhook_idempotency_repository;

pub use in_memory_billing_audit_log_repository::InMemoryBillingAuditLogRepository;
pub use in_memory_billing_webhook_idempotency_repository::InMemoryBillingWebhookIdempotencyRepository;
pub use postgres_billing_audit_log_repository::PostgresBillingAuditLogRepository;
pub use postgres_billing_webhook_idempotency_repository::PostgresBillingWebhookIdempotencyRepository;

// B17 — Durable Scheduler:
pub mod postgres_scheduling_repository;
pub use postgres_scheduling_repository::PostgresSchedulingRepository;

// B20 — Agent Execution & mTLS:
pub mod in_memory_agent_lease_repository;
pub mod postgres_agent_lease_repository;

pub use in_memory_agent_lease_repository::InMemoryAgentLeaseRepository;
pub use postgres_agent_lease_repository::PostgresAgentLeaseRepository;

// B22 — History Retention Enforcement:
pub mod in_memory_history_retention_repository;
pub mod postgres_history_retention_repository;

pub use in_memory_history_retention_repository::InMemoryHistoryRetentionRepository;
pub use postgres_history_retention_repository::PostgresHistoryRetentionRepository;
pub mod in_memory_pricing_config_repository;
pub mod postgres_pricing_config_repository;
pub use postgres_pricing_config_repository::PostgresPricingConfigRepository;

// H12 — Proration Service:
pub mod in_memory_subscription_usage_period_repository;
pub mod in_memory_subscription_usage_transaction;
pub mod postgres_subscription_usage_period_repository;
pub mod postgres_subscription_usage_transaction;
pub use in_memory_subscription_usage_period_repository::InMemorySubscriptionUsagePeriodRepository;
pub use in_memory_subscription_usage_transaction::InMemorySubscriptionUsageTransaction;
pub use postgres_subscription_usage_period_repository::PostgresSubscriptionUsagePeriodRepository;
pub use postgres_subscription_usage_transaction::PostgresSubscriptionUsageTransaction;

pub mod in_memory_add_on_subscription_repository;
pub mod postgres_add_on_subscription_repository;
pub use in_memory_add_on_subscription_repository::InMemoryAddOnSubscriptionRepository;
pub use postgres_add_on_subscription_repository::PostgresAddOnSubscriptionRepository;

// H14 — Help Center & Support:
pub mod in_memory_help_article_repository;
pub mod in_memory_help_category_repository;
pub mod in_memory_support_ticket_repository;
pub mod postgres_help_article_repository;
pub mod postgres_help_category_repository;
pub mod postgres_support_ticket_repository;

pub use in_memory_help_article_repository::InMemoryHelpArticleRepository;
pub use in_memory_help_category_repository::InMemoryHelpCategoryRepository;
pub use in_memory_support_ticket_repository::InMemorySupportTicketRepository;
pub use postgres_help_article_repository::PostgresHelpArticleRepository;
pub use postgres_help_category_repository::PostgresHelpCategoryRepository;
pub use postgres_support_ticket_repository::PostgresSupportTicketRepository;

// RUM (Real User Monitoring):
pub mod postgres_rum_analytics_repository;
pub mod postgres_rum_event_repository;
pub mod postgres_rum_site_repository;
pub use postgres_rum_analytics_repository::PostgresRumAnalyticsRepository;
pub use postgres_rum_event_repository::PostgresRumEventRepository;
pub use postgres_rum_site_repository::PostgresRumSiteRepository;

pub mod in_memory_wallet_repository;
pub mod postgres_wallet_repository;
pub use in_memory_wallet_repository::InMemoryWalletRepository;
pub use postgres_wallet_repository::PostgresWalletRepository;

pub mod in_memory_payg_pricing_repository;
pub mod postgres_payg_pricing_repository;
pub mod in_memory_usage_event_repository;
pub mod postgres_usage_event_repository;
pub use in_memory_payg_pricing_repository::InMemoryPAYGPricingRepository;
pub use postgres_payg_pricing_repository::PostgresPAYGPricingRepository;
pub use in_memory_usage_event_repository::InMemoryUsageEventRepository;
pub use postgres_usage_event_repository::PostgresUsageEventRepository;

EOF_WORKER4

mkdir -p "$(dirname 'crates/infra/src/repositories/postgres_payg_pricing_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/infra/src/repositories/postgres_payg_pricing_repository.rs'
use application::ports::{PAYGPricingRepository, PaygPricingRepoResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::PAYGPricing;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresPAYGPricingRepository { pool: PgPool }
impl PostgresPAYGPricingRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}
#[derive(FromRow)]
struct Row {
    id: Uuid, version: i64, operation: String, unit_name: String, unit_price: i64,
    currency: String, effective_from: DateTime<Utc>, effective_until: Option<DateTime<Utc>>,
    active: bool,
}
impl Row {
    fn into_domain(self) -> PAYGPricing {
        PAYGPricing { id: self.id, version: self.version, operation: self.operation,
            unit_name: self.unit_name, unit_price: self.unit_price, currency: self.currency,
            effective_from: self.effective_from, effective_until: self.effective_until, active: self.active }
    }
}
#[async_trait]
impl PAYGPricingRepository for PostgresPAYGPricingRepository {
    async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
        let row = sqlx::query_as::<_, Row>(
            "SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active
             FROM payg_pricing_configs
             WHERE operation=$1 AND active=TRUE AND effective_from<=now()
               AND (effective_until IS NULL OR effective_until>now())
             ORDER BY version DESC LIMIT 1",
        ).bind(operation).fetch_optional(&self.pool).await?;
        Ok(row.map(Row::into_domain))
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/infra/src/repositories/postgres_usage_event_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/infra/src/repositories/postgres_usage_event_repository.rs'
use application::ports::{UsageEventRepoResult, UsageEventRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::UsageEvent;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresUsageEventRepository { pool: PgPool }
impl PostgresUsageEventRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

#[derive(FromRow)]
struct Row {
    id: Uuid, tenant_id: Uuid, operation: String, units_requested: i64, units_billed: i64,
    unit_price: i64, charge_units: i64, currency: String, pricing_version: i64,
    request_id: String, status: String, created_at: DateTime<Utc>,
}
impl Row {
    fn into_domain(self) -> UsageEvent {
        UsageEvent { id: self.id, tenant_id: self.tenant_id, operation: self.operation,
            units_requested: self.units_requested, units_billed: self.units_billed,
            unit_price: self.unit_price, charge_units: self.charge_units, currency: self.currency,
            pricing_version: self.pricing_version, request_id: self.request_id,
            status: self.status, created_at: self.created_at }
    }
}
#[async_trait]
impl UsageEventRepository for PostgresUsageEventRepository {
    async fn find_by_request(&self, tenant_id: Uuid, request_id: &str) -> UsageEventRepoResult<Option<UsageEvent>> {
        let row = sqlx::query_as::<_, Row>(
            "SELECT id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,
                    pricing_version,request_id,status,created_at
             FROM api_usage_events WHERE tenant_id=$1 AND request_id=$2",
        ).bind(tenant_id).bind(request_id).fetch_optional(&self.pool).await?;
        Ok(row.map(Row::into_domain))
    }
    async fn create(&self, event: &UsageEvent) -> UsageEventRepoResult<UsageEvent> {
        let row = sqlx::query_as::<_, Row>(
            "INSERT INTO api_usage_events
             (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,
              pricing_version,request_id,status)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
             ON CONFLICT(tenant_id,request_id) DO UPDATE
             SET request_id=EXCLUDED.request_id
             RETURNING id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,
                       pricing_version,request_id,status,created_at",
        ).bind(event.id).bind(event.tenant_id).bind(&event.operation)
         .bind(event.units_requested).bind(event.units_billed).bind(event.unit_price)
         .bind(event.charge_units).bind(&event.currency).bind(event.pricing_version)
         .bind(&event.request_id).bind(&event.status).fetch_one(&self.pool).await?;
        Ok(row.into_domain())
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'crates/infra/src/repositories/postgres_wallet_repository.rs')"
cat <<'EOF_WORKER4' > 'crates/infra/src/repositories/postgres_wallet_repository.rs'
use application::ports::{ReservationOutcome, WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{CreditReservation, Wallet, WalletTransaction, WalletTransactionType};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresWalletRepository {
    pool: PgPool,
}
impl PostgresWalletRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn parse_tx_type(v: &str) -> WalletTransactionType {
    match v {
        "TOP_UP" => WalletTransactionType::TopUp,
        "DEBIT" => WalletTransactionType::Debit,
        "REFUND" => WalletTransactionType::Refund,
        "ADJUSTMENT" => WalletTransactionType::Adjustment,
        "PROMOTIONAL_CREDIT" => WalletTransactionType::PromotionalCredit,
        "EXPIRATION" => WalletTransactionType::Expiration,
        _ => WalletTransactionType::Adjustment,
    }
}

#[derive(FromRow)]
struct WalletRow {
    id: Uuid, tenant_id: Uuid, currency: String, balance_units: i64,
    created_at: DateTime<Utc>, updated_at: DateTime<Utc>,
}
impl WalletRow {
    fn into_domain(self) -> Wallet {
        Wallet { id: self.id, tenant_id: self.tenant_id, currency: self.currency,
            balance_units: self.balance_units, created_at: self.created_at, updated_at: self.updated_at }
    }
}

#[derive(FromRow)]
struct TxRow {
    id: Uuid, tenant_id: Uuid, wallet_id: Uuid, transaction_type: String,
    amount_units: i64, balance_after_units: i64, idempotency_key: String,
    reference_type: Option<String>, reference_id: Option<String>,
    description: Option<String>, created_at: DateTime<Utc>,
}
impl TxRow {
    fn into_domain(self) -> WalletTransaction {
        WalletTransaction { id: self.id, tenant_id: self.tenant_id, wallet_id: self.wallet_id,
            transaction_type: parse_tx_type(&self.transaction_type), amount_units: self.amount_units,
            balance_after_units: self.balance_after_units, idempotency_key: self.idempotency_key,
            reference_type: self.reference_type, reference_id: self.reference_id,
            description: self.description, created_at: self.created_at }
    }
}

#[derive(FromRow)]
struct ReservationRow {
    id: Uuid, wallet_id: Uuid, amount_units: i64, status: String,
    operation: String, request_id: String, expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>, updated_at: DateTime<Utc>,
}
impl ReservationRow {
    fn into_domain(self) -> CreditReservation {
        CreditReservation { id: self.id, wallet_id: self.wallet_id, amount_units: self.amount_units,
            status: self.status, operation: self.operation, request_id: self.request_id,
            expires_at: self.expires_at, created_at: self.created_at, updated_at: self.updated_at }
    }
}

const TX_COLUMNS: &str =
    "id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description,created_at";

#[async_trait]
impl WalletRepository for PostgresWalletRepository {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO user_wallets(id,tenant_id,currency) VALUES($1,$2,$3)
             ON CONFLICT(tenant_id) DO NOTHING",
        )
        .bind(Uuid::new_v4()).bind(tenant_id).bind(currency)
        .execute(&mut *tx).await?;
        let row = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        ).bind(tenant_id).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }

    async fn list_transactions(&self, tenant_id: Uuid, limit: u32) -> WalletRepoResult<Vec<WalletTransaction>> {
        let query = format!(
            "SELECT t.{TX_COLUMNS} FROM wallet_transactions t
             WHERE t.tenant_id=$1 ORDER BY t.created_at DESC LIMIT $2"
        );
        let rows = sqlx::query_as::<_, TxRow>(&query)
            .bind(tenant_id).bind(limit.clamp(1, 100) as i64).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(TxRow::into_domain).collect())
    }

    async fn reserve(
        &self, tenant_id: Uuid, amount_units: i64, operation: &str,
        request_id: &str, expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        if amount_units <= 0 { return Err("reservation amount must be positive".into()); }
        if request_id.trim().is_empty() { return Err("request id is required".into()); }
        let mut tx = self.pool.begin().await?;
        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        ).bind(tenant_id).fetch_one(&mut *tx).await?;

        if let Some(existing) = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at
             FROM credit_reservations WHERE wallet_id=$1 AND request_id=$2",
        ).bind(wallet.id).bind(request_id).fetch_optional(&mut *tx).await? {
            tx.commit().await?;
            return Ok(ReservationOutcome { reservation: existing.into_domain(), created: false });
        }

        let reserved_units: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_units),0)::BIGINT
             FROM credit_reservations
             WHERE wallet_id=$1 AND status='PENDING' AND expires_at>now()",
        ).bind(wallet.id).fetch_one(&mut *tx).await?;
        let available = wallet.balance_units.checked_sub(reserved_units)
            .ok_or("wallet reservation accounting overflow")?;
        if available < amount_units { return Err("insufficient wallet balance".into()); }

        let row = sqlx::query_as::<_, ReservationRow>(
            "INSERT INTO credit_reservations
             (id,wallet_id,amount_units,status,operation,request_id,expires_at)
             VALUES($1,$2,$3,'PENDING',$4,$5,$6)
             RETURNING id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at",
        ).bind(Uuid::new_v4()).bind(wallet.id).bind(amount_units).bind(operation)
         .bind(request_id).bind(expires_at).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(ReservationOutcome { reservation: row.into_domain(), created: true })
    }

    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction> {
        let mut tx = self.pool.begin().await?;
        let r = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at
             FROM credit_reservations WHERE id=$1 FOR UPDATE",
        ).bind(reservation_id).fetch_optional(&mut *tx).await?
            .ok_or("reservation not found")?;

        let idem = format!("reservation:{reservation_id}");
        if r.status == CreditReservation::CAPTURED {
            let q = format!("SELECT {TX_COLUMNS} FROM wallet_transactions WHERE idempotency_key=$1");
            let row = sqlx::query_as::<_, TxRow>(&q).bind(&idem).fetch_one(&mut *tx).await?;
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        if r.status != CreditReservation::PENDING || r.expires_at <= Utc::now() {
            return Err("reservation is not capturable".into());
        }

        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE id=$1 FOR UPDATE",
        ).bind(r.wallet_id).fetch_one(&mut *tx).await?;
        let new_balance = wallet.balance_units.checked_sub(r.amount_units)
            .ok_or("insufficient wallet balance")?;
        sqlx::query(
            "UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1",
        ).bind(wallet.id).bind(new_balance).execute(&mut *tx).await?;
        sqlx::query(
            "UPDATE credit_reservations SET status='CAPTURED',updated_at=now() WHERE id=$1",
        ).bind(r.id).execute(&mut *tx).await?;

        let q = format!(
            "INSERT INTO wallet_transactions
             (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description)
             VALUES($1,$2,$3,'DEBIT',$4,$5,$6,'credit_reservation',$7,$8)
             RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4()).bind(wallet.tenant_id).bind(wallet.id)
            .bind(-r.amount_units).bind(new_balance).bind(&idem)
            .bind(r.id.to_string()).bind(r.operation)
            .fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }

    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        sqlx::query(
            "UPDATE credit_reservations SET status='RELEASED',updated_at=now()
             WHERE id=$1 AND status='PENDING'",
        ).bind(reservation_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn credit(
        &self, tenant_id: Uuid, amount_units: i64, transaction_type: WalletTransactionType,
        idempotency_key: &str, description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if amount_units <= 0 { return Err("credit amount must be positive".into()); }
        if idempotency_key.trim().is_empty() { return Err("idempotency key is required".into()); }
        let mut tx = self.pool.begin().await?;
        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        ).bind(tenant_id).fetch_one(&mut *tx).await?;
        let q = format!("SELECT {TX_COLUMNS} FROM wallet_transactions WHERE tenant_id=$1 AND idempotency_key=$2");
        if let Some(row) = sqlx::query_as::<_, TxRow>(&q).bind(tenant_id).bind(idempotency_key)
            .fetch_optional(&mut *tx).await? {
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        let new_balance = wallet.balance_units.checked_add(amount_units)
            .ok_or("wallet balance overflow")?;
        sqlx::query(
            "UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1",
        ).bind(wallet.id).bind(new_balance).execute(&mut *tx).await?;
        let q = format!(
            "INSERT INTO wallet_transactions
             (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,description)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4()).bind(tenant_id).bind(wallet.id)
            .bind(transaction_type.as_str()).bind(amount_units).bind(new_balance)
            .bind(idempotency_key).bind(description)
            .fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }
}

EOF_WORKER4

mkdir -p "$(dirname 'docs/WORKER4_PAYG.md')"
cat <<'EOF_WORKER4' > 'docs/WORKER4_PAYG.md'
# Worker 4 — PAYG domain, metering and pricing

This implementation was built from the source tree first, not from the existing
documentation. The repository already contained a partial wallet/reservation
foundation (`migrations/0074_wallet_payg_core.sql` and wallet repositories), so
Worker 4 completes and hardens that foundation instead of introducing a second
wallet system.

## Money model

All wallet/payment/PAYG monetary values are signed/unsigned integer units in
Postgres `BIGINT` and Rust `i64`. The initial PAYG price book defines one wallet
unit as one micro-USD (`10^-6 USD`). No financial calculation accepts `FLOAT`.

## Metering flow

`MeterUsageUseCase` delegates to `ChargeForUsageUseCase`:

1. authenticated application boundary supplies the tenant id;
2. validate operation, positive units and request id;
3. check usage-event idempotency;
4. load the server-side active PAYG price;
5. calculate `units * unit_price` with checked integer multiplication;
6. atomically reserve credits;
7. run the billable operation;
8. capture on success or release on failure;
9. persist an immutable usage event containing the pricing version.

The reservation locks the wallet row in PostgreSQL, so concurrent requests for
different request ids cannot overspend the same balance.

## Idempotency

Financial ledger rows require a tenant-scoped idempotency key. The database
enforces `(tenant_id, idempotency_key)`. Usage events enforce
`(tenant_id, request_id)`. Duplicate completed requests return the previously
persisted usage result.

## Immutability

Wallet ledger rows and usage events have database triggers that reject UPDATE
and DELETE. Application repositories also expose no mutation operation for
ledger rows.

## Security

- Client metering DTO contains only `operation`, `units`, and `request_id`.
- `serde(deny_unknown_fields)` rejects `price`, `total`, and `unit_price`.
- The server resolves prices and calculates charges.
- Insufficient balance is represented by `PAYG_INSUFFICIENT_BALANCE`; the API
  error layer provides HTTP 402 mapping.
- Wallet balance is constrained to `>= 0`.
- Wallet routes are authenticated and rate-limited.

EOF_WORKER4

mkdir -p "$(dirname 'migrations/0078_payg_domain_metering_pricing.sql')"
cat <<'EOF_WORKER4' > 'migrations/0078_payg_domain_metering_pricing.sql'
-- Worker 4: PAYG ledger, metering, versioned pricing, payment and invoice schema.
-- All monetary values are INTEGER/BIGINT smallest-currency units. No floating-point monetary types.
BEGIN;

-- Repair the original Worker-4 wallet ledger so the required tenant-scoped
-- idempotency invariant is explicit and database-enforced.
ALTER TABLE wallet_transactions
    ADD COLUMN IF NOT EXISTS tenant_id UUID;

UPDATE wallet_transactions wt
SET tenant_id = w.tenant_id
FROM user_wallets w
WHERE wt.wallet_id = w.id
  AND wt.tenant_id IS NULL;

UPDATE wallet_transactions
SET idempotency_key = 'legacy:' || id::text
WHERE idempotency_key IS NULL;

ALTER TABLE wallet_transactions
    ALTER COLUMN tenant_id SET NOT NULL,
    ALTER COLUMN idempotency_key SET NOT NULL;

ALTER TABLE wallet_transactions
    DROP CONSTRAINT IF EXISTS uq_wallet_transaction_idempotency;

ALTER TABLE wallet_transactions
    ADD CONSTRAINT fk_wallet_transactions_tenant
        FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;

CREATE UNIQUE INDEX IF NOT EXISTS uq_wallet_transactions_tenant_idempotency
    ON wallet_transactions(tenant_id, idempotency_key);

CREATE INDEX IF NOT EXISTS idx_wallet_transactions_tenant_created
    ON wallet_transactions(tenant_id, created_at DESC);

-- Financial ledger rows are immutable at the database boundary too.
CREATE OR REPLACE FUNCTION prevent_wallet_transaction_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'wallet_transactions are immutable';
END;
$$;

DROP TRIGGER IF EXISTS trg_wallet_transactions_immutable ON wallet_transactions;
CREATE TRIGGER trg_wallet_transactions_immutable
BEFORE UPDATE OR DELETE ON wallet_transactions
FOR EACH ROW EXECUTE FUNCTION prevent_wallet_transaction_mutation();

REVOKE UPDATE, DELETE ON wallet_transactions FROM PUBLIC;
GRANT INSERT, SELECT ON wallet_transactions TO PUBLIC;

-- Versioned PAYG unit prices. Each version is immutable; new pricing creates
-- a new version rather than modifying a historical price.
CREATE TABLE IF NOT EXISTS payg_pricing_configs (
    id UUID PRIMARY KEY,
    version BIGINT NOT NULL,
    operation TEXT NOT NULL,
    unit_name TEXT NOT NULL,
    unit_price BIGINT NOT NULL CHECK (unit_price >= 0),
    currency TEXT NOT NULL,
    effective_from TIMESTAMPTZ NOT NULL,
    effective_until TIMESTAMPTZ,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_payg_pricing_window
        CHECK (effective_until IS NULL OR effective_until > effective_from),
    CONSTRAINT uq_payg_pricing_version_operation UNIQUE (version, operation)
);

CREATE INDEX IF NOT EXISTS idx_payg_pricing_lookup
    ON payg_pricing_configs(operation, active, effective_from DESC);

CREATE OR REPLACE FUNCTION prevent_payg_pricing_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'payg_pricing_configs are immutable';
END;
$$;

DROP TRIGGER IF EXISTS trg_payg_pricing_immutable ON payg_pricing_configs;
CREATE TRIGGER trg_payg_pricing_immutable
BEFORE UPDATE OR DELETE ON payg_pricing_configs
FOR EACH ROW EXECUTE FUNCTION prevent_payg_pricing_mutation();

-- Usage is an immutable audit record. The existing API usage table is the
-- durable metering event store; these columns pin the exact price used.
ALTER TABLE api_usage_events
    ADD COLUMN IF NOT EXISTS unit_price BIGINT NOT NULL DEFAULT 0 CHECK (unit_price >= 0),
    ADD COLUMN IF NOT EXISTS charge_units BIGINT NOT NULL DEFAULT 0 CHECK (charge_units >= 0),
    ADD COLUMN IF NOT EXISTS currency TEXT NOT NULL DEFAULT 'USD',
    ADD COLUMN IF NOT EXISTS pricing_version BIGINT NOT NULL DEFAULT 0;

ALTER TABLE api_usage_events
    DROP CONSTRAINT IF EXISTS uq_api_usage_request;
ALTER TABLE api_usage_events
    ADD CONSTRAINT uq_api_usage_events_tenant_request UNIQUE (tenant_id, request_id);

CREATE INDEX IF NOT EXISTS idx_api_usage_tenant_created
    ON api_usage_events(tenant_id, created_at DESC);

CREATE OR REPLACE FUNCTION prevent_api_usage_event_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'api_usage_events are immutable';
END;
$$;

DROP TRIGGER IF EXISTS trg_api_usage_events_immutable ON api_usage_events;
CREATE TRIGGER trg_api_usage_events_immutable
BEFORE UPDATE OR DELETE ON api_usage_events
FOR EACH ROW EXECUTE FUNCTION prevent_api_usage_event_mutation();

REVOKE UPDATE, DELETE ON api_usage_events FROM PUBLIC;
GRANT INSERT, SELECT ON api_usage_events TO PUBLIC;

-- Payment schema only in Worker 4. Provider integration is Worker 5.
CREATE TABLE IF NOT EXISTS payments (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    amount_units BIGINT NOT NULL CHECK (amount_units > 0),
    currency TEXT NOT NULL,
    status TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_payment_id TEXT,
    idempotency_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_payments_tenant_idempotency UNIQUE (tenant_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_payments_tenant_created
    ON payments(tenant_id, created_at DESC);

-- Invoice schema only. Worker 6 owns the invoice engine.
CREATE TABLE IF NOT EXISTS invoices (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    number TEXT NOT NULL,
    status TEXT NOT NULL,
    currency TEXT NOT NULL,
    subtotal_units BIGINT NOT NULL CHECK (subtotal_units >= 0),
    total_units BIGINT NOT NULL CHECK (total_units >= 0),
    issued_at TIMESTAMPTZ,
    due_at TIMESTAMPTZ,
    paid_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_invoices_tenant_number UNIQUE (tenant_id, number)
);

CREATE INDEX IF NOT EXISTS idx_invoices_tenant_created
    ON invoices(tenant_id, created_at DESC);

COMMIT;

EOF_WORKER4

mkdir -p "$(dirname 'migrations/0079_seed_payg_pricing_v1.sql')"
cat <<'EOF_WORKER4' > 'migrations/0079_seed_payg_pricing_v1.sql'
-- Initial PAYG price book.
-- One wallet unit is one micro-USD (10^-6 USD), stored as BIGINT.
-- Clients submit usage units only; the server resolves these prices.
INSERT INTO payg_pricing_configs
    (id, version, operation, unit_name, unit_price, currency, effective_from, effective_until, active)
VALUES
    (gen_random_uuid(), 1, 'proxy_check',    'check',      2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'monitor_check',  'check',      2,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'SMS',            'message',    1000, 'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'rum_event',      'event',      1,    'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'storage',        'MB-month',   20,   'USD', now(), NULL, TRUE),
    (gen_random_uuid(), 1, 'network_egress', 'MB',         100,  'USD', now(), NULL, TRUE)
ON CONFLICT (version, operation) DO NOTHING;

EOF_WORKER4

echo "[Worker 4] Source files written."

if command -v cargo >/dev/null 2>&1; then
  echo "[Worker 4] Formatting Rust sources..."
  cargo fmt --all

  echo "[Worker 4] Running domain/application/infra/API tests..."
  cargo test -p domain
  cargo test -p application
  cargo test -p infra
  cargo test -p api

  echo "[Worker 4] Running workspace check (excluding browser dashboard test execution)..."
  cargo check --workspace

  echo "[Worker 4] Rust tests/checks passed."
else
  echo "ERROR: cargo was not found. Install Rust/Cargo, then rerun this script." >&2
  exit 127
fi

if command -v sqlx >/dev/null 2>&1 && [[ -n "${DATABASE_URL:-}" ]]; then
  echo "[Worker 4] Applying database migrations..."
  sqlx migrate run --source migrations
  echo "[Worker 4] Database migrations passed."
else
  echo "[Worker 4] WARNING: sqlx CLI or DATABASE_URL is missing; database migrations were NOT executed."
  echo "[Worker 4] Run: DATABASE_URL='...' sqlx migrate run --source migrations"
fi

echo "[Worker 4] DONE. If all commands above passed, Worker 4 is ready for Worker 5."
