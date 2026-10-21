//! Task 6 — subscription checkout + lifecycle HTTP surface.
//!
//! All four endpoints are additive on `/billing/*`; they coexist with
//! the PAYG surface in `billing.rs::router()`. Wallet and PAYG paths
//! are untouched.

use std::str::FromStr;

use axum::{
    extract::{Extension, Json, Path, State},
    http::StatusCode,
};
use chrono::Utc;
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{
    DomainError,
    enums::PlanTier,
    money::{BillingInterval, Currency},
    shared::ids::CheckoutSessionId,
};
use serde::{Deserialize, Serialize};

use application::commands::CancelSubscription;
use application::use_cases::cancel_subscription::CancelSubscriptionUseCase;
use application::use_cases::initiate_subscription_checkout::{
    InitiateSubscriptionCheckoutCommand, InitiateSubscriptionCheckoutError,
    InitiateSubscriptionCheckoutUseCase,
};
use application::use_cases::subscription_lifecycle_support::SubscriptionLifecycleError;
use application::use_cases::verify_subscription_checkout::{
    VerifySubscriptionCheckoutCommand, VerifySubscriptionCheckoutError,
    VerifySubscriptionCheckoutUseCase,
};

use crate::{error::ApiError, state::AppState};

// ────────────────────────────────────────────────────────────────
// Permission helpers — Task 9B security audit fix
// ────────────────────────────────────────────────────────────────
//
// Task 6 originally shipped these handlers without permission checks.
// The audit found that any tenant member (including Operator / Viewer)
// could initiate, verify, and cancel a subscription. The pattern below
// matches `add_on_billing::require_manage_billing` and is applied
// inside the handler (not the use case) so the Task 6 use-case tests
// keep working unchanged. A future cleanup can move this into the use
// cases (matching `create_monitor.rs`, `delete_monitor.rs`, and
// `create_api_key.rs`) without changing the observable HTTP behaviour.

fn require_manage_billing(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if user.can(Permission::ManageBilling) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "billing management permission is required",
        ))
    }
}

fn require_view_billing(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if user.can(Permission::ViewBilling) {
        Ok(())
    } else {
        Err(ApiError::forbidden("billing view permission is required"))
    }
}

// ────────────────────────────────────────────────────────────────
// POST /billing/subscription/checkout
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InitiateSubscriptionCheckoutBody {
    pub plan_tier: String,
    pub currency: String,
    pub billing_interval: String,
}

#[derive(Debug, Serialize)]
pub struct InitiateSubscriptionCheckoutResponse {
    pub checkout_session_id: String,
    pub checkout_url: String,
    pub amount_minor: i64,
    pub currency: String,
    pub billing_interval: String,
}

