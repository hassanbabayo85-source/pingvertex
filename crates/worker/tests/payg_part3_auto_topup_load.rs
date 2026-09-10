use application::ports::{
    CheckoutDetails, CheckoutRequest, PaymentProvider, PaymentVerification, WalletRepository,
};
use async_trait::async_trait;
use domain::WalletTransactionType;
use infra::InMemoryWalletRepository;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
struct CountingProvider {
    charges: Arc<AtomicUsize>,
}

#[async_trait]
impl PaymentProvider for CountingProvider {
    fn provider_name(&self) -> &'static str {
        "load-test"
    }
    async fn create_checkout(
        &self,
        _request: &CheckoutRequest,
    ) -> application::ports::PaymentProviderResult<CheckoutDetails> {
        Err("unused".into())
    }
    async fn verify_payment(
        &self,
        reference: &str,
    ) -> application::ports::PaymentProviderResult<PaymentVerification> {
        Ok(PaymentVerification::Paid {
            provider_payment_id: format!("payment-{reference}"),
            provider_reference: reference.to_owned(),
            amount_units: 1_000,
            currency: "USD".to_owned(),
        })
    }
    async fn charge_saved_payment_method(
        &self,
        request: &application::ports::WalletTopUpRequest,
    ) -> application::ports::PaymentProviderResult<PaymentVerification> {
        self.charges.fetch_add(1, Ordering::SeqCst);
        Ok(PaymentVerification::Paid {
            provider_payment_id: "provider-payment-once".to_owned(),
            provider_reference: request.idempotency_key.clone(),
            amount_units: request.amount_units,
            currency: request.currency.clone(),
        })
    }
}

#[tokio::test]
async fn concurrent_workers_charge_same_tenant_only_once() {
    let wallet = Arc::new(InMemoryWalletRepository::default());
    let tenant = Uuid::new_v4();
    wallet.get_or_create(tenant, "USD").await.unwrap();
    let mut settings = wallet.get_settings(tenant).await.unwrap();
    settings.auto_top_up_enabled = true;
    settings.auto_top_up_threshold_units = 5_000;
    settings.auto_top_up_amount_units = 10_000;
    settings.payment_method = Some("saved-method".to_owned());
    settings.daily_top_up_limit_units = 100_000;
    settings.monthly_top_up_limit_units = 100_000;
    settings.retry_limit = 3;
    wallet.update_settings(&settings).await.unwrap();
    wallet
        .credit(tenant, 1_000, WalletTransactionType::TopUp, "seed", None)
        .await
        .unwrap();

    let provider = Arc::new(CountingProvider {
        charges: Arc::new(AtomicUsize::new(0)),
    });
    let cancel = CancellationToken::new();
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let worker = worker::AutoTopUpWorker::new(
            wallet.clone(),
            provider.clone(),
            Duration::from_millis(2),
        );
        let cancel = cancel.clone();
        tasks.push(tokio::spawn(
            async move { worker.run_forever(cancel).await },
        ));
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    cancel.cancel();
    for task in tasks {
        task.await.unwrap().unwrap();
    }

    assert_eq!(
        provider.charges.load(Ordering::SeqCst),
        1,
        "provider must be charged once under worker concurrency"
    );
    assert_eq!(
        wallet.find(tenant).await.unwrap().unwrap().balance_units,
        11_000
    );
}
