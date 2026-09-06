use application::ports::{ReservationOutcome, WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{CreditReservation, Wallet, WalletSettings, WalletTransaction, WalletTransactionType};
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
    async fn get_settings(&self, tenant_id: Uuid) -> WalletRepoResult<WalletSettings> {
        #[derive(sqlx::FromRow)]
        struct SettingsRow {
            low_balance_threshold_units: i64,
            critical_balance_threshold_units: i64,
            auto_top_up_enabled: bool,
            auto_top_up_threshold_units: i64,
            auto_top_up_amount_units: i64,
            payment_method: Option<String>,
            daily_top_up_limit_units: i64,
            monthly_top_up_limit_units: i64,
            retry_limit: i32,
        }
        let r = sqlx::query_as::<_, SettingsRow>("SELECT low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,retry_limit FROM wallet_settings WHERE tenant_id=$1").bind(tenant_id).fetch_optional(&self.pool).await?;
        Ok(r.map(|x| WalletSettings {
            tenant_id,
            low_balance_threshold_units: x.low_balance_threshold_units,
            critical_balance_threshold_units: x.critical_balance_threshold_units,
            auto_top_up_enabled: x.auto_top_up_enabled,
            auto_top_up_threshold_units: x.auto_top_up_threshold_units,
            auto_top_up_amount_units: x.auto_top_up_amount_units,
            payment_method: x.payment_method,
            daily_top_up_limit_units: x.daily_top_up_limit_units,
            monthly_top_up_limit_units: x.monthly_top_up_limit_units,
            retry_limit: x.retry_limit,
        })
        .unwrap_or_else(|| {
            let mut x = WalletSettings::default();
            x.tenant_id = tenant_id;
            x
        }))
    }
    async fn update_settings(&self, settings: &WalletSettings) -> WalletRepoResult<WalletSettings> {
        if settings.low_balance_threshold_units < 0
            || settings.critical_balance_threshold_units < 0
            || settings.auto_top_up_threshold_units < 0
            || settings.auto_top_up_amount_units <= 0
            || settings.daily_top_up_limit_units <= 0
            || settings.monthly_top_up_limit_units <= 0
            || settings.retry_limit < 0
            || settings.critical_balance_threshold_units > settings.low_balance_threshold_units
        {
            return Err("invalid wallet settings".into());
        }
        sqlx::query("INSERT INTO wallet_settings (tenant_id,low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,retry_limit) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(tenant_id) DO UPDATE SET low_balance_threshold_units=EXCLUDED.low_balance_threshold_units,critical_balance_threshold_units=EXCLUDED.critical_balance_threshold_units,auto_top_up_enabled=EXCLUDED.auto_top_up_enabled,auto_top_up_threshold_units=EXCLUDED.auto_top_up_threshold_units,auto_top_up_amount_units=EXCLUDED.auto_top_up_amount_units,payment_method=EXCLUDED.payment_method,daily_top_up_limit_units=EXCLUDED.daily_top_up_limit_units,monthly_top_up_limit_units=EXCLUDED.monthly_top_up_limit_units,retry_limit=EXCLUDED.retry_limit,updated_at=now()").bind(settings.tenant_id).bind(settings.low_balance_threshold_units).bind(settings.critical_balance_threshold_units).bind(settings.auto_top_up_enabled).bind(settings.auto_top_up_threshold_units).bind(settings.auto_top_up_amount_units).bind(&settings.payment_method).bind(settings.daily_top_up_limit_units).bind(settings.monthly_top_up_limit_units).bind(settings.retry_limit).execute(&self.pool).await?;
        Ok(settings.clone())
    }
    async fn expire_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<Option<WalletTransaction>> {
        if amount_units <= 0 || idempotency_key.trim().is_empty() {
            return Ok(None);
        }
        let mut tx = self.pool.begin().await?;
        let wallet=sqlx::query_as::<_,WalletRow>("SELECT id,tenant_id,currency,balance_units,created_at,updated_at FROM user_wallets WHERE tenant_id=$1 FOR UPDATE").bind(tenant_id).fetch_one(&mut *tx).await?;
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
            return Ok(Some(row.into_domain()));
        }
        let debit = amount_units.min(wallet.balance_units);
        if debit == 0 {
            tx.commit().await?;
            return Ok(None);
        }
        let new_balance = wallet.balance_units - debit;
        sqlx::query("UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1")
            .bind(wallet.id)
            .bind(new_balance)
            .execute(&mut *tx)
            .await?;
        let q = format!(
            "INSERT INTO wallet_transactions (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,description) VALUES($1,$2,$3,'EXPIRATION',$4,$5,$6,$7) RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(wallet.id)
            .bind(-debit)
            .bind(new_balance)
            .bind(idempotency_key)
            .bind(description)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(row.into_domain()))
    }

    async fn grant_promotional_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        expires_at: DateTime<Utc>,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if amount_units <= 0 || expires_at <= Utc::now() || idempotency_key.trim().is_empty() {
            return Err("invalid promotional credit".into());
        }
        let mut tx = self.pool.begin().await?;
        let wallet=sqlx::query_as::<_,WalletRow>("SELECT id,tenant_id,currency,balance_units,created_at,updated_at FROM user_wallets WHERE tenant_id=$1 FOR UPDATE").bind(tenant_id).fetch_one(&mut *tx).await?;
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
        sqlx::query("INSERT INTO promotional_credits(id,tenant_id,wallet_id,original_amount_units,remaining_amount_units,expires_at,status,idempotency_key,created_at,updated_at) VALUES($1,$2,$3,$4,$4,$5,'ACTIVE',$6,now(),now())").bind(Uuid::new_v4()).bind(tenant_id).bind(wallet.id).bind(amount_units).bind(expires_at).bind(idempotency_key).execute(&mut *tx).await?;
        let q = format!(
            "INSERT INTO wallet_transactions (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,description) VALUES($1,$2,$3,'PROMOTIONAL_CREDIT',$4,$5,$6,$7) RETURNING {TX_COLUMNS}"
        );
        let row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(wallet.id)
            .bind(amount_units)
            .bind(new_balance)
            .bind(idempotency_key)
            .bind(description)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(row.into_domain())
    }
    async fn expire_promotional_credits(&self, limit: u32) -> WalletRepoResult<u64> {
        let rows=sqlx::query_as::<_,(Uuid,Uuid,i64,String)>("SELECT id,tenant_id,remaining_amount_units,idempotency_key FROM promotional_credits WHERE status='ACTIVE' AND expires_at<=now() AND remaining_amount_units>0 ORDER BY expires_at FOR UPDATE SKIP LOCKED LIMIT $1").bind(limit.clamp(1,500) as i64).fetch_all(&self.pool).await?;
        let mut n = 0;
        for (id, tenant, remaining, key) in rows {
            let idem = format!("expiration:{id}");
            let _ = self
                .expire_credit(tenant, remaining, &idem, Some("Promotional credit expired"))
                .await?;
            sqlx::query("UPDATE promotional_credits SET remaining_amount_units=0,status='EXPIRED',updated_at=now() WHERE id=$1 AND status='ACTIVE'").bind(id).execute(&self.pool).await?;
            let _ = key;
            n += 1;
        }
        Ok(n)
    }

    async fn auto_top_up_candidates(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<domain::AutoTopUpCandidate>> {
        #[derive(sqlx::FromRow)]
        struct AutoTopUpRow {
            tenant_id: Uuid,
            balance_units: i64,
            low: i64,
            critical: i64,
            enabled: bool,
            threshold: i64,
            amount: i64,
            payment_method: Option<String>,
            daily_limit: i64,
            monthly_limit: i64,
            retry_limit: i32,
            daily: i64,
            monthly: i64,
            attempts: i64,
        }
        let rows=sqlx::query_as::<_,AutoTopUpRow>("SELECT w.tenant_id,w.balance_units,s.low_balance_threshold_units low,s.critical_balance_threshold_units critical,s.auto_top_up_enabled enabled,s.auto_top_up_threshold_units threshold,s.auto_top_up_amount_units amount,s.payment_method,s.daily_top_up_limit_units daily_limit,s.monthly_top_up_limit_units monthly_limit,s.retry_limit,(SELECT COALESCE(SUM(t.amount_units),0) FROM wallet_transactions t WHERE t.tenant_id=w.tenant_id AND t.transaction_type='TOP_UP' AND t.reference_type='auto_top_up' AND t.created_at>=date_trunc('day',now())) daily,(SELECT COALESCE(SUM(t.amount_units),0) FROM wallet_transactions t WHERE t.tenant_id=w.tenant_id AND t.transaction_type='TOP_UP' AND t.reference_type='auto_top_up' AND t.created_at>=date_trunc('month',now())) monthly,(SELECT COUNT(*) FROM wallet_transactions t WHERE t.tenant_id=w.tenant_id AND t.transaction_type='TOP_UP' AND t.reference_type='auto_top_up' AND t.created_at>=date_trunc('day',now())) attempts FROM user_wallets w JOIN wallet_settings s ON s.tenant_id=w.tenant_id WHERE s.auto_top_up_enabled AND w.balance_units < s.auto_top_up_threshold_units ORDER BY w.balance_units LIMIT $1").bind(limit.clamp(1,100) as i64).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| domain::AutoTopUpCandidate {
                tenant_id: r.tenant_id,
                balance_units: r.balance_units,
                settings: WalletSettings {
                    tenant_id: r.tenant_id,
                    low_balance_threshold_units: r.low,
                    critical_balance_threshold_units: r.critical,
                    auto_top_up_enabled: r.enabled,
                    auto_top_up_threshold_units: r.threshold,
                    auto_top_up_amount_units: r.amount,
                    payment_method: r.payment_method,
                    daily_top_up_limit_units: r.daily_limit,
                    monthly_top_up_limit_units: r.monthly_limit,
                    retry_limit: r.retry_limit,
                },
                daily_spend_units: r.daily,
                monthly_spend_units: r.monthly,
                daily_attempts: r.attempts as i32,
            })
            .collect())
    }

    async fn record_balance_notification(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        dedupe_key: &str,
        message: &str,
    ) -> WalletRepoResult<bool> {
        let n=sqlx::query("INSERT INTO wallet_notifications(id,tenant_id,event_type,dedupe_key,message) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant_id,event_type,dedupe_key) DO NOTHING").bind(Uuid::new_v4()).bind(tenant_id).bind(event_type).bind(dedupe_key).bind(message).execute(&self.pool).await?;
        Ok(n.rows_affected() == 1)
    }
    async fn list_balance_notifications(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<domain::WalletNotification>> {
        #[derive(sqlx::FromRow)]
        struct WalletNotificationRow {
            id: Uuid,
            tenant_id: Uuid,
            event_type: String,
            dedupe_key: String,
            message: String,
            read_at: Option<DateTime<Utc>>,
            created_at: DateTime<Utc>,
        }
        let rows=sqlx::query_as::<_,WalletNotificationRow>("SELECT id,tenant_id,event_type,dedupe_key,message,read_at,created_at FROM wallet_notifications WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT $2").bind(tenant_id).bind(limit.clamp(1,100) as i64).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| domain::WalletNotification {
                id: r.id,
                tenant_id: r.tenant_id,
                event_type: r.event_type,
                dedupe_key: r.dedupe_key,
                message: r.message,
                read_at: r.read_at,
                created_at: r.created_at,
            })
            .collect())
    }
}