pub async fn initiate_subscription_checkout(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(payload): Json<InitiateSubscriptionCheckoutBody>,
) -> Result<(StatusCode, Json<InitiateSubscriptionCheckoutResponse>), ApiError> {
    require_manage_billing(&authenticated_user)?;
    let tenant_id = authenticated_user.tenant_context().tenant_id();
    let user_id = authenticated_user.user_id();

    // Requirement 6: customer email must come from the authenticated
    // account record, never from the request body.
    let user = state
        .platform_user_repository
        .find_by_id(&user_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load user for checkout", e))?
        .ok_or_else(|| ApiError::internal("authenticated user not found"))?;
    let customer_email = user.email().as_str().to_owned();

    let plan_tier: PlanTier = payload
        .plan_tier
        .parse()
        .map_err(|e: DomainError| ApiError::bad_request(format!("invalid plan_tier: {e}")))?;
    let currency = Currency::from_str(&payload.currency)
        .map_err(|e| ApiError::bad_request(format!("invalid currency: {e}")))?;
    let billing_interval = BillingInterval::from_str(&payload.billing_interval)
        .map_err(|e| ApiError::bad_request(format!("invalid billing_interval: {e}")))?;

    let use_case = InitiateSubscriptionCheckoutUseCase::new(
        state.plan_price_repository.clone(),
        state.checkout_session_repository.clone(),
        state.payment_provider.clone(),
    );
    let outcome = use_case
        .execute(InitiateSubscriptionCheckoutCommand {
            tenant_id,
            plan_tier,
            currency,
            billing_interval,
            customer_email,
        })
        .await
        .map_err(map_initiate_error)?;

    Ok((
        StatusCode::CREATED,
        Json(InitiateSubscriptionCheckoutResponse {
            checkout_session_id: outcome.checkout_session_id.0.to_string(),
            checkout_url: outcome.checkout_url,
            amount_minor: outcome.amount_minor,
            currency: outcome.currency.as_str().to_owned(),
            billing_interval: outcome.billing_interval.as_str().to_owned(),
        }),
    ))
}

fn map_initiate_error(err: InitiateSubscriptionCheckoutError) -> ApiError {
    match err {
        InitiateSubscriptionCheckoutError::FreePlanNotAllowed => {
            ApiError::bad_request("the free plan does not require checkout")
        }
        InitiateSubscriptionCheckoutError::EnterpriseIsContactSalesOnly => ApiError::bad_request(
            "Enterprise pricing is by custom quote; contact sales to subscribe",
        ),
        InitiateSubscriptionCheckoutError::NativeRecurringUnavailable(msg) => {
            ApiError::conflict(format!("native recurring unavailable: {msg}"))
        }
        InitiateSubscriptionCheckoutError::PriceNotFound => {
            ApiError::not_found("no effective price configured for this tier / currency / interval")
        }
        InitiateSubscriptionCheckoutError::Invalid(msg) => ApiError::bad_request(msg),
        InitiateSubscriptionCheckoutError::Provider(e) => {
            ApiError::internal_with_source("payment provider error", e)
        }
        InitiateSubscriptionCheckoutError::Repository(e) => {
            ApiError::internal_with_source("subscription checkout repository error", e)
        }
    }
}

// ────────────────────────────────────────────────────────────────
// POST /billing/subscription/checkout/{id}/verify
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct VerifySubscriptionCheckoutResponse {
    pub checkout_status: String,
}

pub async fn verify_subscription_checkout(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Path(checkout_session_id): Path<String>,
) -> Result<Json<VerifySubscriptionCheckoutResponse>, ApiError> {
    require_manage_billing(&authenticated_user)?;
    let tenant_id = authenticated_user.tenant_context().tenant_id();
    let session_id = uuid::Uuid::parse_str(&checkout_session_id)
        .map_err(|_| ApiError::bad_request("invalid checkout_session_id"))?;

    let use_case = VerifySubscriptionCheckoutUseCase::new(
        state.checkout_session_repository.clone(),
        state.subscription_repository.clone(),
        state.payment_provider.clone(),
        state.subscription_activation_repository.clone(),
    );
    let outcome = use_case
        .execute(VerifySubscriptionCheckoutCommand {
            tenant_id,
            checkout_session_id: CheckoutSessionId(session_id),
        })
        .await
        .map_err(map_verify_error)?;

    Ok(Json(VerifySubscriptionCheckoutResponse {
        checkout_status: outcome.checkout_status.as_str().to_owned(),
    }))
}

fn map_verify_error(err: VerifySubscriptionCheckoutError) -> ApiError {
    match err {
        VerifySubscriptionCheckoutError::CheckoutSessionNotFound => {
            ApiError::not_found("checkout session not found")
        }
        VerifySubscriptionCheckoutError::CheckoutNotReady => {
            ApiError::bad_request("checkout session has no provider reference yet")
        }
        VerifySubscriptionCheckoutError::Invalid(msg) => ApiError::bad_request(msg),
        VerifySubscriptionCheckoutError::AmountMismatch {
            expected_minor,
            got_minor,
        } => ApiError::conflict(format!(
            "amount mismatch: expected {expected_minor} minor units, provider reported {got_minor}"
        )),
        VerifySubscriptionCheckoutError::CurrencyMismatch { expected, got } => ApiError::conflict(
            format!("currency mismatch: expected {expected:?}, provider reported {got:?}"),
        ),
        VerifySubscriptionCheckoutError::Subscription(e) => {
            ApiError::conflict(format!("subscription error: {e}"))
        }
        VerifySubscriptionCheckoutError::Provider(e) => {
            ApiError::internal_with_source("payment provider error", e)
        }
        VerifySubscriptionCheckoutError::Repository(e) => {
            ApiError::internal_with_source("subscription checkout repository error", e)
        }
    }
}

// ────────────────────────────────────────────────────────────────
// GET /billing/subscription
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    pub plan_tier: String,
    pub status: String,
    pub current_period_ends_at: Option<String>,
    pub cancel_at_period_end: bool,
    pub canceled_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GetSubscriptionResponse {
    pub subscription: Option<SubscriptionResponse>,
}

pub async fn get_subscription(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
) -> Result<Json<GetSubscriptionResponse>, ApiError> {
    require_view_billing(&authenticated_user)?;
    let tenant_id = authenticated_user.tenant_context().tenant_id();
    let mut sub = state
        .subscription_repository
        .find_by_tenant_id(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load subscription", e))?;

    // Task 6 has no native recurring: lazily finalize an elapsed period
    // on read. This is a no-op unless status == Active and the period end
    // has elapsed.
    if let Some(s) = sub.as_mut() {
        let changed = s
            .finalize_period_end(Utc::now())
            .map_err(|e| ApiError::internal_with_source("failed to finalize period", e))?
            .is_some();
        if changed {
            state.subscription_repository.update(s).await.map_err(|e| {
                ApiError::internal_with_source("failed to persist finalized subscription", e)
            })?;
        }
    }

    Ok(Json(GetSubscriptionResponse {
        subscription: sub.map(|s| SubscriptionResponse {
            plan_tier: s.plan_tier.as_str().to_owned(),
            status: s.status.as_str().to_owned(),
            current_period_ends_at: s.current_period_ends_at.map(|d| d.to_rfc3339()),
            cancel_at_period_end: s.cancel_at_period_end,
            canceled_at: s.canceled_at.map(|d| d.to_rfc3339()),
        }),
    }))
}

// ────────────────────────────────────────────────────────────────
// POST /billing/subscription/cancel
// ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct CancelSubscriptionBody {
    /// Default `true` — safer for the customer (keeps access until the
    /// period ends). Pass `false` to cancel immediately.
    #[serde(default = "default_at_period_end")]
    pub at_period_end: bool,
}

fn default_at_period_end() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct CancelSubscriptionResponse {
    pub status: String,
    pub cancel_at_period_end: bool,
    pub current_period_ends_at: Option<String>,
}

pub async fn cancel_subscription_endpoint(
    State(state): State<AppState>,
    Extension(authenticated_user): Extension<AuthenticatedUser>,
    Json(body): Json<CancelSubscriptionBody>,
) -> Result<Json<CancelSubscriptionResponse>, ApiError> {
    require_manage_billing(&authenticated_user)?;
    let tenant_id = authenticated_user.tenant_context().tenant_id();

    // Delegates to the pre-existing use case; it correctly reads the
    // period end from the subscription, never from a client-supplied
    // timestamp. We pass `current_period_ends_at: None` on purpose.
    let use_case = CancelSubscriptionUseCase::new(state.subscription_repository.clone());
    use_case
        .execute(CancelSubscription {
            tenant_id,
            at_period_end: body.at_period_end,
            current_period_ends_at: None,
        })
        .await
        .map_err(|err| match err {
            SubscriptionLifecycleError::SubscriptionNotFound => {
                ApiError::not_found("no subscription on this tenant")
            }
            SubscriptionLifecycleError::InvalidTransition(e) => ApiError::conflict(format!("{e}")),
            SubscriptionLifecycleError::Repository(e) => {
                ApiError::internal_with_source("failed to cancel subscription", e)
            }
        })?;

    let sub = state
        .subscription_repository
        .find_by_tenant_id(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to reload subscription", e))?
        .ok_or_else(|| ApiError::internal("subscription disappeared after cancel"))?;

    Ok(Json(CancelSubscriptionResponse {
        status: sub.status.as_str().to_owned(),
        cancel_at_period_end: sub.cancel_at_period_end,
        current_period_ends_at: sub.current_period_ends_at.map(|d| d.to_rfc3339()),
    }))
}
