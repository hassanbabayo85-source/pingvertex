use application::ports::{ReservationOutcome, WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{CreditReservation, Wallet, WalletTransaction, WalletTransactionType};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresWalletRepository {
    pool: PgPool,
}
impl PostgresWalletRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn parse_tx_type(v: &str) -> WalletTransactionType {
    match v {
        "TOP_UP" => WalletTransactionType::TopUp,
        "DEBIT" => WalletTransactionType::Debit,
        "REFUND" => WalletTransactionType::Refund,
        "ADJUSTMENT" => WalletTransactionType::Adjustment,
        "PROMOTIONAL_CREDIT" => WalletTransactionType::PromotionalCredit,
        "EXPIRATION" => WalletTransactionType::Expiration,
        _ => WalletTransactionType::Adjustment,
    }
}

#[derive(FromRow)]
struct WalletRow {
    id: Uuid,
    tenant_id: Uuid,
    currency: String,
    balance_units: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl WalletRow {
    fn into_domain(self) -> Wallet {
        Wallet {
            id: self.id,
            tenant_id: self.tenant_id,
            currency: self.currency,
            balance_units: self.balance_units,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(FromRow)]
struct TxRow {
    id: Uuid,
    tenant_id: Uuid,
    wallet_id: Uuid,
    transaction_type: String,
    amount_units: i64,
    balance_after_units: i64,
    idempotency_key: String,
    reference_type: Option<String>,
    reference_id: Option<String>,
    description: Option<String>,
    created_at: DateTime<Utc>,
}
impl TxRow {
    fn into_domain(self) -> WalletTransaction {
        WalletTransaction {
            id: self.id,
            tenant_id: self.tenant_id,
            wallet_id: self.wallet_id,
            transaction_type: parse_tx_type(&self.transaction_type),
            amount_units: self.amount_units,
            balance_after_units: self.balance_after_units,
            idempotency_key: self.idempotency_key,
            reference_type: self.reference_type,
            reference_id: self.reference_id,
            description: self.description,
            created_at: self.created_at,
        }
    }
}

#[derive(FromRow)]
struct ReservationRow {
    id: Uuid,
    wallet_id: Uuid,
    amount_units: i64,
    status: String,
    operation: String,
    request_id: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl ReservationRow {
    fn into_domain(self) -> CreditReservation {
        CreditReservation {
            id: self.id,
            wallet_id: self.wallet_id,
            amount_units: self.amount_units,
            status: self.status,
            operation: self.operation,
            request_id: self.request_id,
            expires_at: self.expires_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

const TX_COLUMNS: &str = "id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description,created_at";

#[async_trait]
impl WalletRepository for PostgresWalletRepository {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO user_wallets(id,tenant_id,currency) VALUES($1,$2,$3)
             ON CONFLICT(tenant_id) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(currency)
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }

    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>> {
        let query = format!(
            "SELECT t.{TX_COLUMNS} FROM wallet_transactions t
             WHERE t.tenant_id=$1 ORDER BY t.created_at DESC LIMIT $2"
        );
        let rows = sqlx::query_as::<_, TxRow>(&query)
            .bind(tenant_id)
            .bind(limit.clamp(1, 100) as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(TxRow::into_domain).collect())
    }

    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        if amount_units <= 0 {
            return Err("reservation amount must be positive".into());
        }
        if request_id.trim().is_empty() {
            return Err("request id is required".into());
        }
        let mut tx = self.pool.begin().await?;
        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;

        if let Some(existing) = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at
             FROM credit_reservations WHERE wallet_id=$1 AND request_id=$2",
        ).bind(wallet.id).bind(request_id).fetch_optional(&mut *tx).await? {
            tx.commit().await?;
            return Ok(ReservationOutcome { reservation: existing.into_domain(), created: false });
        }

        let reserved_units: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_units),0)::BIGINT
             FROM credit_reservations
             WHERE wallet_id=$1 AND status='PENDING' AND expires_at>now()",
        )
        .bind(wallet.id)
        .fetch_one(&mut *tx)
        .await?;
        let available = wallet
            .balance_units
            .checked_sub(reserved_units)
            .ok_or("wallet reservation accounting overflow")?;
        if available < amount_units {
            return Err("insufficient wallet balance".into());
        }

        let row = sqlx::query_as::<_, ReservationRow>(
            "INSERT INTO credit_reservations
             (id,wallet_id,amount_units,status,operation,request_id,expires_at)
             VALUES($1,$2,$3,'PENDING',$4,$5,$6)
             RETURNING id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at",
        ).bind(Uuid::new_v4()).bind(wallet.id).bind(amount_units).bind(operation)
         .bind(request_id).bind(expires_at).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(ReservationOutcome {
            reservation: row.into_domain(),
            created: true,
        })
    }

    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction> {
        let mut tx = self.pool.begin().await?;
        let r = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,wallet_id,amount_units,status,operation,request_id,expires_at,created_at,updated_at
             FROM credit_reservations WHERE id=$1 FOR UPDATE",
        ).bind(reservation_id).fetch_optional(&mut *tx).await?
            .ok_or("reservation not found")?;

        let idem = format!("reservation:{reservation_id}");
        if r.status == CreditReservation::CAPTURED {
            let q =
                format!("SELECT {TX_COLUMNS} FROM wallet_transactions WHERE idempotency_key=$1");
            let row = sqlx::query_as::<_, TxRow>(&q)
                .bind(&idem)
                .fetch_one(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        if r.status != CreditReservation::PENDING || r.expires_at <= Utc::now() {
            return Err("reservation is not capturable".into());
        }

        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE id=$1 FOR UPDATE",
        )
        .bind(r.wallet_id)
        .fetch_one(&mut *tx)
        .await?;
        let new_balance = wallet
            .balance_units
            .checked_sub(r.amount_units)
            .ok_or("insufficient wallet balance")?;
        sqlx::query("UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1")
            .bind(wallet.id)
            .bind(new_balance)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE credit_reservations SET status='CAPTURED',updated_at=now() WHERE id=$1",
        )
        .bind(r.id)
        .execute(&mut *tx)
        .await?;

        let q = format!(
            "INSERT INTO wallet_transactions
             (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description)
             VALUES($1,$2,$3,'DEBIT',$4,$5,$6,'credit_reservation',$7,$8)
             RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(wallet.tenant_id)
            .bind(wallet.id)
            .bind(-r.amount_units)
            .bind(new_balance)
            .bind(&idem)
            .bind(r.id.to_string())
            .bind(r.operation)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }

    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        sqlx::query(
            "UPDATE credit_reservations SET status='RELEASED',updated_at=now()
             WHERE id=$1 AND status='PENDING'",
        )
        .bind(reservation_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if amount_units <= 0 {
            return Err("credit amount must be positive".into());
        }
        if idempotency_key.trim().is_empty() {
            return Err("idempotency key is required".into());
        }
        let mut tx = self.pool.begin().await?;
        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;
        let q = format!(
            "SELECT {TX_COLUMNS} FROM wallet_transactions WHERE tenant_id=$1 AND idempotency_key=$2"
        );
        if let Some(row) = sqlx::query_as::<_, TxRow>(&q)
            .bind(tenant_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *tx)
            .await?
        {
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        let new_balance = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;
        sqlx::query("UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1")
            .bind(wallet.id)
            .bind(new_balance)
            .execute(&mut *tx)
            .await?;
        let q = format!(
            "INSERT INTO wallet_transactions
             (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,description)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(wallet.id)
            .bind(transaction_type.as_str())
            .bind(amount_units)
            .bind(new_balance)
            .bind(idempotency_key)
            .bind(description)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }
}
