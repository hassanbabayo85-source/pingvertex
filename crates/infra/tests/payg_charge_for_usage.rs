use application::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
use application::{ChargeForUsageError, ChargeForUsageInput, ChargeForUsageUseCase};
use chrono::{Duration, Utc};
use domain::PAYGPricing;
use infra::{
    InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

#[allow(clippy::type_complexity)]
fn setup(
    unit_price: i64,
    version: i64,
) -> (
    Arc<dyn WalletRepository>,
    Arc<dyn PAYGPricingRepository>,
    Arc<dyn UsageEventRepository>,
    Uuid,
) {
    let wallet = Arc::new(InMemoryWalletRepository::default());

    let pricing = Arc::new(InMemoryPAYGPricingRepository::new(vec![PAYGPricing {
        id: Uuid::new_v4(),
        version,
        operation: "proxy_check".into(),
        unit_name: "check".into(),
        unit_price,
        currency: "USD".into(),
        effective_from: Utc::now() - Duration::seconds(1),
        effective_until: None,
        active: true,
    }]));

    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let tenant = Uuid::new_v4();

    (wallet, pricing, usage, tenant)
}

#[tokio::test]
async fn duplicate_request_returns_previous_result_without_running_operation_twice() {
    let (wallet, pricing, usage, tenant) = setup(10, 7);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-1",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let calls = Arc::new(AtomicUsize::new(0));

    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "proxy_check".into(),
        units: 2,
        request_id: "req-1".into(),
    };

    let c = calls.clone();
    let first = uc
        .execute(input.clone(), move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &'static str>(())
            }
        })
        .await
        .unwrap();

    let c = calls.clone();
    let second = uc
        .execute(input, move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &'static str>(())
            }
        })
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.usage_event, second.usage_event);
    assert_eq!(second.usage_event.pricing_version, 7);
    assert_eq!(second.usage_event.charge_units, 20);
}

#[tokio::test]
async fn concurrent_different_requests_never_overspend() {
    let (wallet, pricing, usage, tenant) = setup(60, 3);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-2",
            None,
        )
        .await
        .unwrap();

    let uc = Arc::new(ChargeForUsageUseCase::new(wallet.clone(), pricing, usage));
    let mut tasks = Vec::new();

    for i in 0..10 {
        let uc = uc.clone();

        tasks.push(tokio::spawn(async move {
            let input = ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: format!("concurrent-{i}"),
            };

            uc.execute(input, || async { Ok::<_, &'static str>(()) })
                .await
        }));
    }

    let mut success = 0;

    for task in tasks {
        if task.await.unwrap().is_ok() {
            success += 1;
        }
    }

    assert_eq!(success, 1);

    let final_wallet = wallet.get_or_create(tenant, "USD").await.unwrap();
    assert_eq!(final_wallet.balance_units, 40);
}

#[tokio::test]
async fn insufficient_balance_is_rejected_and_wallet_never_goes_negative() {
    let (wallet, pricing, usage, tenant) = setup(60, 2);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            50,
            domain::WalletTransactionType::TopUp,
            "topup-3",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);

    let result = uc
        .execute(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "insufficient".into(),
            },
            || async { Ok::<_, &'static str>(()) },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::InsufficientBalance)
    ));

    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        50
    );
}

#[tokio::test]
async fn failed_operation_releases_reservation_and_does_not_debit() {
    let (wallet, pricing, usage, tenant) = setup(25, 9);

    wallet.get_or_create(tenant, "USD").await.unwrap();

    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-4",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());

    let result = uc
        .execute(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 2,
                request_id: "failed-op".into(),
            },
            || async { Err::<(), _>("downstream failed") },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationFailed(_))
    ));

    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        100
    );

    let event = usage
        .find_by_request(tenant, "failed-op")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(event.status, "FAILED");
    assert_eq!(event.units_billed, 0);
}

#[tokio::test]
async fn expired_pending_reservation_can_be_reused_after_worker_recovery() {
    let (wallet, pricing, usage, tenant) = setup(10, 11);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "topup-expired",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);
    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "proxy_check".into(),
        units: 1,
        request_id: "recoverable-request".into(),
    };

    // Simulate a worker crash after reservation creation by directly reserving
    // an already-expired reservation with the same idempotency key.
    wallet
        .reserve(
            tenant,
            10,
            "proxy_check",
            &input.request_id,
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();

    let result = uc
        .execute(input, || async { Ok::<_, &'static str>(()) })
        .await
        .unwrap();

    assert_eq!(result.usage_event.status, "CAPTURED");
    assert_eq!(result.usage_event.charge_units, 10);
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        90
    );
}

#[tokio::test]
async fn monitoring_attempt_is_billed_when_operation_fails() {
    let (wallet, pricing, usage, tenant) = setup(25, 12);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "attempt-topup",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let result = uc
        .execute_attempt(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "attempt-failure".into(),
            },
            || async { Err::<(), _>("HTTP 500") },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationFailed(_))
    ));
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        75
    );
    let event = usage
        .find_by_request(tenant, "attempt-failure")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.status, "CAPTURED");
    assert_eq!(event.charge_units, 25);
}

