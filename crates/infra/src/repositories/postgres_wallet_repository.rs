use application::ports::{ReservationOutcome, WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    AutoTopUpAttempt, AutoTopUpCandidate, CreditReservation, Wallet, WalletSettings,
    WalletTopUpSession, WalletTransaction, WalletTransactionType,
};
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
    payg_activated_at: Option<DateTime<Utc>>,
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
            payg_activated_at: self.payg_activated_at,
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
    usage_event_id: Option<Uuid>,
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
            usage_event_id: self.usage_event_id,
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
    unit_price: Option<i64>,
    pricing_version: Option<i64>,
    currency: Option<String>,
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
            unit_price: self.unit_price,
            pricing_version: self.pricing_version,
            currency: self.currency,
            request_id: self.request_id,
            expires_at: self.expires_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

const TX_COLUMNS: &str = "id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description,usage_event_id,created_at";

fn validate_usage_for_reservation(
    event: &domain::UsageEvent,
    reservation: &ReservationRow,
    tenant_id: Uuid,
) -> WalletRepoResult<()> {
    if event.tenant_id != tenant_id
        || event.request_id != reservation.request_id
        || event.operation != reservation.operation
        || event.charge_units != reservation.amount_units
        || event.charge_units <= 0
        || event.units_requested <= 0
        || event.units_billed <= 0
        || event.units_billed > event.units_requested
        || event.unit_price <= 0
        || event.status != "CAPTURED"
        || event.charge_units
            != event
                .units_billed
                .checked_mul(event.unit_price)
                .ok_or("usage charge overflow")?
        || reservation
            .unit_price
            .is_some_and(|price| price != event.unit_price)
        || reservation
            .pricing_version
            .is_some_and(|version| version != event.pricing_version)
        || reservation
            .currency
            .as_deref()
            .is_some_and(|currency| currency != event.currency)
    {
        return Err("usage event does not match PAYG reservation".into());
    }
    Ok(())
}

