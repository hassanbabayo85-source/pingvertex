//! Task 9B — permission enforcement for subscription billing handlers.
//!
//! Task 6 shipped `subscription_billing.rs` without permission checks.
//! The 9B audit added `require_manage_billing` / `require_view_billing`
//! to the handlers (matching `add_on_billing.rs`). These tests confirm
//! the fix works: `Admin` (no ManageBilling) and `Operator` (monitor-only)
//! are rejected, and `Owner` passes the permission gate.

mod common;

use std::sync::Arc;

use api::error::ApiError;
use api::handlers::subscription_billing::{
    cancel_subscription_endpoint, get_subscription, initiate_subscription_checkout,
    verify_subscription_checkout, CancelSubscriptionBody, InitiateSubscriptionCheckoutBody,
};
use api::state::AppState;
use axum::Json;
use axum::extract::{Extension, Path, State};
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use infra::{
    InMemoryApiFlowRepository, InMemoryAuditLogEntryRepository, InMemoryCheckoutSessionRepository,
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository,
    InMemoryPlanPriceRepository, InMemoryProxyCheckJobRepository,
    InMemorySubscriptionBillingRecordRepository, InMemorySubscriptionRepository,
    NoopNotificationService, UuidIdGenerator,
};

fn build_minimal_state() -> AppState {
    let monitor_repo = InMemoryMonitorRepository::new();
    AppState::new(
        monitor_repo.clone(),
        common::NoopCheckRepository,
        common::NoopDiagnosticReportRepository,
        common::NoopIncidentRepository,
        InMemoryNotificationChannelRepository::new(monitor_repo),
        NoopNotificationService::new(),
        InMemoryApiFlowRepository::new(),
        InMemoryProxyCheckJobRepository::new(),
        UuidIdGenerator::new(),
        Arc::new(common::NoopSecretCipher),
        InMemoryAuditLogEntryRepository::new(),
    )
    .with_plan_price_repository(InMemoryPlanPriceRepository::default())
    .with_checkout_session_repository(InMemoryCheckoutSessionRepository::new())
    .with_subscription_repository(InMemorySubscriptionRepository::new())
    .with_subscription_billing_record_repository(InMemorySubscriptionBillingRecordRepository::new())
}

fn user_with_role(role: Role) -> AuthenticatedUser {
    AuthenticatedUser::new(UserId::new(), role, TenantContext::new(TenantId::new()))
}

fn initiate_payload() -> Json<InitiateSubscriptionCheckoutBody> {
    Json(InitiateSubscriptionCheckoutBody {
        plan_tier: "starter".to_owned(),
        currency: "NGN".to_owned(),
        billing_interval: "monthly".to_owned(),
    })
}

// ────────────────────────────────────────────────────────────────
// ManageBilling required for initiate / verify / cancel
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn initiate_requires_manage_billing() {
    let state = build_minimal_state();
    // Admin has CreateMonitor / ManageUsers but NOT ManageBilling.
    let user = user_with_role(Role::Admin);

    let err = initiate_subscription_checkout(State(state), Extension(user), initiate_payload())
        .await
        .expect_err("Admin without ManageBilling must be rejected");

    assert!(
        matches!(err, ApiError::Forbidden(_)),
        "expected Forbidden, got {err:?}"
    );
}

#[tokio::test]
async fn verify_requires_manage_billing() {
    let state = build_minimal_state();
    let user = user_with_role(Role::Admin);
    // The permission check runs before the session lookup, so any UUID works.
    let session_id = uuid::Uuid::new_v4().to_string();

    let err = verify_subscription_checkout(State(state), Extension(user), Path(session_id))
        .await
        .expect_err("Admin without ManageBilling must be rejected");

    assert!(
        matches!(err, ApiError::Forbidden(_)),
        "expected Forbidden, got {err:?}"
    );
}

#[tokio::test]
async fn cancel_requires_manage_billing() {
    let state = build_minimal_state();
    let user = user_with_role(Role::Admin);

    let err = cancel_subscription_endpoint(
        State(state),
        Extension(user),
        Json(CancelSubscriptionBody::default()),
    )
    .await
    .expect_err("Admin without ManageBilling must be rejected");

    assert!(
        matches!(err, ApiError::Forbidden(_)),
        "expected Forbidden, got {err:?}"
    );
}

// ────────────────────────────────────────────────────────────────
// ViewBilling required for get_subscription
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_requires_view_billing() {
    let state = build_minimal_state();
    // Operator only has monitor permissions — no ViewBilling, no ManageBilling.
    let user = user_with_role(Role::Operator);

    let err = get_subscription(State(state), Extension(user))
        .await
        .expect_err("Operator without ViewBilling must be rejected");

    assert!(
        matches!(err, ApiError::Forbidden(_)),
        "expected Forbidden, got {err:?}"
    );
}

// ────────────────────────────────────────────────────────────────
// Owner passes the permission gate
// ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn owner_passes_manage_billing_gate() {
    let state = build_minimal_state();
    let user = user_with_role(Role::Owner);

    // The Noop / InMemory state cannot complete a real checkout, but the
    // permission gate must NOT block. Any error other than Forbidden
    // proves the gate was passed.
    let result =
        initiate_subscription_checkout(State(state), Extension(user), initiate_payload()).await;

    if let Err(err) = &result {
        assert!(
            !matches!(err, ApiError::Forbidden(_)),
            "Owner must pass the permission gate; got {err:?}"
        );
    }
}

#[tokio::test]
async fn owner_passes_view_billing_gate() {
    let state = build_minimal_state();
    let user = user_with_role(Role::Owner);

    let result = get_subscription(State(state), Extension(user)).await;

    if let Err(err) = &result {
        assert!(
            !matches!(err, ApiError::Forbidden(_)),
            "Owner must pass the permission gate; got {err:?}"
        );
    }
}
