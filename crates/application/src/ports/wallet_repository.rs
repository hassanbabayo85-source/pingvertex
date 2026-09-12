use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    AutoTopUpAttempt, AutoTopUpCandidate, CreditReservation, Wallet, WalletNotification,
    WalletSettings, WalletTopUpSession, WalletTransaction, WalletTransactionType,
};
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub type WalletRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationOutcome {
    pub reservation: CreditReservation,
    pub created: bool,
}

#[async_trait]
pub trait WalletRepository: Send + Sync {
    /// Read-only lookup. Unlike `get_or_create`, this MUST NOT insert a wallet
    /// row and is therefore safe for repeatable previews.
    async fn find(&self, tenant_id: Uuid) -> WalletRepoResult<Option<Wallet>> {
        let _ = tenant_id;
        Ok(None)
    }

    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet>;
    async fn get_balance_summary(
        &self,
        tenant_id: Uuid,
        currency: &str,
    ) -> WalletRepoResult<domain::WalletBalanceSummary>;
    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>>;

    /// Atomically checks available credits and creates (or returns) the
    /// idempotent reservation. Implementations MUST lock the wallet row and
    /// perform the uniqueness check and insert in the same transaction.
    #[allow(clippy::too_many_arguments)] // PAYG price snapshot (unit_price, pricing_version, currency) must be pinned per Section 7
    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        unit_price: i64,
        pricing_version: i64,
        currency: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome>;

    /// Captures a pending reservation and writes the immutable debit ledger.
    /// When supplied, the usage event and financial audit rows are committed in
    /// the same database transaction as the debit. Replays of CAPTURED rows
    /// backfill missing event/audit rows without creating another debit.
    async fn capture(
        &self,
        reservation_id: Uuid,
        usage_event: Option<&domain::UsageEvent>,
        audit_entries: &[domain::FinancialAuditLogEntry],
    ) -> WalletRepoResult<WalletTransaction>;

    /// Releases a pending reservation without charging the wallet.
    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()>;

    /// Credits the wallet atomically. Used only by trusted payment/webhook
    /// flows; callers must provide an idempotency key.
    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction>;
    async fn adjust_balance(
        &self,
        _tenant_id: Uuid,
        _delta_units: i64,
        _idempotency_key: &str,
        _description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        Err("wallet balance adjustment is not configured".into())
    }

    async fn find_reservation_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> WalletRepoResult<Option<CreditReservation>> {
        let _ = (tenant_id, request_id);
        Ok(None)
    }

    /// Extends a still-pending reservation for the owning tenant/request.
    /// Terminal reservations must never be resurrected.
    async fn renew_reservation_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<bool>;

    async fn create_top_up_session(&self, session: &WalletTopUpSession) -> WalletRepoResult<()>;
    async fn find_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>>;

    async fn find_top_up_session_by_idempotency(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        let _ = (tenant_id, idempotency_key);
        Ok(None)
    }

    /// Looks up a top-up session by provider identity alone, without a
    /// pre-known tenant id. Used by provider webhooks.
    async fn find_top_up_session_global(
        &self,
        provider: &str,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        let _ = (provider, provider_checkout_id);
        Ok(None)
    }

