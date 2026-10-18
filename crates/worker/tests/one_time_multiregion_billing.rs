//! GAP 1 acceptance test: a one-time check fans out to N regional
//! executions, each carrying its own durable `execution_id`. Billing
//! must be per execution_id: retrying one region must never double-
//! charge, a pre-attempt failure must release its reservation, an
//! insufficient balance on one region must not prevent other regions
//! from being charged, and pricing must resolve server-side using the
//! region that produced the execution.

use application::ports::{
    PAYGPricingRepository, PricingContext, PricingRuleContext, WalletRepository,
};
use application::use_cases::charge_for_usage::{
    ChargeForUsageError, ChargeForUsageInput, ChargeForUsageUseCase, MeteredAttempt,
};
use chrono::{Duration as ChronoDuration, Utc};
use domain::{AccountType, MonitorKind, RegionCode, WalletTransactionType};
use std::sync::Arc;
use uuid::Uuid;

// NOTE: adjust the two `_Alias` imports above only if compilation tells
// us the real names differ; the helper below already routes through the
// repository's public trait.

async fn seed_pricing(
    pricing: &Arc<infra::InMemoryPAYGPricingRepository>,
    operation: &str,
    unit_price: i64,
    region: Option<&str>,
) {
    let ctx = match region {
        Some(r) => PricingRuleContext {
            account_type: None,
            region: Some(RegionCode::new(r).unwrap()),
            monitor_kind: Some(MonitorKind::Http),
            min_quantity: None,
            max_quantity: None,
        },
        None => PricingRuleContext::default(),
    };
    pricing
        .create_version_with_context(
            operation,
            "check",
            unit_price,
            "USD",
            Utc::now() - ChronoDuration::minutes(1),
            &ctx,
        )
        .await
        .unwrap();
}

fn one_time_input(
    tenant_id: Uuid,
    region: &str,
    execution_pk: Uuid,
    execution_id: Uuid,
) -> ChargeForUsageInput {
    ChargeForUsageInput {
        tenant_id,
        operation: "http_check.one_time".into(),
        units: 1,
        request_id: format!("one-time:{execution_pk}:{execution_id}"),
        monitor_id: Some(Uuid::new_v4()),
        execution_id: Some(execution_id),
        pricing_context: Some(PricingContext {
            monitor_kind: Some(MonitorKind::Http),
            region: Some(RegionCode::new(region).unwrap()),
            quantity: 1,
            account_type: Some(AccountType::Individual),
        }),
    }
}

async fn setup(
    unit_price: i64,
) -> (
    Arc<infra::InMemoryWalletRepository>,
    Arc<infra::InMemoryPAYGPricingRepository>,
    Arc<infra::InMemoryUsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(infra::InMemoryWalletRepository::default());
    let pricing = Arc::new(infra::InMemoryPAYGPricingRepository::new());
    seed_pricing(&pricing, "http_check.one_time", unit_price, None).await;
    let usage = Arc::new(infra::InMemoryUsageEventRepository::default());
    (wallet, pricing, usage, Uuid::new_v4())
}

#[tokio::test]
async fn three_regional_executions_charge_three_times() {
    let (wallet, pricing, usage, tenant) = setup(2).await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(tenant, 1000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());

    let mut charged = 0u32;
    for region in ["us-east", "eu-west", "af-west"] {
        let pk = Uuid::new_v4();
        let eid = Uuid::new_v4();
        let input = one_time_input(tenant, region, pk, eid);
        let (_result, _attempt) = uc
            .execute_classified_attempt(input, || async {
                Ok::<_, std::convert::Infallible>(MeteredAttempt {
                    attempted: true,
                    error: None,
                    egress_bytes: 0,
                    actual_units: None,
                })
            })
            .await
            .unwrap();
        charged += 1;
    }

    assert_eq!(charged, 3);
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        1000 - (3 * 2),
        "three independent regional executions → three debits"
    );
}

