use application::ports::{
    PaymentProvider, PaymentVerification, WalletRepository, WalletTopUpRequest,
};
use chrono::Utc;
use domain::AutoTopUpAttempt;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub struct AutoTopUpWorker {
    wallet: Arc<dyn WalletRepository>,
    provider: Arc<dyn PaymentProvider>,
    interval: Duration,
}
impl AutoTopUpWorker {
    pub fn new(
        wallet: Arc<dyn WalletRepository>,
        provider: Arc<dyn PaymentProvider>,
        interval: Duration,
    ) -> Self {
        Self {
            wallet,
            provider,
            interval,
        }
    }
    async fn reconcile(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for attempt in self
            .wallet
            .list_auto_top_up_reconciliation_attempts(50)
            .await?
        {
            let Some(reference) = attempt.provider_reference.as_deref() else {
                continue;
            };
            match self.provider.verify_payment(reference).await? {
                PaymentVerification::Paid {
                    provider_payment_id,
                    provider_reference,
                    amount_units,
                    currency,
                } => {
                    if amount_units != attempt.amount_units || !currency.eq_ignore_ascii_case("USD")
                    {
                        self.wallet
                            .record_auto_top_up_attempt(&AutoTopUpAttempt {
                                id: attempt.id,
                                tenant_id: attempt.tenant_id,
                                amount_units: attempt.amount_units,
                                status: "REQUIRES_RECONCILIATION".into(),
                                provider_payment_id: Some(provider_payment_id),
                                provider_reference: Some(provider_reference),
                                error_message: Some("provider amount/currency mismatch".into()),
                                attempted_at: Utc::now(),
                            })
                            .await?;
                        continue;
                    }
                    let key = format!("auto-topup-provider:{provider_payment_id}");
                    match self
                        .wallet
                        .credit(
                            attempt.tenant_id,
                            amount_units,
                            domain::WalletTransactionType::TopUp,
                            &key,
                            Some("Automatic wallet top-up reconciliation"),
                        )
                        .await
                    {
                        Ok(_) => {
                            self.wallet
                                .record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt.id,
                                    tenant_id: attempt.tenant_id,
                                    amount_units: attempt.amount_units,
                                    status: "SUCCEEDED".into(),
                                    provider_payment_id: Some(provider_payment_id),
                                    provider_reference: Some(provider_reference),
                                    error_message: None,
                                    attempted_at: Utc::now(),
                                })
                                .await?
                        }
                        Err(e) => {
                            self.wallet
                                .record_auto_top_up_attempt(&AutoTopUpAttempt {
                                    id: attempt.id,
                                    tenant_id: attempt.tenant_id,
                                    amount_units: attempt.amount_units,
                                    status: "REQUIRES_RECONCILIATION".into(),
                                    provider_payment_id: attempt.provider_payment_id,
                                    provider_reference: attempt.provider_reference,
                                    error_message: Some(format!("wallet credit failed: {e}")),
                                    attempted_at: Utc::now(),
                                })
                                .await?
                        }
                    }
                }
                PaymentVerification::Pending {
                    provider_payment_id,
                    provider_reference,
                    ..
                } => {
                    self.wallet
                        .record_auto_top_up_attempt(&AutoTopUpAttempt {
                            id: attempt.id,
                            tenant_id: attempt.tenant_id,
                            amount_units: attempt.amount_units,
                            status: "REQUIRES_RECONCILIATION".into(),
                            provider_payment_id: provider_payment_id
                                .or(attempt.provider_payment_id),
                            provider_reference: provider_reference.or(attempt.provider_reference),
                            error_message: Some("provider payment is still pending".into()),
                            attempted_at: Utc::now(),
                        })
                        .await?
                }
                PaymentVerification::Failed {
                    provider_payment_id,
                    provider_reference,
                    ..
                } => {
                    self.wallet
                        .record_auto_top_up_attempt(&AutoTopUpAttempt {
                            id: attempt.id,
                            tenant_id: attempt.tenant_id,
                            amount_units: attempt.amount_units,
                            status: "FAILED".into(),
                            provider_payment_id: provider_payment_id
                                .or(attempt.provider_payment_id),
                            provider_reference: provider_reference.or(attempt.provider_reference),
                            error_message: Some("provider reports payment failed".into()),
                            attempted_at: Utc::now(),
                        })
                        .await?
                }
            }
        }
        Ok(())
    }
    pub async fn run_forever(
        &self,
        cancel: CancellationToken,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut ticker = tokio::time::interval(self.interval);
        loop {
            tokio::select! {
                _=cancel.cancelled()=>return Ok(()),
                _=ticker.tick()=>{
                    self.reconcile().await?;
                    for candidate in self.wallet.auto_top_up_candidates(50).await?{
                        if candidate.daily_attempts>=candidate.settings.retry_limit{continue;}
                        let amount=candidate.settings.auto_top_up_amount_units.min(candidate.settings.daily_top_up_limit_units.saturating_sub(candidate.daily_spend_units)).min(candidate.settings.monthly_top_up_limit_units.saturating_sub(candidate.monthly_spend_units));
                        if amount<=0{continue;}
                        let lease_id=uuid::Uuid::new_v4();
                        if !self.wallet.claim_auto_top_up(candidate.tenant_id,lease_id,Duration::from_secs(600)).await?{continue;}
                        let attempt_id=uuid::Uuid::new_v4();
                        let reference=format!("pv-auto-{}",attempt_id.simple());
                        self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt{id:attempt_id,tenant_id:candidate.tenant_id,amount_units:amount,status:"PENDING".into(),provider_payment_id:None,provider_reference:Some(reference.clone()),error_message:None,attempted_at:Utc::now()}).await?;
                        let result=self.provider.charge_saved_payment_method(&WalletTopUpRequest{tenant_id:domain::shared::ids::TenantId(candidate.tenant_id),amount_units:amount,currency:candidate.currency.clone(),idempotency_key:reference.clone(),payment_method:candidate.settings.payment_method.clone(),customer_email:candidate.customer_email.clone()}).await;
                        match result{
                            Ok(PaymentVerification::Paid{provider_payment_id,provider_reference,amount_units,currency})=>{
                                if amount_units!=amount || !currency.eq_ignore_ascii_case(&candidate.currency){self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt{id:attempt_id,tenant_id:candidate.tenant_id,amount_units:amount,status:"REQUIRES_RECONCILIATION".into(),provider_payment_id:Some(provider_payment_id),provider_reference:Some(provider_reference),error_message:Some("provider amount/currency mismatch".into()),attempted_at:Utc::now()}).await?;}
                                else{let key=format!("auto-topup-provider:{provider_payment_id}");match self.wallet.credit(candidate.tenant_id,amount_units,domain::WalletTransactionType::TopUp,&key,Some("Automatic wallet top-up")).await{Ok(_)=>self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt{id:attempt_id,tenant_id:candidate.tenant_id,amount_units:amount,status:"SUCCEEDED".into(),provider_payment_id:Some(provider_payment_id),provider_reference:Some(provider_reference),error_message:None,attempted_at:Utc::now()}).await?,Err(e)=>self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt{id:attempt_id,tenant_id:candidate.tenant_id,amount_units:amount,status:"REQUIRES_RECONCILIATION".into(),provider_payment_id:Some(provider_payment_id),provider_reference:Some(provider_reference),error_message:Some(format!("wallet credit failed: {e}")),attempted_at:Utc::now()}).await?}}}
                            Ok(PaymentVerification::Pending{provider_payment_id,provider_reference,..})=>self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt{id:attempt_id,tenant_id:candidate.tenant_id,amount_units:amount,status:"REQUIRES_RECONCILIATION".into(),provider_payment_id,provider_reference:provider_reference.or(Some(reference)),error_message:Some("provider payment is pending".into()),attempted_at:Utc::now()}).await?,
                            Ok(PaymentVerification::Failed{provider_payment_id,provider_reference,..})=>self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt{id:attempt_id,tenant_id:candidate.tenant_id,amount_units:amount,status:"FAILED".into(),provider_payment_id,provider_reference:provider_reference.or(Some(reference)),error_message:Some("automatic top-up payment failed".into()),attempted_at:Utc::now()}).await?,
                            Err(e)=>self.wallet.record_auto_top_up_attempt(&AutoTopUpAttempt{id:attempt_id,tenant_id:candidate.tenant_id,amount_units:amount,status:"REQUIRES_RECONCILIATION".into(),provider_payment_id:None,provider_reference:Some(reference),error_message:Some(format!("provider error; reconciliation required: {e}")),attempted_at:Utc::now()}).await?,
                        }
                        let _=self.wallet.release_auto_top_up(candidate.tenant_id,lease_id).await;
                    }
                }
            }
        }
    }
}
