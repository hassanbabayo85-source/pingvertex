//! Handler-level tests for the restored PAYG add-on billing surface.
//!
//! Pattern mirrors `payg_pricing_entitlement.rs`: handlers are invoked
//! directly with `State(app_state)` + `Extension(user)`. This sidesteps
//! the JWT middleware (which would require a fully seeded AuthState with
//! an active tenant + membership) — a dependency that has nothing to do
//! with the add-on logic under test.

mod common;

use api::handlers::add_on_billing::{
    CancelAddOnRequest, CreateAddOnRequest, VerifyAddOnRequest, billing_summary, cancel_add_on,
    create_add_on, verify_add_on,
};
use api::state::AppState;
use application::ports::{
    AddOnSubscriptionRepository, SmsProviderPricing, TenantRetentionSettingsRepository,
    WalletRepository,
};
use async_trait::async_trait;
use axum::Json;
use axum::extract::{Extension, State};
use domain::entities::RetentionSource;
use domain::pricing::PricingConfig;
use domain::shared::ids::{TenantId, UserId};
use domain::shared::types::{AuthenticatedUser, Role, TenantContext};
use domain::{AddOnStatus, AddOnType};
use infra::FakeBillingProvider;
use infra::InMemoryAddOnSubscriptionRepository;
use infra::InMemoryApiFlowRepository;
use infra::InMemoryAuditLogEntryRepository;
use infra::InMemoryCheckoutSessionRepository;
use infra::InMemoryFinancialAuditLogRepository;
use infra::InMemoryMonitorRepository;
use infra::InMemoryNotificationChannelRepository;
use infra::InMemoryProxyCheckJobRepository;
use infra::InMemoryTenantRepository;
use infra::InMemoryTenantRetentionSettingsRepository;
use infra::InMemoryWalletRepository;
use infra::NoopNotificationService;
use infra::UuidIdGenerator;
use infra::repositories::in_memory_pricing_config_repository::InMemoryPricingConfigRepository;
use rust_decimal::Decimal;
use std::sync::Arc;

/// Deterministic fake `SmsProviderPricing` so tests don't depend on Twilio.
/// Returns $0.50/segment in USD; with the default 2.00 markup this yields
/// a $1.00/segment final price.
#[derive(Clone, Default)]
struct TestSmsPricing;

#[async_trait]
impl SmsProviderPricing for TestSmsPricing {
    async fn outbound_sms_unit_price(
        &self,
        _iso_country: &str,
    ) -> Result<(Decimal, String), Box<dyn std::error::Error + Send + Sync>> {
        Ok((Decimal::new(50, 2), "USD".to_string()))
    }
}

fn build_state(
    addon_repo: Arc<InMemoryAddOnSubscriptionRepository>,
    checkout_repo: Arc<InMemoryCheckoutSessionRepository>,
    retention_repo: Arc<InMemoryTenantRetentionSettingsRepository>,
    wallet_repo: Arc<InMemoryWalletRepository>,
) -> AppState {
    let monitor_repo = InMemoryMonitorRepository::new();
    let pricing_repo = Arc::new(
        InMemoryPricingConfigRepository::new().with_config(PricingConfig::default_config()),
    );
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
    .with_pricing_config_repository(pricing_repo)
    .with_add_on_subscription_repository(addon_repo)
    .with_checkout_session_repository(checkout_repo)
    .with_tenant_retention_settings_repository(retention_repo)
    .with_wallet_repository(wallet_repo)
    .with_payment_provider(FakeBillingProvider)
    .with_sms_provider_pricing(TestSmsPricing)
    .with_financial_audit_log_repository(InMemoryFinancialAuditLogRepository::new())
    .with_platform_admin_repositories(
        common::NoopUserRepository,
        InMemoryTenantRepository::new(),
    )
}

struct Fixture {
    state: AppState,
    tenant_id: TenantId,
    user: AuthenticatedUser,
    addon_repo: Arc<InMemoryAddOnSubscriptionRepository>,
    retention_repo: Arc<InMemoryTenantRetentionSettingsRepository>,
    wallet_repo: Arc<InMemoryWalletRepository>,
}