#[tokio::test]
async fn same_execution_id_retry_charges_once() {
    let (wallet, pricing, usage, tenant) = setup(5).await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(tenant, 100, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());

    let pk = Uuid::new_v4();
    let eid = Uuid::new_v4();
    let input = one_time_input(tenant, "us-east", pk, eid);

    let (first, _) = uc
        .execute_classified_attempt(input.clone(), || async {
            Ok::<_, std::convert::Infallible>(MeteredAttempt {
                attempted: true,
                error: None,
                egress_bytes: 0,
                actual_units: None,
            })
        })
        .await
        .unwrap();
    assert!(first.transaction.is_some(), "first call must debit wallet");
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        95
    );

    // Retry of the exact same durable execution identity.
    let (second, attempt) = uc
        .execute_classified_attempt(input, || async {
            panic!("replay must not re-execute the external operation");
            #[allow(unreachable_code)]
            Ok::<_, std::convert::Infallible>(MeteredAttempt {
                attempted: true,
                error: None,
                egress_bytes: 0,
                actual_units: None,
            })
        })
        .await
        .unwrap();

    assert!(
        second.transaction.is_none(),
        "replay must not produce a new debit"
    );
    // On replay, `execute_classified_attempt` returns a synthetic
    // MeteredAttempt { attempted: true, .. } because the work already
    // completed earlier — the panic! inside the closure is the proof that
    // the closure is not called again.
    assert!(
        attempt.attempted,
        "replay must report the original attempt as attempted"
    );
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        95,
        "retrying one regional execution must not double-charge"
    );
}

#[tokio::test]
async fn pre_attempt_failure_releases_reservation_without_charging() {
    let (wallet, pricing, usage, tenant) = setup(3).await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(tenant, 100, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let input = one_time_input(tenant, "eu-west", Uuid::new_v4(), Uuid::new_v4());

    let err = uc
        .execute_classified_attempt(input, || async {
            Ok::<_, std::convert::Infallible>(MeteredAttempt {
                attempted: false,
                error: Some("SSRF rejected before request start".into()),
                egress_bytes: 0,
                actual_units: None,
            })
        })
        .await
        .unwrap_err();

    assert!(
        matches!(err, ChargeForUsageError::OperationNotAttempted(_)),
        "expected OperationNotAttempted, got {err:?}"
    );
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        100,
        "a released reservation must leave the wallet untouched"
    );
}

#[tokio::test]
async fn insufficient_balance_on_one_region_does_not_block_other_regions() {
    let (wallet, pricing, usage, tenant) = setup(10).await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    // Enough for exactly two regional charges at 10 units each.
    wallet
        .credit(tenant, 20, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());

    let mut successes = 0u32;
    let mut insufficient = 0u32;
    for region in ["us-east", "eu-west", "af-west"] {
        let input = one_time_input(tenant, region, Uuid::new_v4(), Uuid::new_v4());
        let outcome = uc
            .execute_classified_attempt(input, || async {
                Ok::<_, std::convert::Infallible>(MeteredAttempt {
                    attempted: true,
                    error: None,
                    egress_bytes: 0,
                    actual_units: None,
                })
            })
            .await;
        match outcome {
            Ok(_) => successes += 1,
            Err(ChargeForUsageError::InsufficientBalance) => insufficient += 1,
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    assert_eq!(
        successes, 2,
        "two regions must be able to reserve + capture"
    );
    assert_eq!(
        insufficient, 1,
        "the third region must fail with InsufficientBalance"
    );
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        0,
        "the third region's reservation must have been released, not silently captured"
    );
}

#[tokio::test]
async fn regional_billing_uses_region_scoped_pricing() {
    let (wallet, pricing, usage, tenant) = setup(1).await;
    // Region override: us-east is 50 units/check for HTTP one-time.
    seed_pricing(&pricing, "http_check.one_time", 50, Some("us-east")).await;

    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(tenant, 1000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());

    // us-east must resolve the contextual price.
    let (_r, _a) = uc
        .execute_classified_attempt(
            one_time_input(tenant, "us-east", Uuid::new_v4(), Uuid::new_v4()),
            || async {
                Ok::<_, std::convert::Infallible>(MeteredAttempt {
                    attempted: true,
                    error: None,
                    egress_bytes: 0,
                    actual_units: None,
                })
            },
        )
        .await
        .unwrap();
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        950,
        "us-east must be billed at the region-specific price (50)"
    );

    // eu-west must fall back to the generic price (1).
    let (_r, _a) = uc
        .execute_classified_attempt(
            one_time_input(tenant, "eu-west", Uuid::new_v4(), Uuid::new_v4()),
            || async {
                Ok::<_, std::convert::Infallible>(MeteredAttempt {
                    attempted: true,
                    error: None,
                    egress_bytes: 0,
                    actual_units: None,
                })
            },
        )
        .await
        .unwrap();
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        949,
        "eu-west must fall back to the generic price (1)"
    );
}
