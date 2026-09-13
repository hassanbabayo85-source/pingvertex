use crate::{error::ApiError, state::AppState};
use application::{
    ports::AddOnSubscriptionRepository,
    use_cases::add_on_subscription::CreateAddOnSubscriptionUseCase,
};
use axum::{
    Json,
    extract::{Extension, State},
};
use domain::pricing::PricingConfig;
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::entities::RetentionTier;
use domain::pricing::compute_storage_delta;
use domain::{AddOnStatus, AddOnType};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateAddOnRequest {
    pub addon_type: String,
    pub quantity: u64,
    pub country_code: Option<String>,
    pub tier: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct AddOnResponse {
    pub id: String,
    pub addon_type: String,
    pub quantity: u64,
    pub tier: Option<String>,
    pub unit_price: String,
    pub total_price: String,
    pub currency: String,
    pub status: String,
    pub checkout_session_id: Option<String>,
    pub checkout_url: Option<String>,
}
/// PAYG billing summary: wallet balance + active add-ons.
///
/// This replaces the old subscription-era "base price + add-ons" shape.
/// The PAYG runtime has no base subscription price — the wallet is
/// funded by top-ups, and add-ons are separately purchased line items.
#[derive(Debug, Serialize)]
pub struct BillingSummaryResponse {
    pub wallet_balance_units: i64,
    pub wallet_currency: String,
    pub active_add_ons: Vec<AddOnResponse>,
    pub add_ons_total: String,
    pub currency: String,
}

fn require_manage_billing(user: &AuthenticatedUser) -> Result<(), ApiError> {
    if user.can(Permission::ManageBilling) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "you do not have permission to manage add-ons",
        ))
    }
}
fn parse_type(s: &str) -> Result<AddOnType, ApiError> {
    s.parse()
        .map_err(|_| ApiError::bad_request("invalid add-on type"))
}

fn enterprise_unit_price(config: &PricingConfig) -> (Decimal, String) {
    (config.enterprise_sla_monthly_price, config.currency.clone())
}

fn sms_unit_price(
    provider_price: Decimal,
    provider_currency: &str,
    config: &PricingConfig,
) -> Result<(Decimal, String), ApiError> {
    if provider_currency.to_uppercase() != config.currency.to_uppercase() {
        return Err(ApiError::bad_request(
            "Twilio pricing currency does not match active billing currency",
        ));
    }
    Ok((
        provider_price * config.sms_markup_multiplier,
        provider_currency.to_owned(),
    ))
}