fn fixture() -> Fixture {
    let addon_repo = Arc::new(InMemoryAddOnSubscriptionRepository::new());
    let checkout_repo = Arc::new(InMemoryCheckoutSessionRepository::new());
    let retention_repo = Arc::new(InMemoryTenantRetentionSettingsRepository::new());
    let wallet_repo = Arc::new(InMemoryWalletRepository::default());

    let state = build_state(
        addon_repo.clone(),
        checkout_repo,
        retention_repo.clone(),
        wallet_repo.clone(),
    );

    let tenant_id = TenantId::new();
    let user = AuthenticatedUser::new(UserId::new(), Role::Owner, TenantContext::new(tenant_id));

    Fixture {
        state,
        tenant_id,
        user,
        addon_repo,
        retention_repo,
        wallet_repo,
    }
}

#[tokio::test]
async fn create_sms_add_on_without_subscription() {
    let fx = fixture();

    let Json(resp) = create_add_on(
        State(fx.state.clone()),
        Extension(fx.user.clone()),
        Json(CreateAddOnRequest {
            addon_type: "sms".into(),
            quantity: 100,
            country_code: Some("US".into()),
            tier: None,
        }),
    )
    .await
    .expect("SMS add-on must be accepted without any subscription");

    assert_eq!(resp.addon_type, "sms");
    assert_eq!(resp.quantity, 100);
    // Compare numerically — rust_decimal scales multiply, so the string
    // form here is "1.0000" / "100.0000" rather than the truncated form.
    assert_eq!(
        resp.unit_price.parse::<Decimal>().unwrap(),
        Decimal::new(100, 2),
        "0.50 provider * 2.00 markup must yield a $1.00 unit price"
    );
    assert_eq!(
        resp.total_price.parse::<Decimal>().unwrap(),
        Decimal::new(10000, 2),
        "1.00 * 100 segments must yield $100.00"
    );
    assert_eq!(resp.currency, "USD");
    assert_eq!(resp.status, "pending");
    assert!(resp.checkout_session_id.is_some());
    assert!(resp.checkout_url.is_some());

    let stored = fx
        .addon_repo
        .find_by_id(domain::AddOnSubscriptionId(resp.id.parse().unwrap()))
        .await
        .unwrap()
        .expect("add-on row must be persisted");
    assert_eq!(stored.status, AddOnStatus::Pending);
    assert_eq!(stored.tenant_id, fx.tenant_id);
    assert_eq!(stored.addon_type, AddOnType::SmsCredits);
    assert_eq!(stored.quantity, 100);
}

#[tokio::test]
async fn create_enterprise_sla_add_on() {
    let fx = fixture();

    let Json(resp) = create_add_on(
        State(fx.state.clone()),
        Extension(fx.user.clone()),
        Json(CreateAddOnRequest {
            addon_type: "enterprise".into(),
            quantity: 2,
            country_code: None,
            tier: None,
        }),
    )
    .await
    .expect("enterprise SLA add-on must be accepted");

    assert_eq!(resp.addon_type, "enterprise");
    assert_eq!(resp.quantity, 2);
    assert_eq!(resp.unit_price, "99.00");
    assert_eq!(resp.total_price, "198.00");
    assert_eq!(resp.status, "pending");
    assert!(resp.checkout_session_id.is_some());
}

#[tokio::test]
async fn create_extended_retention_add_on_90() {
    let fx = fixture();

    let Json(resp) = create_add_on(
        State(fx.state.clone()),
        Extension(fx.user.clone()),
        Json(CreateAddOnRequest {
            addon_type: "retention".into(),
            quantity: 1,
            country_code: None,
            tier: Some("90".into()),
        }),
    )
    .await
    .expect("extended-retention add-on must be accepted");

    assert_eq!(resp.addon_type, "retention");
    assert_eq!(resp.quantity, 1);
    assert_eq!(resp.tier.as_deref(), Some("90"));
    assert_eq!(resp.status, "pending");
    assert!(resp.checkout_session_id.is_some());
    assert_eq!(resp.currency, "USD");
}

