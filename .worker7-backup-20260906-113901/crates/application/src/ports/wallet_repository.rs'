use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{CreditReservation, Wallet, WalletTransaction, WalletTransactionType};
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
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet>;
    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>>;

    /// Atomically checks available credits and creates (or returns) the
    /// idempotent reservation. Implementations MUST lock the wallet row and
    /// perform the uniqueness check and insert in the same transaction.
    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome>;

    /// Captures a pending reservation and writes the immutable debit ledger
    /// entry atomically. Idempotent for an already-captured reservation.
    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction>;

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
}

#[async_trait]
impl<T> WalletRepository for Arc<T>
where
    T: WalletRepository + Send + Sync + ?Sized,
{
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        (**self).get_or_create(tenant_id, currency).await
    }
    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>> {
        (**self).list_transactions(tenant_id, limit).await
    }
    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        (**self)
            .reserve(tenant_id, amount_units, operation, request_id, expires_at)
            .await
    }
    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction> {
        (**self).capture(reservation_id).await
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
}
