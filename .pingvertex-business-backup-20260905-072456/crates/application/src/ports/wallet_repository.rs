use std::error::Error;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{CreditReservation, Wallet, WalletTransaction, WalletTransactionType};
use uuid::Uuid;

pub type WalletRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[async_trait]
pub trait WalletRepository: Send + Sync {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet>;
    async fn list_transactions(&self, tenant_id: Uuid, limit: u32) -> WalletRepoResult<Vec<WalletTransaction>>;

    /// Atomically reserves available credits. Implementations MUST lock the
    /// wallet row and create the reservation in the same transaction.
    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<CreditReservation>;

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