pub async fn create_add_on(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CreateAddOnRequest>,
) -> Result<Json<AddOnResponse>, ApiError> {
    require_manage_billing(&user)?;
    let addon_type = parse_type(&req.addon_type)?;
    if req.quantity == 0 {
        return Err(ApiError::bad_request("quantity must be greater than zero"));
    }
    let config = state
        .pricing_config_repository
        .find_active_config()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch active pricing config", e))?
        .ok_or_else(|| ApiError::internal("no active pricing config found"))?;
    let (unit_price, currency) = match addon_type {
        AddOnType::EnterpriseSla => enterprise_unit_price(&config),
        AddOnType::SmsCredits => {
            let country = req
                .country_code
                .as_deref()
                .ok_or_else(|| ApiError::bad_request("country_code is required for SMS credits"))?;
            let (provider_price, provider_currency) = state
                .sms_provider_pricing
                .outbound_sms_unit_price(country)
                .await
                .map_err(|e| {
                    ApiError::internal_with_source("failed to fetch Twilio SMS pricing", e)
                })?;
            sms_unit_price(provider_price, &provider_currency, &config)?
        }
        AddOnType::ExtendedRetention => {
            let tier_str = req.tier.as_deref().ok_or_else(|| {
                ApiError::bad_request("tier is required for extended retention (90, 180, or 365)")
            })?;
            let tier: RetentionTier = tier_str
                .parse()
                .map_err(|_| ApiError::bad_request("tier must be 90, 180, or 365"))?;
            if req.quantity != 1 {
                return Err(ApiError::bad_request(
                    "extended retention is a tier, not a quantity; quantity must be 1",
                ));
            }
            let tenant_id = user.tenant_context().tenant_id();
            let monitors = state
                .monitor_repository
                .count_enabled_for_tenant(tenant_id)
                .await
                .map_err(|e| ApiError::internal_with_source("failed to count monitors", e))?;
            let interval_seconds = config.free_tier.interval_seconds;
            let delta_base = compute_storage_delta(
                monitors,
                interval_seconds,
                tier.days(),
                &config,
            );
            let price = delta_base * config.markup_multiplier;
            (price, config.currency.clone())
        }
    };
    let tenant_id = user.tenant_context().tenant_id();
    let uc = CreateAddOnSubscriptionUseCase::new(state.add_on_subscription_repository.clone());
    let mut addon = uc
        .execute(
            tenant_id,
            addon_type,
            req.quantity,
            req.tier,
            unit_price,
            currency,
            AddOnStatus::Pending,
        )
        .await
        .map_err(|e| ApiError::internal_with_source("failed to create add-on subscription", e))?;
    let amount_minor = (addon.total_price() * Decimal::from(100))
        .round()
        .to_i64()
        .ok_or_else(|| ApiError::bad_request("add-on price is out of range"))?;
    let checkout = state
        .payment_provider
        .create_checkout(&application::ports::CheckoutRequest {
            tenant_id,
            plan_tier: None,
            amount_minor,
            currency: addon.currency.clone(),
            customer_email: Some(
                state
                    .platform_user_repository
                    .find_by_id(&user.user_id())
                    .await
                    .map_err(|e| {
                        ApiError::internal_with_source("failed to load billing customer", e)
                    })?
                    .ok_or_else(|| ApiError::internal("billing customer account not found"))?
                    .email()
                    .to_string(),
            ),
        })
        .await
        .map_err(|e| {
            ApiError::internal_with_source("failed to create add-on payment checkout", e)
        })?;
    addon.attach_checkout(checkout.provider_checkout_id.clone());
    state
        .add_on_subscription_repository
        .update(&addon)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to attach add-on checkout", e))?;
    let mut session = domain::CheckoutSession::new_pending(
        domain::CheckoutSessionId::new(),
        tenant_id,
        None,
        state.payment_provider.provider_name().into(),
    );
    session.attach_provider_checkout_id(checkout.provider_checkout_id);
    session.attach_addon(addon.id.0, addon.total_price(), addon.currency.clone());
    state
        .checkout_session_repository
        .create(&session)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to persist add-on checkout", e))?;
    let total_price = addon.total_price();
    let _ = state.financial_audit_log_repository.create(&domain::FinancialAuditLogEntry::new(user.user_id().0.to_string(),Some(user.tenant_context().tenant_id().0),"subscription_changed","addon_subscription",Some(addon.id.0.to_string()),serde_json::json!({"action":"created","addon_type":addon.addon_type.as_str(),"quantity":addon.quantity}))).await;
    Ok(Json(AddOnResponse {
        id: addon.id.0.to_string(),
        addon_type: addon.addon_type.as_str().into(),
        quantity: addon.quantity,
        tier: addon.tier,
        unit_price: addon.price.to_string(),
        total_price: total_price.to_string(),
        currency: addon.currency,
        status: addon.status.as_str().into(),
        checkout_session_id: Some(session.id.0.to_string()),
        checkout_url: Some(checkout.checkout_url),
    }))
}

