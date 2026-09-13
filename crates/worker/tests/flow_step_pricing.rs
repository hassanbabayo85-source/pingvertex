//! GAP 3 Kashi B: flow-step-based pricing.
//!
//! A Flow check reserves `MAX_STEPS × unit_price` up front, then reports
//! the number of steps it actually executed. The wallet is billed for
//! that many steps and the reservation remainder is released in the same
//! capture transaction. Non-flow checks keep their legacy single-unit
//! behaviour.

use application::ports::{PAYGPricingRepository, WalletRepository};
use application::use_cases::charge_for_usage::{
    ChargeForUsageError, ChargeForUsageInput, ChargeForUsageUseCase, MeteredAttempt,
};
use chrono::{Duration as ChronoDuration, Utc};
use domain::WalletTransactionType;
use std::sync::Arc;
use uuid::Uuid;

/// `api_flow_check.monitoring` pricing is 1000 nano-USD per step
/// (matching migration 0114).
const FLOW_UNIT_PRICE: i64 = 1_000;
/// `domain::entities::api_flow::MAX_STEPS`.
const MAX_STEPS: i64 = 10;

async fn setup() -> (
    Arc<infra::InMemoryWalletRepository>,
    Arc<infra::InMemoryPAYGPricingRepository>,
    Arc<infra::InMemoryUsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(infra::InMemoryWalletRepository::default());
    let pricing = Arc::new(infra::InMemoryPAYGPricingRepository::new());
    pricing
        .create_version(
            "api_flow_check.monitoring",
            "request/check",
            FLOW_UNIT_PRICE,
            "USD",
            Utc::now() - ChronoDuration::minutes(1),
        )
        .await
        .unwrap();
    let usage = Arc::new(infra::InMemoryUsageEventRepository::default());
    (wallet, pricing, usage, Uuid::new_v4())
}

fn flow_input(tenant_id: Uuid, request_id: &str) -> ChargeForUsageInput {
    ChargeForUsageInput {
        tenant_id,
        operation: "api_flow_check.monitoring".into(),
        // Reserve the full MAX_STEPS budget up front.
        units: MAX_STEPS,
        request_id: request_id.to_owned(),
        monitor_id: Some(Uuid::new_v4()),
        execution_id: Some(Uuid::new_v4()),
        pricing_context: None,
    }
}

/// 10 steps reserved × 1000 = 10_000 nano-USD.
fn reserved_amount() -> i64 {
    MAX_STEPS * FLOW_UNIT_PRICE
}

#[tokio::test]
async fn flow_of_three_steps_charges_three_steps() {
    let (wallet, pricing, usage, tenant) = setup().await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(tenant, 100_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let input = flow_input(tenant, "flow-3-steps");

    let (result, _) = uc
        .execute_classified_attempt(input, || async {
            Ok::<_, std::convert::Infallible>(MeteredAttempt {
                attempted: true,
                error: None,
                egress_bytes: 0,
                actual_units: Some(3),
            })
        })
        .await
        .unwrap();

    assert_eq!(
        result.usage_event.units_billed, 3,
        "only 3 steps are billed"
    );
    assert_eq!(
        result.usage_event.charge_units,
        3 * FLOW_UNIT_PRICE,
        "3 steps × 1000 nano-USD"
    );
    assert_eq!(result.usage_event.units_requested, MAX_STEPS);

    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(
        w.balance_units,
        100_000 - (3 * FLOW_UNIT_PRICE),
        "wallet is debited only for the 3 executed steps"
    );
}

#[tokio::test]
async fn flow_failing_at_step_two_charges_two_steps() {
    let (wallet, pricing, usage, tenant) = setup().await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(tenant, 100_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let input = flow_input(tenant, "flow-fail-step-2");

    // Step 2 failed after its request went out. `attempted = true`
    // means the request started and must be billed.
    let err = uc
        .execute_classified_attempt(input, || async {
            Ok::<_, std::convert::Infallible>(MeteredAttempt {
                attempted: true,
                error: Some("step 2 failed".into()),
                egress_bytes: 0,
                actual_units: Some(2),
            })
        })
        .await
        .unwrap_err();

    assert!(
        matches!(err, ChargeForUsageError::OperationFailed(_)),
        "expected OperationFailed, got {err:?}"
    );

    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(
        w.balance_units,
        100_000 - (2 * FLOW_UNIT_PRICE),
        "the two attempted steps are still billed"
    );
}

#[tokio::test]
async fn flow_with_zero_attempts_releases_reservation() {
    let (wallet, pricing, usage, tenant) = setup().await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(tenant, 100_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let input = flow_input(tenant, "flow-zero-attempts");

    // SSRF-blocked first step: no HTTP request went out.
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

    let w = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(
        w.balance_units, 100_000,
        "a non-billable attempt must leave the wallet untouched"
    );
    assert_eq!(reserved_amount(), 10_000);
}

#[tokio::test]
async fn flow_retry_does_not_double_charge() {
    let (wallet, pricing, usage, tenant) = setup().await;
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(tenant, 100_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let input = flow_input(tenant, "flow-retry");

    // First attempt: 4 steps executed and billed.
    let (_first, _) = uc
        .execute_classified_attempt(input.clone(), || async {
            Ok::<_, std::convert::Infallible>(MeteredAttempt {
                attempted: true,
                error: None,
                egress_bytes: 0,
                actual_units: Some(4),
            })
        })
        .await
        .unwrap();

    let after_first = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(after_first.balance_units, 100_000 - (4 * FLOW_UNIT_PRICE));

    // Same durable execution identity replays. The operation closure
    // must not run again and the wallet must not be debited again.
    let (_second, attempt) = uc
        .execute_classified_attempt(input, || async {
            panic!("replay must not re-execute the external operation");
            #[allow(unreachable_code)]
            Ok::<_, std::convert::Infallible>(MeteredAttempt {
                attempted: true,
                error: None,
                egress_bytes: 0,
                actual_units: Some(4),
            })
        })
        .await
        .unwrap();

    // On replay the outer wrapper returns the default `MeteredAttempt`
    // (the `panic!()` above already proved the closure was not called).
    assert!(
        attempt.actual_units.is_none(),
        "replay must not report actual_units"
    );
    assert!(attempt.attempted, "replay default is attempted=true");

    let after_second = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(
        after_second.balance_units, after_first.balance_units,
        "replay must not double-charge"
    );
}