    /// Marks a PENDING wallet top-up session as FAILED. Only affects rows
    /// whose current status is PENDING; sessions already COMPLETED or
    /// FAILED are left untouched and the method returns `false`.
    ///
    /// Paystack's `charge.failed` webhook does not go through
    /// `ProcessBillingWebhookUseCase` because that use case only routes
    /// `wallet_top_up=true` events when the event type is PaymentSucceeded,
    /// and otherwise falls into the subscription path (which will not find
    /// a subscription for a wallet-only tenant). The state transition is a
    /// direct, idempotent, tenant-scoped repository update instead.
    ///
    /// `reason` is informational; the schema has no reason column on
    /// `wallet_top_up_sessions`, so it is only logged by implementations.
    ///
    /// Default returns `Ok(false)`, matching the convention used by other
    /// optional methods on this trait.
    async fn mark_top_up_session_failed(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        reason: &str,
    ) -> WalletRepoResult<bool> {
        let _ = (tenant_id, provider_checkout_id, reason);
        Ok(false)
    }
    async fn complete_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        provider_payment_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>>;
    async fn refundable_top_up(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
    ) -> WalletRepoResult<Option<WalletTopUpSession>>;
    async fn record_verified_refund(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
        provider_refund_id: &str,
    ) -> WalletRepoResult<bool>;
    async fn create_refund_request(
        &self,
        request: &domain::WalletRefundRequest,
    ) -> WalletRepoResult<Option<domain::WalletRefundRequest>>;
    async fn set_refund_provider_id(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
        provider_refund_id: &str,
    ) -> WalletRepoResult<()>;
    async fn list_auto_top_up_reconciliation_attempts(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<AutoTopUpAttempt>> {
        let _ = limit;
        Ok(Vec::new())
    }

    async fn record_auto_top_up_attempt(&self, attempt: &AutoTopUpAttempt) -> WalletRepoResult<()>;
    async fn claim_auto_top_up(
        &self,
        tenant_id: Uuid,
        lease_id: Uuid,
        lease_for: std::time::Duration,
    ) -> WalletRepoResult<bool>;
    async fn release_auto_top_up(&self, tenant_id: Uuid, lease_id: Uuid) -> WalletRepoResult<()>;
    async fn get_settings(&self, tenant_id: Uuid) -> WalletRepoResult<WalletSettings>;
    async fn update_settings(&self, settings: &WalletSettings) -> WalletRepoResult<WalletSettings>;
    async fn expire_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<Option<WalletTransaction>>;
    async fn grant_promotional_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        expires_at: DateTime<Utc>,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction>;
    async fn expire_promotional_credits(&self, limit: u32) -> WalletRepoResult<u64>;
    async fn expire_pending_reservations(&self, limit: u32) -> WalletRepoResult<u64>;
    async fn auto_top_up_candidates(&self, limit: u32)
    -> WalletRepoResult<Vec<AutoTopUpCandidate>>;
    async fn record_balance_notification(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        dedupe_key: &str,
        message: &str,
    ) -> WalletRepoResult<bool>;
    async fn list_balance_notifications(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>>;
    async fn claim_pending_email_notifications(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>> {
        let _ = limit;
        Ok(Vec::new())
    }
    async fn mark_balance_notification_emailed(&self, id: Uuid) -> WalletRepoResult<bool> {
        let _ = id;
        Ok(false)
    }

    /// Creates at most one daily and one monthly usage-threshold notification
    /// per tenant/period. Production implementations must claim/insert using
    /// database uniqueness so multiple workers cannot duplicate alerts.
    async fn record_usage_threshold_notifications(&self, limit: u32) -> WalletRepoResult<u64> {
        let _ = limit;
        Ok(0)
    }
}

#[async_trait]
impl<T> WalletRepository for Arc<T>
where
    T: WalletRepository + Send + Sync + ?Sized,
{
    async fn find(&self, tenant_id: Uuid) -> WalletRepoResult<Option<Wallet>> {
        (**self).find(tenant_id).await
    }

    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        (**self).get_or_create(tenant_id, currency).await
    }
    async fn get_balance_summary(
        &self,
        tenant_id: Uuid,
        currency: &str,
    ) -> WalletRepoResult<domain::WalletBalanceSummary> {
        (**self).get_balance_summary(tenant_id, currency).await
    }

    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>> {
        (**self).list_transactions(tenant_id, limit).await
    }
    #[allow(clippy::too_many_arguments)]
    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        unit_price: i64,
        pricing_version: i64,
        currency: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        (**self)
            .reserve(
                tenant_id,
                amount_units,
                operation,
                unit_price,
                pricing_version,
                currency,
                request_id,
                expires_at,
            )
            .await
    }
    async fn capture(
        &self,
        reservation_id: Uuid,
        usage_event: Option<&domain::UsageEvent>,
        audit_entries: &[domain::FinancialAuditLogEntry],
    ) -> WalletRepoResult<WalletTransaction> {
        (**self)
            .capture(reservation_id, usage_event, audit_entries)
            .await
    }
    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        (**self).release(reservation_id).await
    }
    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        (**self)
            .credit(
                tenant_id,
                amount_units,
                transaction_type,
                idempotency_key,
                description,
            )
            .await
    }
    async fn adjust_balance(
        &self,
        tenant_id: Uuid,
        delta_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        (**self)
            .adjust_balance(tenant_id, delta_units, idempotency_key, description)
            .await
    }
    async fn find_reservation_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> WalletRepoResult<Option<CreditReservation>> {
        (**self)
            .find_reservation_by_request(tenant_id, request_id)
            .await
    }

    async fn create_top_up_session(&self, session: &WalletTopUpSession) -> WalletRepoResult<()> {
        (**self).create_top_up_session(session).await
    }
    async fn find_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        (**self)
            .find_top_up_session(tenant_id, provider_checkout_id)
            .await
    }

    async fn find_top_up_session_by_idempotency(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        (**self)
            .find_top_up_session_by_idempotency(tenant_id, idempotency_key)
            .await
    }
    async fn complete_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        provider_payment_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        (**self)
            .complete_top_up_session(tenant_id, provider_checkout_id, provider_payment_id)
            .await
    }
    async fn refundable_top_up(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        (**self)
            .refundable_top_up(tenant_id, provider_payment_id, amount_units)
            .await
    }
    async fn record_verified_refund(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
        provider_refund_id: &str,
    ) -> WalletRepoResult<bool> {
        (**self)
            .record_verified_refund(
                tenant_id,
                provider_payment_id,
                amount_units,
                provider_refund_id,
            )
            .await
    }
    async fn create_refund_request(
        &self,
        request: &domain::WalletRefundRequest,
    ) -> WalletRepoResult<Option<domain::WalletRefundRequest>> {
        (**self).create_refund_request(request).await
    }
    async fn set_refund_provider_id(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
        provider_refund_id: &str,
    ) -> WalletRepoResult<()> {
        (**self)
            .set_refund_provider_id(tenant_id, idempotency_key, provider_refund_id)
            .await
    }
    async fn list_auto_top_up_reconciliation_attempts(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<AutoTopUpAttempt>> {
        (**self)
            .list_auto_top_up_reconciliation_attempts(limit)
            .await
    }

    async fn record_auto_top_up_attempt(&self, attempt: &AutoTopUpAttempt) -> WalletRepoResult<()> {
        (**self).record_auto_top_up_attempt(attempt).await
    }
    async fn claim_auto_top_up(
        &self,
        tenant_id: Uuid,
        lease_id: Uuid,
        lease_for: std::time::Duration,
    ) -> WalletRepoResult<bool> {
        (**self)
            .claim_auto_top_up(tenant_id, lease_id, lease_for)
            .await
    }
    async fn release_auto_top_up(&self, tenant_id: Uuid, lease_id: Uuid) -> WalletRepoResult<()> {
        (**self).release_auto_top_up(tenant_id, lease_id).await
    }
    async fn get_settings(&self, tenant_id: Uuid) -> WalletRepoResult<WalletSettings> {
        (**self).get_settings(tenant_id).await
    }
    async fn update_settings(&self, settings: &WalletSettings) -> WalletRepoResult<WalletSettings> {
        (**self).update_settings(settings).await
    }
    async fn expire_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<Option<WalletTransaction>> {
        (**self)
            .expire_credit(tenant_id, amount_units, idempotency_key, description)
            .await
    }
    async fn grant_promotional_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        expires_at: DateTime<Utc>,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        (**self)
            .grant_promotional_credit(
                tenant_id,
                amount_units,
                expires_at,
                idempotency_key,
                description,
            )
            .await
    }
    async fn expire_promotional_credits(&self, limit: u32) -> WalletRepoResult<u64> {
        (**self).expire_promotional_credits(limit).await
    }
    async fn expire_pending_reservations(&self, limit: u32) -> WalletRepoResult<u64> {
        (**self).expire_pending_reservations(limit).await
    }
    async fn auto_top_up_candidates(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<AutoTopUpCandidate>> {
        (**self).auto_top_up_candidates(limit).await
    }
    async fn record_balance_notification(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        dedupe_key: &str,
        message: &str,
    ) -> WalletRepoResult<bool> {
        (**self)
            .record_balance_notification(tenant_id, event_type, dedupe_key, message)
            .await
    }
    async fn list_balance_notifications(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>> {
        (**self).list_balance_notifications(tenant_id, limit).await
    }
    async fn renew_reservation_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<bool> {
        (**self)
            .renew_reservation_by_request(tenant_id, request_id, expires_at)
            .await
    }
}