#[tokio::test]
async fn verify_extended_retention_updates_settings() {
    let fx = fixture();

    let Json(created) = create_add_on(
        State(fx.state.clone()),
        Extension(fx.user.clone()),
        Json(CreateAddOnRequest {
            addon_type: "retention".into(),
            quantity: 1,
            country_code: None,
            tier: Some("180".into()),
        }),
    )
    .await
    .expect("create retention add-on");

    let session_id: uuid::Uuid = created
        .checkout_session_id
        .as_deref()
        .expect("create must return a checkout session id")
        .parse()
        .unwrap();

    let Json(body) = verify_add_on(
        State(fx.state.clone()),
        Extension(fx.user.clone()),
        Json(VerifyAddOnRequest {
            checkout_session_id: session_id,
        }),
    )
    .await
    .expect("verify must succeed");

    assert_eq!(body["status"], "completed");
    assert_eq!(
        body["addon_subscription_id"].as_str(),
        Some(created.id.as_str())
    );

    let settings = fx
        .retention_repo
        .get(fx.tenant_id)
        .await
        .unwrap()
        .expect("retention settings row must exist after verify");
    assert_eq!(settings.retention_days, 180);
    assert_eq!(settings.source, RetentionSource::Addon);

    let stored = fx
        .addon_repo
        .find_by_id(domain::AddOnSubscriptionId(created.id.parse().unwrap()))
        .await
        .unwrap()
        .expect("add-on row must still exist");
    assert_eq!(stored.status, AddOnStatus::Active);
}

#[tokio::test]
async fn cancel_extended_retention_reverts_to_default() {
    let fx = fixture();

    let Json(created) = create_add_on(
        State(fx.state.clone()),
        Extension(fx.user.clone()),
        Json(CreateAddOnRequest {
            addon_type: "retention".into(),
            quantity: 1,
            country_code: None,
            tier: Some("365".into()),
        }),
    )
    .await
    .unwrap();

    let session_id: uuid::Uuid = created
        .checkout_session_id
        .as_deref()
        .unwrap()
        .parse()
        .unwrap();
    let _ = verify_add_on(
        State(fx.state.clone()),
        Extension(fx.user.clone()),
        Json(VerifyAddOnRequest {
            checkout_session_id: session_id,
        }),
    )
    .await
    .unwrap();

    let pre = fx.retention_repo.get(fx.tenant_id).await.unwrap().unwrap();
    assert_eq!(pre.retention_days, 365);
    assert_eq!(pre.source, RetentionSource::Addon);

    let Json(canceled) = cancel_add_on(
        State(fx.state.clone()),
        Extension(fx.user.clone()),
        Json(CancelAddOnRequest {
            addon_subscription_id: created.id.parse().unwrap(),
        }),
    )
    .await
    .expect("cancel must succeed");

    assert_eq!(canceled.status, "canceled");

    // Option A: cancel must DELETE the custom retention row so that
    // `effective_retention` falls back to the pricing-config default.
    let post = fx.retention_repo.get(fx.tenant_id).await.unwrap();
    assert!(
        post.is_none(),
        "cancel must remove the custom retention row; found {post:?}"
    );

    let effective = fx
        .retention_repo
        .effective_retention(fx.tenant_id, 30, 3650)
        .await
        .unwrap();
    assert_eq!(
        effective, 30,
        "after cancel, effective retention must fall back to the pricing-config default"
    );
}

#[tokio::test]
async fn billing_summary_shows_wallet_and_addons() {
    let fx = fixture();

    let addon = domain::AddOnSubscription::new(
        domain::AddOnSubscriptionId::new(),
        fx.tenant_id,
        AddOnType::EnterpriseSla,
        1,
        None,
        Decimal::new(9900, 2),
        "USD".into(),
        AddOnStatus::Active,
    )
    .unwrap();
    fx.addon_repo.create(&addon).await.unwrap();

    // Ensure a wallet row exists (balance = 0).
    fx.wallet_repo
        .get_or_create(fx.tenant_id.0, "USD")
        .await
        .unwrap();

    let Json(summary) = billing_summary(State(fx.state.clone()), Extension(fx.user.clone()))
        .await
        .expect("billing summary must succeed");

    assert_eq!(summary.wallet_currency, "USD");
    assert_eq!(summary.currency, "USD");
    assert_eq!(summary.wallet_balance_units, 0);
    assert_eq!(summary.active_add_ons.len(), 1);
    assert_eq!(summary.active_add_ons[0].addon_type, "enterprise");
    assert_eq!(summary.add_ons_total, "99.00");
}