#[tokio::test]
async fn expired_reservation_is_terminally_marked_then_reopened_safely() {
    let (wallet, pricing, usage, tenant) = setup(10, 13);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "expiry-topup",
            None,
        )
        .await
        .unwrap();
    wallet
        .reserve(
            tenant,
            10,
            "proxy_check",
            "expiry-retry",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();

    assert_eq!(wallet.expire_pending_reservations(50).await.unwrap(), 1);
    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage);
    let result = uc
        .execute(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "expiry-retry".into(),
            },
            || async { Ok::<_, &'static str>(()) },
        )
        .await
        .unwrap();

    assert_eq!(result.usage_event.charge_units, 10);
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        90
    );
}

#[tokio::test]
async fn classified_pre_attempt_failure_releases_reservation_without_debit() {
    let (wallet, pricing, usage, tenant) = setup(25, 20);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "classified-preflight-topup",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let result = uc
        .execute_classified_attempt(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "classified-preflight".into(),
            },
            || async {
                Ok::<_, &'static str>(application::MeteredAttempt {
                    attempted: false,
                    error: Some("SSRF validation blocked the target".into()),
                })
            },
        )
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationNotAttempted(_))
    ));
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        100
    );
    assert!(
        usage
            .find_by_request(tenant, "classified-preflight")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn classified_attempt_failure_is_captured_and_billed_once() {
    let (wallet, pricing, usage, tenant) = setup(25, 21);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "classified-attempt-topup",
            None,
        )
        .await
        .unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let input = ChargeForUsageInput {
        tenant_id: tenant,
        operation: "proxy_check".into(),
        units: 1,
        request_id: "classified-attempt".into(),
    };
    let result = uc
        .execute_classified_attempt(input.clone(), || async {
            Ok::<_, &'static str>(application::MeteredAttempt {
                attempted: true,
                error: Some("connection refused".into()),
            })
        })
        .await;

    assert!(matches!(
        result,
        Err(ChargeForUsageError::OperationFailed(_))
    ));
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        75
    );

    let replay = uc
        .execute_classified_attempt(input, || async {
            Err::<application::MeteredAttempt, _>(
                "idempotent replay must not execute the operation",
            )
        })
        .await
        .unwrap();
    assert_eq!(replay.0.usage_event.charge_units, 25);
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        75
    );
}

#[tokio::test]
async fn crash_after_attempt_before_capture_is_recoverable_without_double_debit() {
    let (wallet, pricing, usage, tenant) = setup(10, 22);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "crash-after-attempt-topup",
            None,
        )
        .await
        .unwrap();

    wallet
        .reserve(
            tenant,
            10,
            "proxy_check",
            "crash-after-attempt",
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap();
    // The external attempt is assumed to have happened immediately before a
    // worker crash. The reservation expiry worker makes the durable request
    // reusable; the retry performs the logical execution once more but still
    // permits only one ledger debit for the request.
    wallet.expire_pending_reservations(50).await.unwrap();

    let uc = ChargeForUsageUseCase::new(wallet.clone(), pricing, usage.clone());
    let result = uc
        .execute_classified_attempt(
            ChargeForUsageInput {
                tenant_id: tenant,
                operation: "proxy_check".into(),
                units: 1,
                request_id: "crash-after-attempt".into(),
            },
            || async {
                Ok::<_, &'static str>(application::MeteredAttempt {
                    attempted: true,
                    error: None,
                })
            },
        )
        .await
        .unwrap();

    assert_eq!(result.0.usage_event.charge_units, 10);
    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        90
    );
}

#[tokio::test]
async fn concurrent_same_execution_id_can_debit_at_most_once() {
    let (wallet, pricing, usage, tenant) = setup(10, 23);
    wallet.get_or_create(tenant, "USD").await.unwrap();
    wallet
        .credit(
            tenant,
            100,
            domain::WalletTransactionType::TopUp,
            "same-execution-topup",
            None,
        )
        .await
        .unwrap();

    let uc = Arc::new(ChargeForUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let uc = uc.clone();
        tasks.push(tokio::spawn(async move {
            uc.execute_classified_attempt(
                ChargeForUsageInput {
                    tenant_id: tenant,
                    operation: "proxy_check".into(),
                    units: 1,
                    request_id: "same-execution".into(),
                },
                || async {
                    Ok::<_, &'static str>(application::MeteredAttempt {
                        attempted: true,
                        error: None,
                    })
                },
            )
            .await
        }));
    }
    for task in tasks {
        let _ = task.await.unwrap();
    }

    assert_eq!(
        wallet
            .get_or_create(tenant, "USD")
            .await
            .unwrap()
            .balance_units,
        90
    );
    let event = usage
        .find_by_request(tenant, "same-execution")
        .await
        .unwrap();
    assert!(event.is_some());
}
