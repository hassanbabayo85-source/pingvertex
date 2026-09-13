//! GAP 3 Kashi E: session-based RUM billing.
//!
//! Tests the reserve → finalize lifecycle directly on
//! `ChargeForUsageUseCase`:
//!
//!   * new session      -> finalize(attempted = true)  -> one debit
//!   * extended session -> finalize(attempted = false) -> no debit
//!   * retry with same session_id -> no double charge
//!   * empty wallet + new session -> InsufficientBalance at reserve time

use application::ports::{PAYGPricingRepository, WalletRepository};
use application::use_cases::charge_for_usage::{
    ChargeForUsageError, ChargeForUsageInput, ChargeForUsageUseCase,
};
use chrono::{Duration, Utc};
use domain::WalletTransactionType;
use infra::{
    InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
};
use std::sync::Arc;
use uuid::Uuid;

const SESSION_PRICE_NANO_USD: i64 = 1_000;

async fn setup(
    balance_units: i64,
) -> (
    Arc<InMemoryWalletRepository>,
    Arc<InMemoryPAYGPricingRepository>,
    Arc<InMemoryUsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    pricing
        .create_version(
            "rum_session",
            "session",
            SESSION_PRICE_NANO_USD,
            "USD",
            Utc::now() - Duration::minutes(1),
        )
        .await
        .unwrap();
    let usage = Arc::new(InMemoryUsageEventRepository::default());

    let tenant = Uuid::new_v4();
    wallet.get_or_create(tenant, "USD").await.unwrap();
    if balance_units > 0 {
        wallet
            .credit(
                tenant,
                balance_units,
                WalletTransactionType::TopUp,
                "seed",
                None,
            )
            .await
            .unwrap();
    }

    (wallet, pricing, usage, tenant)
}

fn input(tenant: Uuid, session_id: Uuid) -> ChargeForUsageInput {
    ChargeForUsageInput {
        tenant_id: tenant,
        operation: "rum_session".to_string(),
        units: 1,
        request_id: format!("rum_session:{session_id}"),
        monitor_id: None,
        execution_id: None,
        pricing_context: None,
    }
}

#[tokio::test]
async fn new_session_is_billed_exactly_once() {
    let (wallet, pricing, usage, tenant) = setup(1_000_000).await;
    let charge = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);

    let session_id = Uuid::new_v4();
    let inp = input(tenant, session_id);

    charge.reserve_for_execution(inp.clone()).await.unwrap();
    charge.finalize_reserved_attempt(inp, true).await.unwrap();

    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(
        w.balance_units,
        1_000_000 - SESSION_PRICE_NANO_USD,
        "new session must be billed once"
    );
}

#[tokio::test]
async fn extended_session_is_not_billed() {
    let (wallet, pricing, usage, tenant) = setup(1_000_000).await;
    let charge = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);

    let session_id = Uuid::new_v4();
    let inp = input(tenant, session_id);

    // Reserve → finalize with attempted = false (extended session).
    charge.reserve_for_execution(inp.clone()).await.unwrap();
    let result = charge.finalize_reserved_attempt(inp, false).await;

    // finalize_reserved_attempt returns OperationNotAttempted when it
    // releases without capturing.
    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationNotAttempted(_))
    ));

    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(w.balance_units, 1_000_000, "extended session must not debit");
}

#[tokio::test]
async fn retrying_same_session_id_does_not_double_charge() {
    let (wallet, pricing, usage, tenant) = setup(1_000_000).await;
    let charge = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);

    let session_id = Uuid::new_v4();
    let inp = input(tenant, session_id);

    // First attempt: billed.
    charge.reserve_for_execution(inp.clone()).await.unwrap();
    charge
        .finalize_reserved_attempt(inp.clone(), true)
        .await
        .unwrap();

    let after_first = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(after_first.balance_units, 999_000);

    // Retry with the same session_id: must be a no-op.
    charge.reserve_for_execution(inp.clone()).await.unwrap();
    charge.finalize_reserved_attempt(inp, true).await.unwrap();

    let after_retry = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(
        after_retry.balance_units, after_first.balance_units,
        "retrying the same session_id must not double-charge"
    );
}

#[tokio::test]
async fn empty_wallet_fails_reservation() {
    let (wallet, pricing, usage, tenant) = setup(0).await;
    let charge = ChargeForUsageUseCase::new(wallet, pricing, usage);

    let session_id = Uuid::new_v4();
    let inp = input(tenant, session_id);

    let err = charge.reserve_for_execution(inp).await.unwrap_err();
    assert!(
        matches!(err, ChargeForUsageError::InsufficientBalance),
        "expected InsufficientBalance, got {err:?}"
    );
}
