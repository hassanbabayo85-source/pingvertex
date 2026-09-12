//! Task 15 acceptance test: scheduler billing context survival.
//!
//! The scheduler path builds a trusted `PricingContext` (account_type,
//! region, monitor_kind, quantity) from the monitor and tenant records it
//! already loaded, forwards it through metering, and reserves wallet
//! credit at the resolved contextual price.
//!
//! This test proves the context actually reaches the wallet by seeding
//! two pricing rules — one generic, one for a specific
//! `(account_type, region, monitor_kind)` tuple — and asserting the
//! wallet is debited at the contextual price, not the generic one.
//!
//! The pipeline under test is exactly what `durable_scheduler.rs`
//! executes at runtime: `PricingContext` → `resolve_price` →
//! `reserve()` pins `unit_price` + `pricing_version` into
//! `credit_reservations`.

use application::ports::{
    PAYGPricingRepository, PricingContext, PricingRuleContext, WalletRepository,
};
use application::use_cases::charge_for_usage::{
    ChargeForUsageInput, ChargeForUsageUseCase, MeteredAttempt,
};
use chrono::{Duration as ChronoDuration, Utc};
use domain::{AccountType, MonitorKind, RegionCode, WalletTransactionType};
use infra::repositories::in_memory_payg_pricing_repository::InMemoryPAYGPricingRepository;
use infra::{InMemoryUsageEventRepository, InMemoryWalletRepository};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn scheduler_context_resolves_contextual_price_not_generic() {
    // -------- Seed two pricing rules ---------------------------------
    let pricing_repo = Arc::new(InMemoryPAYGPricingRepository::new());
    let effective_from = Utc::now() - ChronoDuration::minutes(1);

    // Generic fallback at 2 units per check.
    pricing_repo
        .create_version_with_context(
            "http_check.monitoring",
            "check",
            2,
            "USD",
            effective_from,
            &PricingRuleContext::default(),
        )
        .await
        .unwrap();

    // Contextual override: Company + us-east + http = 100 units per check.
    let region = RegionCode::new("us-east").unwrap();
    let contextual = PricingRuleContext {
        account_type: Some(AccountType::Company),
        region: Some(region.clone()),
        monitor_kind: Some(MonitorKind::Http),
        min_quantity: None,
        max_quantity: None,
    };
    pricing_repo
        .create_version_with_context(
            "http_check.monitoring",
            "check",
            100,
            "USD",
            effective_from,
            &contextual,
        )
        .await
        .unwrap();

    // -------- Wallet for a company tenant ----------------------------
    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
    let tenant_id = Uuid::new_v4();
    wallet_repo.get_or_create(tenant_id, "USD").await.unwrap();
    wallet_repo
        .credit(
            tenant_id,
            1_000_000,
            WalletTransactionType::TopUp,
            "seed",
            Some("seed balance"),
        )
        .await
        .unwrap();

    // -------- Drive the scheduler-shaped charge path -----------------
    let usage_repo = Arc::new(InMemoryUsageEventRepository::default());
    let use_case = ChargeForUsageUseCase::new(
        wallet_repo.clone(),
        pricing_repo.clone(),
        usage_repo.clone(),
    );

    let pricing_context = PricingContext {
        account_type: Some(AccountType::Company),
        region: Some(region),
        monitor_kind: Some(MonitorKind::Http),
        quantity: 1,
    };

    let request_id = format!("sched-{}", Uuid::new_v4());
    let input = ChargeForUsageInput {
        tenant_id,
        operation: "http_check.monitoring".to_owned(),
        units: 1,
        request_id: request_id.clone(),
        monitor_id: None,
        execution_id: None,
        pricing_context: Some(pricing_context),
    };

    let (result, attempt) = use_case
        .execute_classified_attempt(input, || async {
            Ok::<_, std::convert::Infallible>(MeteredAttempt {
                attempted: true,
                error: None,
                egress_bytes: 0,
            })
        })
        .await
        .expect("charge must succeed");

    assert!(attempt.attempted);

    // -------- Assertions ---------------------------------------------
    // Usage event must record the contextual unit price (100), not 2.
    assert_eq!(
        result.usage_event.unit_price, 100,
        "usage event must record the contextual unit price (100), not the generic one (2)"
    );

    // Wallet must be debited by exactly 100 units (1 check × 100 units).
    let wallet = wallet_repo.find(tenant_id).await.unwrap().unwrap();
    assert_eq!(
        wallet.balance_units,
        1_000_000 - 100,
        "wallet must be debited at the contextual price"
    );

    // NOTE: the assertion that the reservation row pins the same
    // (unit_price, pricing_version) is intentionally NOT made here.
    // `InMemoryWalletRepository` does not implement
    // `find_reservation_by_request` (the trait provides a default
    // `Ok(None)`), so this test cannot observe the reservation row
    // directly. The two assertions above already prove the observable
    // contract: the usage event recorded the contextual price, and the
    // wallet was debited at that price. The same invariant is covered
    // end-to-end against a real PostgreSQL database by
    // `crates/infra/tests/postgres_payg_overlap_and_wallet.rs`.
}