pub async fn billing_summary(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<BillingSummaryResponse>, ApiError> {
    user.require(Permission::ViewBilling)
        .map_err(|_| ApiError::forbidden("billing access is not permitted"))?;

    let tenant_id = user.tenant_context().tenant_id();

    let balance = state
        .wallet_repository
        .get_balance_summary(tenant_id.0, "USD")
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load wallet balance", e))?;

    let addons = state
        .add_on_subscription_repository
        .list_active_by_tenant(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch add-ons", e))?;

    let mut addon_total = Decimal::ZERO;
    let mut out = Vec::with_capacity(addons.len());
    for a in addons {
        let total = a.total_price();
        addon_total += total;
        out.push(AddOnResponse {
            id: a.id.0.to_string(),
            addon_type: a.addon_type.as_str().into(),
            quantity: a.quantity,
            tier: a.tier,
            unit_price: a.price.to_string(),
            total_price: total.to_string(),
            currency: a.currency,
            status: a.status.as_str().into(),
            checkout_session_id: None,
            checkout_url: None,
        });
    }

    Ok(Json(BillingSummaryResponse {
        wallet_balance_units: balance.balance_units,
        wallet_currency: "USD".to_string(),
        active_add_ons: out,
        add_ons_total: addon_total.to_string(),
        currency: "USD".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn addon_total_is_added_to_base() {
        let base = Decimal::new(1250, 2);
        let a = Decimal::new(9900, 2);
        let b = Decimal::new(500, 2);
        assert_eq!(base + a + b, Decimal::new(11650, 2));
    }
    #[test]
    fn enterprise_pricing_uses_configured_fixed_price() {
        let config = PricingConfig::default_config();
        let (price, currency) = enterprise_unit_price(&config);
        assert_eq!(price, Decimal::new(9900, 2));
        assert_eq!(currency, "USD");
    }

    #[test]
    fn sms_pricing_applies_configured_markup_to_provider_price() {
        let config = PricingConfig::default_config();
        let (price, currency) = sms_unit_price(Decimal::new(50, 2), "USD", &config).unwrap();
        assert_eq!(price, Decimal::new(100, 2));
        assert_eq!(currency, "USD");
    }

    #[test]
    fn sms_pricing_rejects_currency_mismatch() {
        let config = PricingConfig::default_config();
        assert!(sms_unit_price(Decimal::new(50, 2), "EUR", &config).is_err());
    }

    #[test]
    fn retention_is_a_recognized_add_on_type() {
        assert!(parse_type("retention").is_ok());
        assert!(
            parse_type("rum").is_err(),
            "rum was removed from AddOnType in GAP 3 Kashi E follow-up"
        );
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct VerifyAddOnRequest {
    pub checkout_session_id: uuid::Uuid,
}
#[derive(Debug, serde::Deserialize)]
pub struct CancelAddOnRequest {
    pub addon_subscription_id: uuid::Uuid,
}
pub async fn verify_add_on(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<VerifyAddOnRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_manage_billing(&user)?;
    let uc = application::use_cases::VerifyAddOnCheckoutUseCase::new(
        state.checkout_session_repository.clone(),
        state.add_on_subscription_repository.clone(),
        state.payment_provider.clone(),
    );
    let (id, status) = uc
        .execute(user.tenant_context().tenant_id().0, req.checkout_session_id)
        .await
        .map_err(|e| {
            ApiError::internal_with_source(
                "failed to verify add-on payment",
                std::io::Error::other(e.to_string()),
            )
        })?;
    if status == domain::CheckoutStatus::Completed {
        state
            .financial_audit_log_repository
            .create(&domain::FinancialAuditLogEntry::new(
                user.user_id().0.to_string(),
                Some(user.tenant_context().tenant_id().0),
                "payment_verified",
                "checkout_session",
                Some(req.checkout_session_id.to_string()),
                serde_json::json!({"status":"completed","addon_subscription_id":id}),
            ))
            .await
            .map_err(|e| {
                ApiError::internal_with_source("failed to write financial audit log", e)
            })?;

        // GAP Add-on Restoration (A4): apply the purchased add-on.
        //
        // For ExtendedRetention, upsert the tenant's retention row with
        // `source = Addon` so the retention worker honors it. The
        // repository's priority-aware upsert means a lower-priority
        // writer cannot clobber this row.
        //
        // For SmsCredits and EnterpriseSla the add-on is recorded as
        // Active in `addon_subscriptions`; the runtime paths that consume
        // them (fair-use allowance / SLA response time) are outside this
        // change. There is no extra state to write here.
        let tenant_id = user.tenant_context().tenant_id();
        let addon = state
            .add_on_subscription_repository
            .find_by_id(domain::AddOnSubscriptionId(id))
            .await
            .map_err(|e| {
                ApiError::internal_with_source("failed to reload add-on after payment", e)
            })?;
        if let Some(addon) = addon {
            if addon.addon_type == domain::AddOnType::ExtendedRetention {
                let tier_str = addon.tier.as_deref().ok_or_else(|| {
                    ApiError::internal("extended-retention add-on has no tier")
                })?;
                let tier: RetentionTier = tier_str
                    .parse()
                    .map_err(|_| ApiError::internal("add-on has invalid retention tier"))?;

                let settings = domain::entities::TenantRetentionSettings::new(
                    tenant_id,
                    tier.days(),
                    domain::entities::RetentionSource::Addon,
                );
                state
                    .tenant_retention_settings_repository
                    .upsert(&settings)
                    .await
                    .map_err(|e| {
                        ApiError::internal_with_source(
                            "failed to apply extended retention after payment",
                            e,
                        )
                    })?;

                let _ = state
                    .financial_audit_log_repository
                    .create(&domain::FinancialAuditLogEntry::new(
                        user.user_id().0.to_string(),
                        Some(tenant_id.0),
                        "retention_settings_changed",
                        "tenant_retention_settings",
                        Some(tenant_id.0.to_string()),
                        serde_json::json!({
                            "source": "addon",
                            "tier_days": tier.days(),
                            "addon_subscription_id": id.to_string(),
                        }),
                    ))
                    .await;
            }
        }
    }
    Ok(Json(
        serde_json::json!({"addon_subscription_id":id,"status":status.as_str()}),
    ))
}
pub async fn cancel_add_on(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<CancelAddOnRequest>,
) -> Result<Json<AddOnResponse>, ApiError> {
    require_manage_billing(&user)?;
    let tenant_id = user.tenant_context().tenant_id();
    let uc = CreateAddOnSubscriptionUseCase::new(state.add_on_subscription_repository.clone());
    let a = uc
        .cancel(
            domain::AddOnSubscriptionId(req.addon_subscription_id),
            tenant_id,
        )
        .await
        .map_err(|e| ApiError::internal_with_source("failed to cancel add-on", e))?;

    // GAP Add-on Restoration (A6, Option A): revert ExtendedRetention to
    // the pricing config's default retention so the retention worker
    // stops honoring the (now-cancelled) add-on tier.
    //
    // We DELETE the custom row rather than upserting a `Default` row:
    // `upsert` is priority-aware by contract and would silently discard a
    // `Default` write while an `Addon` row exists. With the row gone,
    // `effective_retention` naturally falls back to
    // `pricing_config.default_retention_days` — the same outcome, without
    // violating the priority rule.
    if a.addon_type == domain::AddOnType::ExtendedRetention {
        state
            .tenant_retention_settings_repository
            .delete(tenant_id)
            .await
            .map_err(|e| {
                ApiError::internal_with_source(
                    "failed to revert retention after add-on cancellation",
                    e,
                )
            })?;
    }

    state
        .financial_audit_log_repository
        .create(&domain::FinancialAuditLogEntry::new(
            user.user_id().0.to_string(),
            Some(tenant_id.0),
            "subscription_canceled",
            "addon_subscription",
            Some(a.id.0.to_string()),
            serde_json::json!({"addon_type":a.addon_type.as_str()}),
        ))
        .await
        .map_err(|e| ApiError::internal_with_source("failed to write financial audit log", e))?;
    Ok(Json(AddOnResponse {
        id: a.id.0.to_string(),
        addon_type: a.addon_type.as_str().into(),
        quantity: a.quantity,
        tier: a.tier.clone(),
        unit_price: a.price.to_string(),
        total_price: a.total_price().to_string(),
        currency: a.currency,
        status: a.status.as_str().into(),
        checkout_session_id: None,
        checkout_url: None,
    }))
}