#[async_trait]
impl WalletRepository for PostgresWalletRepository {
    async fn find(&self, tenant_id: Uuid) -> WalletRepoResult<Option<Wallet>> {
        let row = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(WalletRow::into_domain))
    }

    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        // This endpoint is read-heavy. `FOR UPDATE` here needlessly serialized
        // concurrent dashboard reads against the wallet row. The unique
        // tenant constraint makes the insert race-safe; the following plain
        // SELECT is sufficient because callers are not mutating the wallet.
        sqlx::query(
            "INSERT INTO user_wallets(id,tenant_id,currency) VALUES($1,$2,$3)
             ON CONFLICT(tenant_id) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(currency)
        .execute(&self.pool)
        .await?;
        let row = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1",
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into_domain())
    }

    async fn get_balance_summary(
        &self,
        tenant_id: Uuid,
        currency: &str,
    ) -> WalletRepoResult<domain::WalletBalanceSummary> {
        let balance: i64 = sqlx::query_scalar(
            "SELECT balance_units FROM user_wallets WHERE tenant_id=$1 AND currency=$2",
        )
        .bind(tenant_id)
        .bind(currency)
        .fetch_optional(&self.pool)
        .await?
        .ok_or("wallet not found")?;
        let reserved: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(amount_units),0) FROM credit_reservations WHERE wallet_id=(SELECT id FROM user_wallets WHERE tenant_id=$1 AND currency=$2) AND status='PENDING' AND expires_at>now()")
            .bind(tenant_id).bind(currency).fetch_one(&self.pool).await?;
        Ok(domain::WalletBalanceSummary {
            balance_units: balance,
            reserved_units: reserved,
            available_units: balance.saturating_sub(reserved),
        })
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

    async fn find_reservation_by_request(
        &self,
        tenant_id: Uuid,
        request_id: &str,
    ) -> WalletRepoResult<Option<CreditReservation>> {
        let row = sqlx::query_as::<_, ReservationRow>(
            "SELECT r.id,r.wallet_id,r.amount_units,r.status,r.operation,r.unit_price,r.pricing_version,r.currency,r.request_id,r.expires_at,r.created_at,r.updated_at
             FROM credit_reservations r
             JOIN user_wallets w ON w.id=r.wallet_id
             WHERE w.tenant_id=$1 AND r.request_id=$2"
        )
        .bind(tenant_id)
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ReservationRow::into_domain))
    }

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
        if amount_units <= 0 {
            return Err("reservation amount must be positive".into());
        }
        if request_id.trim().is_empty() {
            return Err("request id is required".into());
        }
        let mut tx = self.pool.begin().await?;
        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;

        let mut expired_reservation_id = None;

        if let Some(existing) = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,wallet_id,amount_units,status,operation,unit_price,pricing_version,currency,request_id,expires_at,created_at,updated_at
             FROM credit_reservations WHERE wallet_id=$1 AND request_id=$2",
        ).bind(wallet.id).bind(request_id).fetch_optional(&mut *tx).await? {
            if (existing.status == CreditReservation::PENDING && existing.expires_at <= Utc::now())
                || existing.status == CreditReservation::EXPIRED
            {
                // Do not reopen an expired reservation before spend-limit checks.
                // The request must pass the same enforcement as a new reservation.
                expired_reservation_id = Some(existing.id);
            } else {
                tx.commit().await?;
                return Ok(ReservationOutcome { reservation: existing.into_domain(), created: false });
            }
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

        // Spend limits are enforced while the wallet row is locked. Pending,
        // unexpired reservations count toward the limit so concurrent workers
        // cannot all authorize the same remaining daily/monthly budget before
        // capture. The current reservation is not yet inserted at this point.
        #[derive(sqlx::FromRow)]
        struct SpendLimits {
            daily_spend_limit_units: Option<i64>,
            monthly_spend_limit_units: Option<i64>,
        }
        let limits = sqlx::query_as::<_, SpendLimits>(
            "SELECT daily_spend_limit_units, monthly_spend_limit_units
             FROM wallet_settings WHERE tenant_id=$1",
        )
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(SpendLimits {
            daily_spend_limit_units: None,
            monthly_spend_limit_units: None,
        });

        if limits.daily_spend_limit_units.is_some() || limits.monthly_spend_limit_units.is_some() {
            let captured_today: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(charge_units),0)::BIGINT
                 FROM api_usage_events
                 WHERE tenant_id=$1 AND status='CAPTURED'
                   AND created_at >= date_trunc('day', now())
                   AND created_at < date_trunc('day', now()) + interval '1 day'",
            )
            .bind(tenant_id)
            .fetch_one(&mut *tx)
            .await?;
            let captured_month: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(charge_units),0)::BIGINT
                 FROM api_usage_events
                 WHERE tenant_id=$1 AND status='CAPTURED'
                   AND created_at >= date_trunc('month', now())
                   AND created_at < date_trunc('month', now()) + interval '1 month'",
            )
            .bind(tenant_id)
            .fetch_one(&mut *tx)
            .await?;
            let pending_today: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_units),0)::BIGINT
                 FROM credit_reservations
                 WHERE wallet_id=$1 AND status='PENDING' AND expires_at>now()
                   AND created_at >= date_trunc('day', now())
                   AND created_at < date_trunc('day', now()) + interval '1 day'",
            )
            .bind(wallet.id)
            .fetch_one(&mut *tx)
            .await?;
            let pending_month: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_units),0)::BIGINT
                 FROM credit_reservations
                 WHERE wallet_id=$1 AND status='PENDING' AND expires_at>now()
                   AND created_at >= date_trunc('month', now())
                   AND created_at < date_trunc('month', now()) + interval '1 month'",
            )
            .bind(wallet.id)
            .fetch_one(&mut *tx)
            .await?;

            let today_after = captured_today
                .checked_add(pending_today)
                .and_then(|v| v.checked_add(amount_units))
                .ok_or("wallet daily spend accounting overflow")?;
            let month_after = captured_month
                .checked_add(pending_month)
                .and_then(|v| v.checked_add(amount_units))
                .ok_or("wallet monthly spend accounting overflow")?;

            if let Some(limit) = limits.daily_spend_limit_units
                && today_after > limit
            {
                tx.commit().await?;
                return Err("wallet daily spend limit exceeded".into());
            }
            if let Some(limit) = limits.monthly_spend_limit_units
                && month_after > limit
            {
                tx.commit().await?;
                return Err("wallet monthly spend limit exceeded".into());
            }
        }

        if available < amount_units {
            // Persist the billing block in the same wallet transaction. This is
            // durable state, not merely a UI-derived balance label, so every
            // worker/region observes the same decision.
            if wallet.payg_activated_at.is_some() {
                sqlx::query(
                    "UPDATE user_wallets
                     SET billing_status='PAYG_BALANCE_EXHAUSTED', updated_at=now()
                     WHERE id=$1",
                )
                .bind(wallet.id)
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
            return Err("insufficient wallet balance".into());
        }

        // A successful authorization is also the durable resume transition.
        sqlx::query(
            "UPDATE user_wallets
             SET billing_status='ACTIVE', updated_at=now()
             WHERE id=$1",
        )
        .bind(wallet.id)
        .execute(&mut *tx)
        .await?;

        let row = if let Some(reservation_id) = expired_reservation_id {
            sqlx::query_as::<_, ReservationRow>(
                "UPDATE credit_reservations
                 SET amount_units=$2, operation=$3, unit_price=$4, pricing_version=$5,
                     currency=$6, request_id=$7, expires_at=$8, status='PENDING', updated_at=now()
                 WHERE id=$1
                 RETURNING id,wallet_id,amount_units,status,operation,unit_price,pricing_version,currency,request_id,expires_at,created_at,updated_at",
            )
            .bind(reservation_id)
            .bind(amount_units)
            .bind(operation)
            .bind(unit_price)
            .bind(pricing_version)
            .bind(currency)
            .bind(request_id)
            .bind(expires_at)
            .fetch_one(&mut *tx)
            .await?
        } else {
            sqlx::query_as::<_, ReservationRow>(
                "INSERT INTO credit_reservations
                 (id,wallet_id,amount_units,status,operation,unit_price,pricing_version,currency,request_id,expires_at)
                 VALUES ($1,$2,$3,'PENDING',$4,$5,$6,$7,$8,$9)
                 RETURNING id,wallet_id,amount_units,status,operation,unit_price,pricing_version,currency,request_id,expires_at,created_at,updated_at",
            )
            .bind(Uuid::new_v4())
            .bind(wallet.id)
            .bind(amount_units)
            .bind(operation)
            .bind(unit_price)
            .bind(pricing_version)
            .bind(currency)
            .bind(request_id)
            .bind(expires_at)
            .fetch_one(&mut *tx)
            .await?
        };

        tx.commit().await?;
        Ok(ReservationOutcome {
            reservation: row.into_domain(),
            created: true,
        })
    }

    async fn capture(
        &self,
        reservation_id: Uuid,
        usage_event: Option<&domain::UsageEvent>,
        audit_entries: &[domain::FinancialAuditLogEntry],
    ) -> WalletRepoResult<WalletTransaction> {
        let mut tx = self.pool.begin().await?;
        let r = sqlx::query_as::<_, ReservationRow>(
            "SELECT id,wallet_id,amount_units,status,operation,unit_price,pricing_version,currency,request_id,expires_at,created_at,updated_at
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
            if let Some(event) = usage_event {
                validate_usage_for_reservation(event, &r, row.tenant_id)?;
                sqlx::query(
                    "INSERT INTO api_usage_events
                     (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,pricing_version,request_id,monitor_id,execution_id,status)
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                     ON CONFLICT (tenant_id,request_id) DO NOTHING",
                )
                .bind(event.id).bind(event.tenant_id).bind(&event.operation)
                .bind(event.units_requested).bind(event.units_billed).bind(event.unit_price)
                .bind(event.charge_units).bind(&event.currency).bind(event.pricing_version)
                .bind(&event.request_id).bind(event.monitor_id).bind(event.execution_id).bind(&event.status)
                .execute(&mut *tx).await?;
            }
            for audit in audit_entries {
                sqlx::query(
                    "INSERT INTO financial_audit_log(id,actor,tenant_id,action,resource,resource_id,created_at,metadata)
                     SELECT $1,$2,$3,$4,$5,$6,$7,$8
                     WHERE NOT EXISTS (
                         SELECT 1 FROM financial_audit_log
                         WHERE action=$4 AND resource=$5 AND resource_id=$6
                     )",
                )
                .bind(audit.id).bind(&audit.actor).bind(audit.tenant_id).bind(&audit.action)
                .bind(&audit.resource).bind(&audit.resource_id).bind(audit.created_at).bind(&audit.metadata)
                .execute(&mut *tx).await?;
            }
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        if r.status != CreditReservation::PENDING || r.expires_at <= Utc::now() {
            return Err("reservation is not capturable".into());
        }

        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at
             FROM user_wallets WHERE id=$1 FOR UPDATE",
        )
        .bind(r.wallet_id)
        .fetch_one(&mut *tx)
        .await?;
        let new_balance = wallet
            .balance_units
            .checked_sub(r.amount_units)
            .ok_or("insufficient wallet balance")?;

        // Promotional credits are part of the wallet balance, but they expire.
        // Consume the oldest active promotional credits first so expiration can
        // never claw back cash that was already purchased.
        let mut promo_remaining = r.amount_units;
        let promo_rows = sqlx::query_as::<_, (Uuid, i64)>(
            "SELECT id, remaining_amount_units
             FROM promotional_credits
             WHERE wallet_id=$1 AND status='ACTIVE' AND remaining_amount_units>0
               AND expires_at>now()
             ORDER BY expires_at ASC, created_at ASC
             FOR UPDATE",
        )
        .bind(wallet.id)
        .fetch_all(&mut *tx)
        .await?;
        for (promo_id, remaining) in promo_rows {
            if promo_remaining == 0 {
                break;
            }
            let used = remaining.min(promo_remaining);
            let left = remaining - used;
            sqlx::query(
                "UPDATE promotional_credits
                 SET remaining_amount_units=$2,
                     status=CASE WHEN $2=0 THEN 'EXPIRED' ELSE status END,
                     updated_at=now()
                 WHERE id=$1",
            )
            .bind(promo_id)
            .bind(left)
            .execute(&mut *tx)
            .await?;
            promo_remaining -= used;
        }

        sqlx::query(
            "UPDATE user_wallets
             SET balance_units=$2,
                 billing_status=CASE WHEN $2 <= 0 THEN 'PAYG_BALANCE_EXHAUSTED' ELSE 'ACTIVE' END,
                 updated_at=now()
             WHERE id=$1",
        )
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
        let mut row = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(wallet.tenant_id)
            .bind(wallet.id)
            .bind(-r.amount_units)
            .bind(new_balance)
            .bind(&idem)
            .bind(r.id.to_string())
            .bind(r.operation.clone())
            .fetch_one(&mut *tx)
            .await?;
        if let Some(event) = usage_event {
            validate_usage_for_reservation(event, &r, wallet.tenant_id)?;
            row.usage_event_id = Some(event.id);
            sqlx::query(
                "INSERT INTO api_usage_events
                 (id,tenant_id,operation,units_requested,units_billed,unit_price,charge_units,currency,pricing_version,request_id,monitor_id,execution_id,status)
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                 ON CONFLICT (tenant_id,request_id) DO NOTHING",
            )
            .bind(event.id).bind(event.tenant_id).bind(&event.operation)
            .bind(event.units_requested).bind(event.units_billed).bind(event.unit_price)
            .bind(event.charge_units).bind(&event.currency).bind(event.pricing_version)
            .bind(&event.request_id).bind(event.monitor_id).bind(event.execution_id).bind(&event.status)
            .execute(&mut *tx).await?;
            sqlx::query("UPDATE wallet_transactions SET usage_event_id=$2 WHERE id=$1 AND usage_event_id IS NULL")
                .bind(row.id)
                .bind(event.id)
                .execute(&mut *tx)
                .await?;
        }
        for audit in audit_entries {
            sqlx::query(
                "INSERT INTO financial_audit_log(id,actor,tenant_id,action,resource,resource_id,created_at,metadata)
                 SELECT $1,$2,$3,$4,$5,$6,$7,$8
                 WHERE NOT EXISTS (
                     SELECT 1 FROM financial_audit_log
                     WHERE action=$4 AND resource=$5 AND resource_id=$6
                 )",
            )
            .bind(audit.id).bind(&audit.actor).bind(audit.tenant_id).bind(&audit.action)
            .bind(&audit.resource).bind(&audit.resource_id).bind(audit.created_at).bind(&audit.metadata)
            .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        let domain_row = row.into_domain();

        // Record threshold transitions close to the actual debit, not only when
        // a user opens the wallet page. This makes zero/critical/low alerts
        // durable even when usage is generated by background jobs.
        let Ok(settings) = self.get_settings(wallet.tenant_id).await else {
            return Ok(domain_row);
        };
        let level = if new_balance <= 0 {
            "BLOCKED"
        } else if new_balance <= settings.critical_balance_threshold_units {
            "CRITICAL"
        } else if new_balance <= settings.low_balance_threshold_units {
            "WARNING"
        } else {
            "OK"
        };
        if level != "OK" {
            let _ = self
                .record_balance_notification(
                    wallet.tenant_id,
                    level,
                    &format!("{}:{}", level, Utc::now().date_naive()),
                    match level {
                        "BLOCKED" => "PAYG is blocked because your wallet balance is zero.",
                        "CRITICAL" => "Your PAYG wallet balance is critically low.",
                        _ => "Your PAYG wallet balance is low.",
                    },
                )
                .await;
        }
        Ok(domain_row)
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
            "SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at
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
        let activate_payg = matches!(
            transaction_type,
            WalletTransactionType::TopUp | WalletTransactionType::PromotionalCredit
        );
        let should_resume_blocked_monitors = wallet.payg_activated_at.is_some() || activate_payg;
        sqlx::query(
            "UPDATE user_wallets
             SET balance_units=$2,
                 payg_activated_at=CASE WHEN $3 THEN COALESCE(payg_activated_at, now()) ELSE payg_activated_at END,
                 billing_status='ACTIVE',
                 updated_at=now()
             WHERE id=$1",
        )
        .bind(wallet.id)
        .bind(new_balance)
        .bind(activate_payg)
        .execute(&mut *tx)
        .await?;

        // Funding is the durable resume transition. Only monitors that were
        // explicitly blocked by PAYG billing are resumed; user-disabled
        // monitors remain disabled. Scheduler staggering prevents a large
        // tenant from producing a thundering herd after top-up.
        if should_resume_blocked_monitors && new_balance > 0 {
            sqlx::query(
                r#"
                UPDATE monitors m
                SET next_due_at = now()
                    + ((abs(hashtextextended(m.id::text, 0)) % 60) * interval '1 second'),
                    updated_at = now()
                WHERE m.tenant_id = $1
                  AND EXISTS (
                      SELECT 1 FROM monitor_billing_blocks b
                      WHERE b.monitor_id = m.id
                  )
                "#,
            )
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM monitor_billing_blocks WHERE tenant_id = $1")
                .bind(tenant_id)
                .execute(&mut *tx)
                .await?;
        }

        // Funding is the durable resume transition. Only monitors that were
        // explicitly blocked by PAYG billing are resumed; user-disabled
        // monitors remain disabled. Scheduler staggering prevents a large
        // tenant from producing a thundering herd after top-up.
        if should_resume_blocked_monitors && new_balance > 0 {
            sqlx::query(
                r#"
                UPDATE monitors m
                SET next_due_at = now()
                    + ((abs(hashtextextended(m.id::text, 0)) % 60) * interval '1 second'),
                    updated_at = now()
                WHERE m.tenant_id = $1
                  AND EXISTS (
                      SELECT 1 FROM monitor_billing_blocks b
                      WHERE b.monitor_id = m.id
                  )
                "#,
            )
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM monitor_billing_blocks WHERE tenant_id = $1")
                .bind(tenant_id)
                .execute(&mut *tx)
                .await?;
        }
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
            daily_spend_limit_units: Option<i64>,
            monthly_spend_limit_units: Option<i64>,
            daily_usage_alert_threshold_units: Option<i64>,
            monthly_usage_alert_threshold_units: Option<i64>,
            retry_limit: i32,
        }
        let r = sqlx::query_as::<_, SettingsRow>("SELECT low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,daily_spend_limit_units,monthly_spend_limit_units,daily_usage_alert_threshold_units,monthly_usage_alert_threshold_units,retry_limit FROM wallet_settings WHERE tenant_id=$1").bind(tenant_id).fetch_optional(&self.pool).await?;
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
            daily_spend_limit_units: x.daily_spend_limit_units,
            monthly_spend_limit_units: x.monthly_spend_limit_units,
            daily_usage_alert_threshold_units: x.daily_usage_alert_threshold_units,
            monthly_usage_alert_threshold_units: x.monthly_usage_alert_threshold_units,
            retry_limit: x.retry_limit,
        })
        .unwrap_or_else(|| WalletSettings {
            tenant_id,
            ..WalletSettings::default()
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
            || settings
                .daily_usage_alert_threshold_units
                .is_some_and(|v| v <= 0)
            || settings
                .monthly_usage_alert_threshold_units
                .is_some_and(|v| v <= 0)
            || settings.critical_balance_threshold_units > settings.low_balance_threshold_units
        {
            return Err("invalid wallet settings".into());
        }
        sqlx::query("INSERT INTO wallet_settings (tenant_id,low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,daily_spend_limit_units,monthly_spend_limit_units,daily_usage_alert_threshold_units,monthly_usage_alert_threshold_units,retry_limit) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT(tenant_id) DO UPDATE SET low_balance_threshold_units=EXCLUDED.low_balance_threshold_units,critical_balance_threshold_units=EXCLUDED.critical_balance_threshold_units,auto_top_up_enabled=EXCLUDED.auto_top_up_enabled,auto_top_up_threshold_units=EXCLUDED.auto_top_up_threshold_units,auto_top_up_amount_units=EXCLUDED.auto_top_up_amount_units,payment_method=EXCLUDED.payment_method,daily_top_up_limit_units=EXCLUDED.daily_top_up_limit_units,monthly_top_up_limit_units=EXCLUDED.monthly_top_up_limit_units,daily_spend_limit_units=EXCLUDED.daily_spend_limit_units,monthly_spend_limit_units=EXCLUDED.monthly_spend_limit_units,daily_usage_alert_threshold_units=EXCLUDED.daily_usage_alert_threshold_units,monthly_usage_alert_threshold_units=EXCLUDED.monthly_usage_alert_threshold_units,retry_limit=EXCLUDED.retry_limit,updated_at=now()").bind(settings.tenant_id).bind(settings.low_balance_threshold_units).bind(settings.critical_balance_threshold_units).bind(settings.auto_top_up_enabled).bind(settings.auto_top_up_threshold_units).bind(settings.auto_top_up_amount_units).bind(&settings.payment_method).bind(settings.daily_top_up_limit_units).bind(settings.monthly_top_up_limit_units)
            .bind(settings.daily_spend_limit_units)
            .bind(settings.monthly_spend_limit_units)
            .bind(settings.daily_usage_alert_threshold_units)
            .bind(settings.monthly_usage_alert_threshold_units)
            .bind(settings.retry_limit).execute(&self.pool).await?;
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
        let wallet=sqlx::query_as::<_,WalletRow>("SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at FROM user_wallets WHERE tenant_id=$1 FOR UPDATE").bind(tenant_id).fetch_one(&mut *tx).await?;
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
        let wallet=sqlx::query_as::<_,WalletRow>("SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at FROM user_wallets WHERE tenant_id=$1 FOR UPDATE").bind(tenant_id).fetch_one(&mut *tx).await?;
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
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, (Uuid, Uuid)>(
            "SELECT id,tenant_id
             FROM promotional_credits
             WHERE status='ACTIVE' AND expires_at<=now() AND remaining_amount_units>0
             ORDER BY expires_at ASC
             LIMIT $1",
        )
        .bind(limit.clamp(1, 500) as i64)
        .fetch_all(&mut *tx)
        .await?;

        let mut count = 0_u64;
        for (promo_id, tenant_id) in rows {
            // Lock order is wallet -> promo, matching capture(), to avoid
            // wallet/promo deadlocks under concurrent expiry and usage.
            let wallet = sqlx::query_as::<_, WalletRow>(
                "SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at
                 FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
            )
            .bind(tenant_id)
            .fetch_one(&mut *tx)
            .await?;
            let remaining: i64 = sqlx::query_scalar(
                "SELECT remaining_amount_units FROM promotional_credits
                 WHERE id=$1 AND status='ACTIVE' AND expires_at<=now()
                 FOR UPDATE",
            )
            .bind(promo_id)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(0);
            if remaining <= 0 {
                continue;
            }
            let debit = remaining.min(wallet.balance_units);
            if debit > 0 {
                let new_balance = wallet.balance_units - debit;
                sqlx::query(
                    "UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1",
                )
                .bind(wallet.id)
                .bind(new_balance)
                .execute(&mut *tx)
                .await?;
                let idem = format!("expiration:{promo_id}");
                let q = format!(
                    "INSERT INTO wallet_transactions
                     (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description)
                     VALUES($1,$2,$3,'EXPIRATION',$4,$5,$6,'promotional_credit',$7,$8)
                     ON CONFLICT (tenant_id,idempotency_key) DO NOTHING
                     RETURNING {TX_COLUMNS}"
                );
                let _ = sqlx::query_as::<_, TxRow>(&q)
                    .bind(Uuid::new_v4())
                    .bind(tenant_id)
                    .bind(wallet.id)
                    .bind(-debit)
                    .bind(new_balance)
                    .bind(&idem)
                    .bind(promo_id.to_string())
                    .bind("Promotional credit expired")
                    .fetch_optional(&mut *tx)
                    .await?;
            }
            sqlx::query(
                "UPDATE promotional_credits
                 SET remaining_amount_units=0,status='EXPIRED',updated_at=now()
                 WHERE id=$1 AND status='ACTIVE'",
            )
            .bind(promo_id)
            .execute(&mut *tx)
            .await?;
            count += 1;
        }
        tx.commit().await?;
        Ok(count)
    }

    async fn expire_pending_reservations(&self, limit: u32) -> WalletRepoResult<u64> {
        let result = sqlx::query(
            "UPDATE credit_reservations
             SET status='EXPIRED', updated_at=now()
             WHERE id IN (
                 SELECT id FROM credit_reservations
                 WHERE status='PENDING' AND expires_at <= now()
                 ORDER BY expires_at
                 LIMIT $1
                 FOR UPDATE SKIP LOCKED
             )",
        )
        .bind(i64::from(limit.min(10_000)))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn auto_top_up_candidates(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<AutoTopUpCandidate>> {
        #[derive(sqlx::FromRow)]
        struct AutoTopUpRow {
            tenant_id: Uuid,
            balance_units: i64,
            currency: String,
            customer_email: String,
            low: i64,
            critical: i64,
            enabled: bool,
            threshold: i64,
            amount: i64,
            payment_method: Option<String>,
            daily_limit: i64,
            monthly_limit: i64,
            daily_spend_limit: Option<i64>,
            monthly_spend_limit: Option<i64>,
            daily_usage_alert_threshold: Option<i64>,
            monthly_usage_alert_threshold: Option<i64>,
            retry_limit: i32,
            daily: i64,
            monthly: i64,
            attempts: i64,
        }
        let rows = sqlx::query_as::<_, AutoTopUpRow>(
            "SELECT w.tenant_id,w.balance_units,w.currency,u.email::text customer_email,
                    s.low_balance_threshold_units low,
                    s.critical_balance_threshold_units critical,
                    s.auto_top_up_enabled enabled,
                    s.auto_top_up_threshold_units threshold,
                    s.auto_top_up_amount_units amount,
                    s.payment_method,
                    s.daily_top_up_limit_units daily_limit,
                    s.monthly_top_up_limit_units monthly_limit,
                    s.daily_spend_limit_units daily_spend_limit,
                    s.monthly_spend_limit_units monthly_spend_limit,
                    s.daily_usage_alert_threshold_units daily_usage_alert_threshold,
                    s.monthly_usage_alert_threshold_units monthly_usage_alert_threshold,
                    s.retry_limit,
                    COALESCE((SELECT SUM(amount_units) FROM wallet_auto_top_up_attempts a
                              WHERE a.tenant_id=w.tenant_id AND a.status='SUCCEEDED'
                                AND a.attempted_at>=date_trunc('day',now())),0) daily,
                    COALESCE((SELECT SUM(amount_units) FROM wallet_auto_top_up_attempts a
                              WHERE a.tenant_id=w.tenant_id AND a.status='SUCCEEDED'
                                AND a.attempted_at>=date_trunc('month',now())),0) monthly,
                    COALESCE((SELECT COUNT(*) FROM wallet_auto_top_up_attempts a
                              WHERE a.tenant_id=w.tenant_id AND a.attempted_at>=date_trunc('day',now())),0) attempts
             FROM user_wallets w
             JOIN wallet_settings s ON s.tenant_id=w.tenant_id
             JOIN tenants t ON t.id=w.tenant_id
             JOIN users u ON u.id=t.owner_user_id
             WHERE s.auto_top_up_enabled
               AND w.balance_units < s.auto_top_up_threshold_units
             ORDER BY w.balance_units
             LIMIT $1"
        )
        .bind(limit.clamp(1,100) as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| domain::AutoTopUpCandidate {
                tenant_id: r.tenant_id,
                balance_units: r.balance_units,
                currency: r.currency,
                customer_email: r.customer_email,
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
                    daily_spend_limit_units: r.daily_spend_limit,
                    monthly_spend_limit_units: r.monthly_spend_limit,
                    daily_usage_alert_threshold_units: r.daily_usage_alert_threshold,
                    monthly_usage_alert_threshold_units: r.monthly_usage_alert_threshold,
                    retry_limit: r.retry_limit,
                },
                daily_spend_units: r.daily,
                monthly_spend_units: r.monthly,
                daily_attempts: r.attempts as i32,
            })
            .collect())
    }

    async fn adjust_balance(
        &self,
        tenant_id: Uuid,
        delta_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if delta_units == 0 || idempotency_key.trim().is_empty() {
            return Err("adjustment amount and idempotency key are required".into());
        }
        let mut tx = self.pool.begin().await?;
        let wallet=sqlx::query_as::<_,WalletRow>("SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at FROM user_wallets WHERE tenant_id=$1 FOR UPDATE").bind(tenant_id).fetch_one(&mut *tx).await?;
        if let Some(row) = sqlx::query_as::<_, TxRow>(&format!(
            "SELECT {TX_COLUMNS} FROM wallet_transactions WHERE tenant_id=$1 AND idempotency_key=$2"
        ))
        .bind(tenant_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(row.into_domain());
        }
        let reserved:i64=sqlx::query_scalar("SELECT COALESCE(SUM(amount_units),0) FROM credit_reservations WHERE wallet_id=$1 AND status='PENDING' AND expires_at>now()").bind(wallet.id).fetch_one(&mut *tx).await?;
        let new_balance = wallet
            .balance_units
            .checked_add(delta_units)
            .ok_or("wallet balance overflow")?;
        if new_balance < 0 || new_balance < reserved {
            return Err("adjustment would reduce balance below available funds".into());
        }
        let txrow=sqlx::query_as::<_,TxRow>("INSERT INTO wallet_transactions(id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,description) VALUES($1,$2,$3,'ADJUSTMENT',$4,$5,$6,$7) RETURNING id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description,NULL::uuid AS usage_event_id,created_at").bind(Uuid::new_v4()).bind(tenant_id).bind(wallet.id).bind(delta_units).bind(new_balance).bind(idempotency_key).bind(description).fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE user_wallets SET balance_units=$2,billing_status=CASE WHEN $2>0 THEN 'ACTIVE' ELSE billing_status END,updated_at=now() WHERE id=$1").bind(wallet.id).bind(new_balance).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(txrow.into_domain())
    }

    async fn create_top_up_session(&self, session: &WalletTopUpSession) -> WalletRepoResult<()> {
        sqlx::query(
            "INSERT INTO wallet_top_up_sessions
             (id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        )
        .bind(session.id).bind(session.tenant_id).bind(&session.provider)
        .bind(&session.provider_checkout_id).bind(&session.provider_payment_id)
        .bind(session.amount_units).bind(session.refunded_amount_units).bind(&session.currency).bind(&session.status)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow1 {
            id: Uuid,
            tenant_id: Uuid,
            provider: String,
            provider_checkout_id: String,
            provider_payment_id: Option<String>,
            amount_units: i64,
            refunded_amount_units: i64,
            currency: String,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let r = sqlx::query_as::<_,WalletRepoRow1>(
            "SELECT id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status,created_at,updated_at
             FROM wallet_top_up_sessions WHERE tenant_id=$1 AND provider_checkout_id=$2"
        ).bind(tenant_id).bind(provider_checkout_id).fetch_optional(&self.pool).await?;
        Ok(r.map(|x| WalletTopUpSession {
            id: x.id,
            tenant_id: x.tenant_id,
            provider: x.provider,
            provider_checkout_id: x.provider_checkout_id,
            provider_payment_id: x.provider_payment_id,
            amount_units: x.amount_units,
            refunded_amount_units: x.refunded_amount_units,
            currency: x.currency,
            status: x.status,
            created_at: x.created_at,
            updated_at: x.updated_at,
        }))
    }

    async fn find_top_up_session_global(
        &self,
        provider: &str,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        #[derive(sqlx::FromRow)]
        struct WalletRepoRowGlobal {
            id: Uuid,
            tenant_id: Uuid,
            provider: String,
            provider_checkout_id: String,
            provider_payment_id: Option<String>,
            amount_units: i64,
            refunded_amount_units: i64,
            currency: String,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let r = sqlx::query_as::<_,WalletRepoRowGlobal>(
            "SELECT id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status,created_at,updated_at
             FROM wallet_top_up_sessions WHERE provider=$1 AND provider_checkout_id=$2"
        ).bind(provider).bind(provider_checkout_id).fetch_optional(&self.pool).await?;
        Ok(r.map(|x| WalletTopUpSession {
            id: x.id,
            tenant_id: x.tenant_id,
            provider: x.provider,
            provider_checkout_id: x.provider_checkout_id,
            provider_payment_id: x.provider_payment_id,
            amount_units: x.amount_units,
            refunded_amount_units: x.refunded_amount_units,
            currency: x.currency,
            status: x.status,
            created_at: x.created_at,
            updated_at: x.updated_at,
        }))
    }

    async fn mark_top_up_session_failed(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        reason: &str,
    ) -> WalletRepoResult<bool> {
        // Only a PENDING session may transition to FAILED. A session that
        // is already COMPLETED must never be rewritten by a late-arriving
        // failure (Paystack can deliver charge.failed after charge.success
        // in race conditions); a session that is already FAILED should
        // simply report the same idempotent result.
        let result = sqlx::query(
            "UPDATE wallet_top_up_sessions
                SET status='FAILED', updated_at=now()
              WHERE tenant_id=$1
                AND provider_checkout_id=$2
                AND status='PENDING'",
        )
        .bind(tenant_id)
        .bind(provider_checkout_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        tracing::info!(
            tenant_id = %tenant_id,
            provider_checkout_id = %provider_checkout_id,
            reason = %reason,
            "wallet top-up session marked FAILED"
        );
        Ok(true)
    }

    async fn complete_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        provider_payment_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        let mut tx = self.pool.begin().await?;
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow2 {
            id: Uuid,
            tenant_id: Uuid,
            provider: String,
            provider_checkout_id: String,
            provider_payment_id: Option<String>,
            amount_units: i64,
            refunded_amount_units: i64,
            currency: String,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let r=sqlx::query_as::<_,WalletRepoRow2>(
            "SELECT id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status,created_at,updated_at
             FROM wallet_top_up_sessions WHERE tenant_id=$1 AND provider_checkout_id=$2 FOR UPDATE"
        ).bind(tenant_id).bind(provider_checkout_id).fetch_optional(&mut *tx).await?;
        let Some(r) = r else {
            tx.commit().await?;
            return Ok(None);
        };
        if r.status == WalletTopUpSession::COMPLETED {
            tx.commit().await?;
            return Ok(Some(WalletTopUpSession {
                id: r.id,
                tenant_id: r.tenant_id,
                provider: r.provider,
                provider_checkout_id: r.provider_checkout_id,
                provider_payment_id: r.provider_payment_id,
                amount_units: r.amount_units,
                refunded_amount_units: r.refunded_amount_units,
                currency: r.currency,
                status: r.status,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }));
        }
        sqlx::query("UPDATE wallet_top_up_sessions SET status='COMPLETED',provider_payment_id=$3,updated_at=now() WHERE tenant_id=$1 AND provider_checkout_id=$2")
            .bind(tenant_id).bind(provider_checkout_id).bind(provider_payment_id).execute(&mut *tx).await?;
        tx.commit().await?;
        self.find_top_up_session(tenant_id, provider_checkout_id)
            .await
    }

    async fn refundable_top_up(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow3 {
            id: Uuid,
            tenant_id: Uuid,
            provider: String,
            provider_checkout_id: String,
            provider_payment_id: Option<String>,
            amount_units: i64,
            refunded_amount_units: i64,
            currency: String,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let r=sqlx::query_as::<_,WalletRepoRow3>(
            "SELECT id,tenant_id,provider,provider_checkout_id,provider_payment_id,amount_units,refunded_amount_units,currency,status,created_at,updated_at
             FROM wallet_top_up_sessions
             WHERE tenant_id=$1 AND provider_payment_id=$2 AND status='COMPLETED' AND (amount_units-refunded_amount_units)>=$3
             LIMIT 1"
        ).bind(tenant_id).bind(provider_payment_id).bind(amount_units).fetch_optional(&self.pool).await?;
        Ok(r.map(|x| WalletTopUpSession {
            id: x.id,
            tenant_id: x.tenant_id,
            provider: x.provider,
            provider_checkout_id: x.provider_checkout_id,
            provider_payment_id: x.provider_payment_id,
            amount_units: x.amount_units,
            refunded_amount_units: x.refunded_amount_units,
            currency: x.currency,
            status: x.status,
            created_at: x.created_at,
            updated_at: x.updated_at,
        }))
    }

    async fn record_verified_refund(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
        provider_refund_id: &str,
    ) -> WalletRepoResult<bool> {
        if amount_units <= 0 || provider_refund_id.trim().is_empty() {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        let already_credited: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM wallet_transactions
                WHERE tenant_id=$1 AND reference_type='provider_refund' AND reference_id=$2
            )",
        )
        .bind(tenant_id)
        .bind(provider_refund_id)
        .fetch_one(&mut *tx)
        .await?;
        if already_credited {
            tx.commit().await?;
            return Ok(true);
        }
        let session = sqlx::query_as::<_, (Uuid, i64, i64)>(
            "SELECT id,amount_units,refunded_amount_units
             FROM wallet_top_up_sessions
             WHERE tenant_id=$1 AND provider_payment_id=$2 AND status='COMPLETED'
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(provider_payment_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((session_id, original, refunded)) = session else {
            tx.commit().await?;
            return Ok(false);
        };
        if refunded.saturating_add(amount_units) > original {
            return Err("refund exceeds remaining refundable payment amount".into());
        }

        let wallet = sqlx::query_as::<_, WalletRow>(
            "SELECT id,tenant_id,currency,balance_units,payg_activated_at,created_at,updated_at
             FROM user_wallets WHERE tenant_id=$1 FOR UPDATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await?;

        let new_balance = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;

        sqlx::query(
            "UPDATE wallet_top_up_sessions
             SET refunded_amount_units=refunded_amount_units+$3,updated_at=now()
             WHERE id=$1 AND tenant_id=$2",
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(amount_units)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE user_wallets SET balance_units=$2,updated_at=now() WHERE id=$1")
            .bind(wallet.id)
            .bind(new_balance)
            .execute(&mut *tx)
            .await?;

        let idem = format!("refund:{provider_refund_id}");
        let q = format!(
            "INSERT INTO wallet_transactions
             (id,tenant_id,wallet_id,transaction_type,amount_units,balance_after_units,idempotency_key,reference_type,reference_id,description)
             VALUES($1,$2,$3,'REFUND',$4,$5,$6,'provider_refund',$7,$8)
             ON CONFLICT (tenant_id,idempotency_key) DO NOTHING
             RETURNING {TX_COLUMNS}"
        );
        let _ = sqlx::query_as::<_, TxRow>(&q)
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(wallet.id)
            .bind(amount_units)
            .bind(new_balance)
            .bind(&idem)
            .bind(provider_refund_id)
            .bind("Verified payment refund")
            .fetch_optional(&mut *tx)
            .await?;

        sqlx::query("UPDATE wallet_refund_requests SET status='COMPLETED',updated_at=now() WHERE tenant_id=$1 AND provider_refund_id=$2")
            .bind(tenant_id).bind(provider_refund_id).execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(true)
    }

    async fn create_refund_request(
        &self,
        request: &domain::WalletRefundRequest,
    ) -> WalletRepoResult<Option<domain::WalletRefundRequest>> {
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow4 {
            id: Uuid,
            tenant_id: Uuid,
            provider_payment_id: String,
            amount_units: i64,
            idempotency_key: String,
            provider_refund_id: Option<String>,
            status: String,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }
        let row=sqlx::query_as::<_,WalletRepoRow4>(
            "INSERT INTO wallet_refund_requests
             (id,tenant_id,provider_payment_id,amount_units,idempotency_key,status)
             VALUES($1,$2,$3,$4,$5,'PENDING')
             ON CONFLICT (tenant_id,idempotency_key) DO NOTHING
             RETURNING id,tenant_id,provider_payment_id,amount_units,idempotency_key,provider_refund_id,status,created_at,updated_at"
        ).bind(request.id).bind(request.tenant_id).bind(&request.provider_payment_id)
         .bind(request.amount_units).bind(&request.idempotency_key)
         .fetch_optional(&self.pool).await?;
        if let Some(r) = row {
            return Ok(Some(domain::WalletRefundRequest {
                id: r.id,
                tenant_id: r.tenant_id,
                provider_payment_id: r.provider_payment_id,
                amount_units: r.amount_units,
                idempotency_key: r.idempotency_key,
                provider_refund_id: r.provider_refund_id,
                status: r.status,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }));
        }
        let r=sqlx::query_as::<_,WalletRepoRow4>(
            "SELECT id,tenant_id,provider_payment_id,amount_units,idempotency_key,provider_refund_id,status,created_at,updated_at
             FROM wallet_refund_requests WHERE tenant_id=$1 AND idempotency_key=$2"
        ).bind(request.tenant_id).bind(&request.idempotency_key).fetch_optional(&self.pool).await?;
        Ok(r.map(|r| domain::WalletRefundRequest {
            id: r.id,
            tenant_id: r.tenant_id,
            provider_payment_id: r.provider_payment_id,
            amount_units: r.amount_units,
            idempotency_key: r.idempotency_key,
            provider_refund_id: r.provider_refund_id,
            status: r.status,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn set_refund_provider_id(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
        provider_refund_id: &str,
    ) -> WalletRepoResult<()> {
        sqlx::query("UPDATE wallet_refund_requests SET provider_refund_id=$3,status='SUBMITTED',updated_at=now() WHERE tenant_id=$1 AND idempotency_key=$2 AND provider_refund_id IS NULL")
            .bind(tenant_id).bind(idempotency_key).bind(provider_refund_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn claim_auto_top_up(
        &self,
        tenant_id: Uuid,
        lease_id: Uuid,
        lease_for: std::time::Duration,
    ) -> WalletRepoResult<bool> {
        let seconds = lease_for.as_secs().clamp(30, 900) as i64;
        let r = sqlx::query(
            "INSERT INTO wallet_auto_top_up_claims(tenant_id,lease_id,lease_until)
             VALUES($1,$2,now()+($3 * interval '1 second'))
             ON CONFLICT (tenant_id) DO UPDATE
             SET lease_id=EXCLUDED.lease_id, lease_until=EXCLUDED.lease_until
             WHERE wallet_auto_top_up_claims.lease_until < now()
             RETURNING tenant_id",
        )
        .bind(tenant_id)
        .bind(lease_id)
        .bind(seconds)
        .fetch_optional(&self.pool)
        .await?;
        Ok(r.is_some())
    }

    async fn release_auto_top_up(&self, tenant_id: Uuid, lease_id: Uuid) -> WalletRepoResult<()> {
        sqlx::query("DELETE FROM wallet_auto_top_up_claims WHERE tenant_id=$1 AND lease_id=$2")
            .bind(tenant_id)
            .bind(lease_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_auto_top_up_reconciliation_attempts(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<AutoTopUpAttempt>> {
        #[derive(sqlx::FromRow)]
        struct AttemptRow {
            id: Uuid,
            tenant_id: Uuid,
            amount_units: i64,
            status: String,
            provider_payment_id: Option<String>,
            provider_reference: Option<String>,
            error_message: Option<String>,
            attempted_at: DateTime<Utc>,
        }
        let rows = sqlx::query_as::<_, AttemptRow>(
            "SELECT id,tenant_id,amount_units,status,provider_payment_id,provider_reference,error_message,attempted_at
             FROM wallet_auto_top_up_attempts
             WHERE status IN ('PENDING','PROCESSING','REQUIRES_RECONCILIATION')
               AND provider_reference IS NOT NULL
             ORDER BY attempted_at ASC LIMIT $1"
        )
        .bind(limit.clamp(1,100) as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| AutoTopUpAttempt {
                id: r.id,
                tenant_id: r.tenant_id,
                amount_units: r.amount_units,
                status: r.status,
                provider_payment_id: r.provider_payment_id,
                provider_reference: r.provider_reference,
                error_message: r.error_message,
                attempted_at: r.attempted_at,
            })
            .collect())
    }

    async fn record_auto_top_up_attempt(&self, attempt: &AutoTopUpAttempt) -> WalletRepoResult<()> {
        sqlx::query(
            "INSERT INTO wallet_auto_top_up_attempts
             (id,tenant_id,amount_units,status,provider_payment_id,provider_reference,error_message,attempted_at)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT (id) DO UPDATE 
             SET status = EXCLUDED.status,
                 provider_payment_id = COALESCE(EXCLUDED.provider_payment_id, wallet_auto_top_up_attempts.provider_payment_id),
                 provider_reference = COALESCE(EXCLUDED.provider_reference, wallet_auto_top_up_attempts.provider_reference),
                 error_message = EXCLUDED.error_message",
        )
        .bind(attempt.id)
        .bind(attempt.tenant_id)
        .bind(attempt.amount_units)
        .bind(&attempt.status)
        .bind(&attempt.provider_payment_id)
        .bind(&attempt.provider_reference)
        .bind(&attempt.error_message)
        .bind(attempt.attempted_at)
        .execute(&self.pool)
        .await?;
        Ok(())
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
            email_sent_at: Option<DateTime<Utc>>,
            email_processing_at: Option<DateTime<Utc>>,
            created_at: DateTime<Utc>,
        }
        let rows=sqlx::query_as::<_,WalletNotificationRow>("SELECT id,tenant_id,event_type,dedupe_key,message,read_at,email_sent_at,email_processing_at,created_at FROM wallet_notifications WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT $2").bind(tenant_id).bind(limit.clamp(1,100) as i64).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| domain::WalletNotification {
                id: r.id,
                tenant_id: r.tenant_id,
                event_type: r.event_type,
                dedupe_key: r.dedupe_key,
                message: r.message,
                read_at: r.read_at,
                email_sent_at: r.email_sent_at,
                email_processing_at: r.email_processing_at,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn claim_pending_email_notifications(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<domain::WalletNotification>> {
        let mut tx = self.pool.begin().await?;
        #[derive(sqlx::FromRow)]
        struct WalletRepoRow5 {
            id: Uuid,
            tenant_id: Uuid,
            event_type: String,
            dedupe_key: String,
            message: String,
            read_at: Option<DateTime<Utc>>,
            email_sent_at: Option<DateTime<Utc>>,
            email_processing_at: Option<DateTime<Utc>>,
            created_at: DateTime<Utc>,
        }
        let rows=sqlx::query_as::<_,WalletRepoRow5>(
            "WITH picked AS (
                SELECT id FROM wallet_notifications
                WHERE email_sent_at IS NULL
                  AND (email_processing_at IS NULL OR email_processing_at < now() - interval '10 minutes')
                  AND event_type IN ('WARNING','CRITICAL','BLOCKED','TOP_UP_SUCCESS','REFUND_SUCCESS','DAILY_USAGE','MONTHLY_USAGE')
                ORDER BY created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT $1
             )
             UPDATE wallet_notifications n
             SET email_processing_at=now()
             FROM picked
             WHERE n.id=picked.id
             RETURNING n.id,n.tenant_id,n.event_type,n.dedupe_key,n.message,n.read_at,n.email_sent_at,n.email_processing_at,n.created_at"
        ).bind(limit.clamp(1,100) as i64).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|r| domain::WalletNotification {
                id: r.id,
                tenant_id: r.tenant_id,
                event_type: r.event_type,
                dedupe_key: r.dedupe_key,
                message: r.message,
                read_at: r.read_at,
                email_sent_at: r.email_sent_at,
                email_processing_at: r.email_processing_at,
                created_at: r.created_at,
            })
            .collect())
    }

    async fn mark_balance_notification_emailed(&self, id: Uuid) -> WalletRepoResult<bool> {
        let r=sqlx::query("UPDATE wallet_notifications SET email_sent_at=now(),email_processing_at=NULL WHERE id=$1 AND email_sent_at IS NULL")
            .bind(id).execute(&self.pool).await?;
        Ok(r.rows_affected() == 1)
    }
    async fn record_usage_threshold_notifications(&self, limit: u32) -> WalletRepoResult<u64> {
        let cap = i64::from(limit.clamp(1, 100));
        let mut total = 0u64;
        let daily = sqlx::query(
            r#"WITH usage AS (
                   SELECT e.tenant_id, SUM(e.charge_units)::numeric AS usage_units
                   FROM api_usage_events e
                   WHERE e.status='CAPTURED'
                     AND e.created_at >= date_trunc('day', now())
                     AND e.created_at < date_trunc('day', now()) + interval '1 day'
                   GROUP BY e.tenant_id
               ), candidates AS (
                   SELECT w.tenant_id, u.usage_units
                   FROM usage u
                   JOIN wallet_settings s ON s.tenant_id=u.tenant_id
                   JOIN user_wallets w ON w.tenant_id=u.tenant_id
                   WHERE s.daily_usage_alert_threshold_units IS NOT NULL
                     AND s.daily_usage_alert_threshold_units > 0
                     AND u.usage_units >= s.daily_usage_alert_threshold_units
                     AND NOT EXISTS (
                         SELECT 1 FROM wallet_notifications n
                         WHERE n.tenant_id=u.tenant_id AND n.event_type='DAILY_USAGE'
                           AND n.dedupe_key='daily:' || to_char(current_date, 'YYYY-MM-DD')
                     )
                   ORDER BY u.usage_units DESC
                   LIMIT $1
               )
               INSERT INTO wallet_notifications(id,tenant_id,event_type,dedupe_key,message)
               SELECT gen_random_uuid(), tenant_id, 'DAILY_USAGE',
                      'daily:' || to_char(current_date, 'YYYY-MM-DD'),
                      'Daily PAYG usage has reached ' || to_char(ROUND(usage_units / 1000000.0, 2), 'FM999999990.00') || ' USD.'
               FROM candidates
               ON CONFLICT (tenant_id,event_type,dedupe_key) DO NOTHING"#,
        )
        .bind(cap).execute(&self.pool).await?;
        total += daily.rows_affected();

        let monthly = sqlx::query(
            r#"WITH usage AS (
                   SELECT e.tenant_id, SUM(e.charge_units)::numeric AS usage_units
                   FROM api_usage_events e
                   WHERE e.status='CAPTURED'
                     AND e.created_at >= date_trunc('month', now())
                     AND e.created_at < date_trunc('month', now()) + interval '1 month'
                   GROUP BY e.tenant_id
               ), candidates AS (
                   SELECT w.tenant_id, u.usage_units
                   FROM usage u
                   JOIN wallet_settings s ON s.tenant_id=u.tenant_id
                   JOIN user_wallets w ON w.tenant_id=u.tenant_id
                   WHERE s.monthly_usage_alert_threshold_units IS NOT NULL
                     AND s.monthly_usage_alert_threshold_units > 0
                     AND u.usage_units >= s.monthly_usage_alert_threshold_units
                     AND NOT EXISTS (
                         SELECT 1 FROM wallet_notifications n
                         WHERE n.tenant_id=u.tenant_id AND n.event_type='MONTHLY_USAGE'
                           AND n.dedupe_key='monthly:' || to_char(date_trunc('month', now()), 'YYYY-MM')
                     )
                   ORDER BY u.usage_units DESC
                   LIMIT $1
               )
               INSERT INTO wallet_notifications(id,tenant_id,event_type,dedupe_key,message)
               SELECT gen_random_uuid(), tenant_id, 'MONTHLY_USAGE',
                      'monthly:' || to_char(date_trunc('month', now()), 'YYYY-MM'),
                      'Monthly PAYG usage has reached ' || to_char(ROUND(usage_units / 1000000.0, 2), 'FM999999990.00') || ' USD.'
               FROM candidates
               ON CONFLICT (tenant_id,event_type,dedupe_key) DO NOTHING"#,
        )
        .bind(cap).execute(&self.pool).await?;
        total += monthly.rows_affected();
        Ok(total)
    }
}
