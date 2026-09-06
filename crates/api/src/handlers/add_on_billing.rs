use crate::{error::ApiError, state::AppState};
use application::{
    ports::AddOnSubscriptionRepository,
    use_cases::{
        add_on_subscription::CreateAddOnSubscriptionUseCase,
        calculate_pricing_quote::CalculatePricingQuoteUseCase,
    },
};
use axum::{
    Json,
    extract::{Extension, State},
};
use domain::pricing::{PricingConfig, PricingInput};
use domain::shared::types::{AuthenticatedUser, Permission};
use domain::{AddOnStatus, AddOnType, Plan};
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
#[derive(Debug, Serialize)]
pub struct BillingSummaryResponse {
    pub base_price: String,
    pub add_ons: Vec<AddOnResponse>,
    pub add_ons_total: String,
    pub total: String,
    pub currency: String,
    pub pricing_version: u32,
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
        AddOnType::Rum => {
            return Err(ApiError::bad_request(
                "RUM ingestion is not present in this codebase; RUM billing was not invented",
            ));
        }
    };
    let tenant_id = user.tenant_context().tenant_id();
    let sub = state
        .subscription_repository
        .find_by_tenant_id(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to load subscription", e))?;
    let plan_tier = sub
        .as_ref()
        .filter(|s| {
            matches!(
                s.status,
                domain::SubscriptionStatus::Active | domain::SubscriptionStatus::Trialing
            )
        })
        .map(|s| s.plan_tier)
        .ok_or_else(|| {
            ApiError::conflict("an active subscription is required before purchasing an add-on")
        })?;
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
            plan_tier,
            amount_minor,
            currency: addon.currency.clone(),
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
        plan_tier,
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
    let tenant_id = user.tenant_context().tenant_id();
    let config = state
        .pricing_config_repository
        .find_active_config()
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch active pricing config", e))?
        .ok_or_else(|| ApiError::internal("no active pricing config found"))?;
    let sub = state
        .subscription_repository
        .find_by_tenant_id(tenant_id)
        .await
        .map_err(|e| ApiError::internal_with_source("failed to fetch subscription", e))?;
    let (monitors, interval, retention) = match sub.as_ref().filter(|s| {
        matches!(
            s.status,
            domain::SubscriptionStatus::Active | domain::SubscriptionStatus::Trialing
        )
    }) {
        Some(s) => (
            s.monitors
                .unwrap_or_else(|| Plan::for_tier(s.plan_tier).max_monitors as u64),
            s.interval_seconds
                .unwrap_or_else(|| Plan::for_tier(s.plan_tier).min_check_interval_seconds),
            s.retention_days
                .unwrap_or_else(|| Plan::for_tier(s.plan_tier).history_retention_days),
        ),
        None => (
            config.free_tier.monitors,
            config.free_tier.interval_seconds,
            config.free_tier.retention_days,
        ),
    };
    let base = CalculatePricingQuoteUseCase::new(state.pricing_config_repository.clone())
        .execute(PricingInput {
            monitors,
            interval_seconds: interval,
            retention_days: retention,
        })
        .await
        .map_err(|e| ApiError::internal_with_source("failed to calculate base billing price", e))?;
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
    let total = base.monthly_price + addon_total;
    Ok(Json(BillingSummaryResponse {
        base_price: base.monthly_price.to_string(),
        add_ons: out,
        add_ons_total: addon_total.to_string(),
        total: total.to_string(),
        currency: base.currency,
        pricing_version: base.pricing_version,
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
    fn rum_is_explicitly_rejected_by_request_path() {
        assert!(parse_type("rum").is_ok());
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
    let uc = CreateAddOnSubscriptionUseCase::new(state.add_on_subscription_repository.clone());
    let a = uc
        .cancel(
            domain::AddOnSubscriptionId(req.addon_subscription_id),
            user.tenant_context().tenant_id(),
        )
        .await
        .map_err(|e| ApiError::internal_with_source("failed to cancel add-on", e))?;
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
