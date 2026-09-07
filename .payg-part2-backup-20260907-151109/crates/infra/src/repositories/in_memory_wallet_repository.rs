use application::ports::{ReservationOutcome, WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use domain::{
    AutoTopUpAttempt, CreditReservation, Wallet, WalletNotification, WalletSettings,
    WalletTopUpSession, WalletTransaction, WalletTransactionType,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
struct Promo {
    id: Uuid,
    tenant_id: Uuid,
    remaining: i64,
    expires_at: DateTime<Utc>,
}

type WalletTransactions = Vec<WalletTransaction>;
type WalletReservations = HashMap<Uuid, CreditReservation>;
type WalletStore = HashMap<Uuid, (Wallet, WalletTransactions, WalletReservations)>;
type WalletClaimStore = HashMap<Uuid, (Uuid, DateTime<Utc>)>;

#[derive(Clone, Default)]
pub struct InMemoryWalletRepository {
    inner: Arc<Mutex<WalletStore>>,
    settings: Arc<Mutex<HashMap<Uuid, WalletSettings>>>,
    promos: Arc<Mutex<Vec<Promo>>>,
    sessions: Arc<Mutex<HashMap<String, WalletTopUpSession>>>,
    attempts: Arc<Mutex<Vec<AutoTopUpAttempt>>>,
    notifications: Arc<Mutex<Vec<WalletNotification>>>,

    claims: Arc<Mutex<WalletClaimStore>>,
    refunds: Arc<Mutex<HashMap<String, domain::WalletRefundRequest>>>,
}

#[async_trait]
impl WalletRepository for InMemoryWalletRepository {
    async fn get_or_create(&self, tenant_id: Uuid, currency: &str) -> WalletRepoResult<Wallet> {
        let mut g = self.inner.lock().unwrap();
        let (wallet, _, _) = g.entry(tenant_id).or_insert_with(|| {
            (
                Wallet::new(Uuid::new_v4(), tenant_id, currency),
                Vec::new(),
                HashMap::new(),
            )
        });
        Ok(wallet.clone())
    }

    async fn list_transactions(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletTransaction>> {
        let g = self.inner.lock().unwrap();
        Ok(g.get(&tenant_id)
            .map(|(_, txs, _)| {
                txs.iter()
                    .rev()
                    .take(limit.min(100) as usize)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn reserve(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        operation: &str,
        request_id: &str,
        expires_at: DateTime<Utc>,
    ) -> WalletRepoResult<ReservationOutcome> {
        if amount_units <= 0 || request_id.trim().is_empty() {
            return Err("invalid reservation".into());
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, _, reservations) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        if let Some(existing) = reservations
            .values_mut()
            .find(|r| r.request_id == request_id)
        {
            if existing.status == CreditReservation::PENDING && existing.expires_at <= Utc::now() {
                existing.amount_units = amount_units;
                existing.operation = operation.to_owned();
                existing.expires_at = expires_at;
                existing.status = CreditReservation::PENDING.to_owned();
                existing.updated_at = Utc::now();
                return Ok(ReservationOutcome {
                    reservation: existing.clone(),
                    created: true,
                });
            }
            return Ok(ReservationOutcome {
                reservation: existing.clone(),
                created: false,
            });
        }
        let reserved = reservations
            .values()
            .filter(|r| r.status == CreditReservation::PENDING && r.expires_at > Utc::now())
            .map(|r| r.amount_units)
            .sum::<i64>();
        if wallet.balance_units - reserved < amount_units {
            return Err("insufficient wallet balance".into());
        }
        let r = CreditReservation {
            id: Uuid::new_v4(),
            wallet_id: wallet.id,
            amount_units,
            status: CreditReservation::PENDING.to_owned(),
            operation: operation.to_owned(),
            request_id: request_id.to_owned(),
            expires_at,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        reservations.insert(r.id, r.clone());
        Ok(ReservationOutcome {
            reservation: r,
            created: true,
        })
    }

    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction> {
        let mut g = self.inner.lock().unwrap();
        for (wallet, txs, reservations) in g.values_mut() {
            if let Some(r) = reservations.get_mut(&reservation_id) {
                if r.status == CreditReservation::CAPTURED {
                    return txs
                        .iter()
                        .find(|t| t.idempotency_key == format!("reservation:{reservation_id}"))
                        .cloned()
                        .ok_or_else(|| "captured ledger entry missing".into());
                }
                if r.status != CreditReservation::PENDING || r.expires_at <= Utc::now() {
                    return Err("reservation is not capturable".into());
                }
                if wallet.balance_units < r.amount_units {
                    return Err("insufficient wallet balance".into());
                }

                let amount = r.amount_units;
                let tenant_id = wallet.tenant_id;
                let operation = r.operation.clone();
                wallet.balance_units -= amount;
                wallet.updated_at = Utc::now();
                r.status = CreditReservation::CAPTURED.to_owned();
                r.updated_at = Utc::now();

                drop(g);
                // Promo consumption is synchronized separately. The wallet balance
                // was already atomically reduced under the repository mutex.
                let mut remaining = amount;
                let mut promos = self.promos.lock().unwrap();
                let now = Utc::now();
                promos.retain(|p| p.remaining > 0 && p.expires_at > now);
                for p in promos.iter_mut().filter(|p| p.tenant_id == tenant_id) {
                    if remaining == 0 {
                        break;
                    }
                    let used = p.remaining.min(remaining);
                    p.remaining -= used;
                    remaining -= used;
                }
                drop(promos);
                let mut g = self.inner.lock().unwrap();
                let (wallet, txs, _) = g.get_mut(&tenant_id).unwrap();
                let tx = WalletTransaction {
                    id: Uuid::new_v4(),
                    tenant_id: wallet.tenant_id,
                    wallet_id: wallet.id,
                    transaction_type: WalletTransactionType::Debit,
                    amount_units: -amount,
                    balance_after_units: wallet.balance_units,
                    idempotency_key: format!("reservation:{reservation_id}"),
                    reference_type: Some("credit_reservation".to_owned()),
                    reference_id: Some(reservation_id.to_string()),
                    description: Some(operation),
                    created_at: Utc::now(),
                };
                txs.push(tx.clone());
                return Ok(tx);
            }
        }
        Err("reservation not found".into())
    }

    async fn release(&self, reservation_id: Uuid) -> WalletRepoResult<()> {
        let mut g = self.inner.lock().unwrap();
        for (_, _, reservations) in g.values_mut() {
            if let Some(r) = reservations.get_mut(&reservation_id) {
                if r.status == CreditReservation::PENDING {
                    r.status = CreditReservation::RELEASED.to_owned();
                    r.updated_at = Utc::now();
                }
                return Ok(());
            }
        }
        Err("reservation not found".into())
    }

    async fn credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        transaction_type: WalletTransactionType,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if amount_units <= 0 || idempotency_key.trim().is_empty() {
            return Err("invalid credit".into());
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, txs, _) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        if let Some(existing) = txs.iter().find(|t| t.idempotency_key == idempotency_key) {
            return Ok(existing.clone());
        }
        wallet.balance_units = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;
        wallet.updated_at = Utc::now();
        let tx = WalletTransaction {
            id: Uuid::new_v4(),
            tenant_id,
            wallet_id: wallet.id,
            transaction_type,
            amount_units,
            balance_after_units: wallet.balance_units,
            idempotency_key: idempotency_key.to_owned(),
            reference_type: None,
            reference_id: None,
            description: description.map(str::to_owned),
            created_at: Utc::now(),
        };
        txs.push(tx.clone());
        Ok(tx)
    }

    async fn create_top_up_session(&self, session: &WalletTopUpSession) -> WalletRepoResult<()> {
        let mut g = self.sessions.lock().unwrap();
        if g.values().any(|x| {
            x.provider == session.provider
                && x.provider_checkout_id == session.provider_checkout_id
                && x.tenant_id != session.tenant_id
        }) {
            return Err("provider checkout already belongs to another tenant".into());
        }
        g.insert(session.provider_checkout_id.clone(), session.clone());
        Ok(())
    }

    async fn find_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .get(provider_checkout_id)
            .filter(|x| x.tenant_id == tenant_id)
            .cloned())
    }

    async fn complete_top_up_session(
        &self,
        tenant_id: Uuid,
        provider_checkout_id: &str,
        provider_payment_id: &str,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        let mut g = self.sessions.lock().unwrap();
        let Some(s) = g.get_mut(provider_checkout_id) else {
            return Ok(None);
        };
        if s.tenant_id != tenant_id {
            return Ok(None);
        }
        if s.status == WalletTopUpSession::COMPLETED {
            return Ok(Some(s.clone()));
        }
        s.status = WalletTopUpSession::COMPLETED.to_owned();
        s.provider_payment_id = Some(provider_payment_id.to_owned());
        s.updated_at = Utc::now();
        Ok(Some(s.clone()))
    }

    async fn refundable_top_up(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
    ) -> WalletRepoResult<Option<WalletTopUpSession>> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .values()
            .find(|s| {
                s.tenant_id == tenant_id
                    && s.provider_payment_id.as_deref() == Some(provider_payment_id)
                    && s.status == WalletTopUpSession::COMPLETED
                    && s.amount_units.saturating_sub(s.refunded_amount_units) >= amount_units
            })
            .cloned())
    }

    async fn record_verified_refund(
        &self,
        tenant_id: Uuid,
        provider_payment_id: &str,
        amount_units: i64,
        provider_refund_id: &str,
    ) -> WalletRepoResult<bool> {
        {
            let g = self.inner.lock().unwrap();
            if g.get(&tenant_id)
                .map(|(_, txs, _)| {
                    txs.iter().any(|t| {
                        t.reference_type.as_deref() == Some("provider_refund")
                            && t.reference_id.as_deref() == Some(provider_refund_id)
                    })
                })
                .unwrap_or(false)
            {
                return Ok(true);
            }
        }

        // Keep the sessions mutex strictly inside this scope.
        // Nothing borrowed from the guard is carried into an await.
        let valid = {
            let mut sessions = self.sessions.lock().unwrap();

            let Some(session) = sessions.values_mut().find(|s| {
                s.tenant_id == tenant_id
                    && s.provider_payment_id.as_deref() == Some(provider_payment_id)
                    && s.status == WalletTopUpSession::COMPLETED
            }) else {
                return Ok(false);
            };

            if amount_units <= 0
                || session.refunded_amount_units.saturating_add(amount_units) > session.amount_units
            {
                return Ok(false);
            }

            session.refunded_amount_units += amount_units;
            session.updated_at = Utc::now();

            true
        };

        if !valid {
            return Ok(false);
        }

        let _ = self
            .credit(
                tenant_id,
                amount_units,
                WalletTransactionType::Refund,
                &format!("refund:{provider_refund_id}"),
                Some("Verified payment refund"),
            )
            .await?;

        // Update the refund request only after the wallet credit has
        // completed. This mutex is also fully scoped before returning.
        {
            let mut refunds = self.refunds.lock().unwrap();

            if let Some(r) = refunds.values_mut().find(|r| {
                r.tenant_id == tenant_id
                    && r.provider_refund_id.as_deref() == Some(provider_refund_id)
            }) {
                r.status = "COMPLETED".to_owned();
                r.updated_at = Utc::now();
            }
        }

        Ok(true)
    }

    async fn create_refund_request(
        &self,
        request: &domain::WalletRefundRequest,
    ) -> WalletRepoResult<Option<domain::WalletRefundRequest>> {
        let mut g = self.refunds.lock().unwrap();
        if let Some(existing) = g.get(&request.idempotency_key) {
            return Ok(Some(existing.clone()));
        }
        g.insert(request.idempotency_key.clone(), request.clone());
        Ok(Some(request.clone()))
    }

    async fn set_refund_provider_id(
        &self,
        tenant_id: Uuid,
        idempotency_key: &str,
        provider_refund_id: &str,
    ) -> WalletRepoResult<()> {
        let mut g = self.refunds.lock().unwrap();
        if let Some(r) = g.get_mut(idempotency_key)
            && r.tenant_id == tenant_id
            && r.provider_refund_id.is_none()
        {
            r.provider_refund_id = Some(provider_refund_id.to_owned());
            r.status = "SUBMITTED".to_owned();
            r.updated_at = Utc::now();
        }
        Ok(())
    }

    async fn claim_auto_top_up(
        &self,
        tenant_id: Uuid,
        lease_id: Uuid,
        lease_for: std::time::Duration,
    ) -> WalletRepoResult<bool> {
        let mut g = self.claims.lock().unwrap();
        let now = Utc::now();
        if let Some((_, until)) = g.get(&tenant_id)
            && *until > now
        {
            return Ok(false);
        }
        g.insert(
            tenant_id,
            (
                lease_id,
                now + chrono::Duration::from_std(lease_for)
                    .unwrap_or(chrono::Duration::seconds(60)),
            ),
        );
        Ok(true)
    }

    async fn release_auto_top_up(&self, tenant_id: Uuid, lease_id: Uuid) -> WalletRepoResult<()> {
        let mut g = self.claims.lock().unwrap();
        if g.get(&tenant_id).map(|x| x.0) == Some(lease_id) {
            g.remove(&tenant_id);
        }
        Ok(())
    }

    async fn record_auto_top_up_attempt(&self, attempt: &AutoTopUpAttempt) -> WalletRepoResult<()> {
        let mut g = self.attempts.lock().unwrap();
        if !g.iter().any(|x| x.id == attempt.id) {
            g.push(attempt.clone());
        }
        Ok(())
    }

    async fn get_settings(&self, tenant_id: Uuid) -> WalletRepoResult<WalletSettings> {
        let mut g = self.settings.lock().unwrap();
        Ok(g.entry(tenant_id)
            .or_insert_with(|| WalletSettings {
                tenant_id,
                ..WalletSettings::default()
            })
            .clone())
    }

    async fn update_settings(&self, settings: &WalletSettings) -> WalletRepoResult<WalletSettings> {
        if settings.critical_balance_threshold_units > settings.low_balance_threshold_units
            || settings.auto_top_up_amount_units <= 0
            || settings.daily_top_up_limit_units <= 0
            || settings.monthly_top_up_limit_units <= 0
            || !(0..=10).contains(&settings.retry_limit)
        {
            return Err("invalid wallet settings".into());
        }
        self.settings
            .lock()
            .unwrap()
            .insert(settings.tenant_id, settings.clone());
        Ok(settings.clone())
    }

    async fn expire_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<Option<WalletTransaction>> {
        if amount_units <= 0 {
            return Ok(None);
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, txs, _) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        if let Some(x) = txs.iter().find(|t| t.idempotency_key == idempotency_key) {
            return Ok(Some(x.clone()));
        }
        let debit = amount_units.min(wallet.balance_units);
        if debit == 0 {
            return Ok(None);
        }
        wallet.balance_units -= debit;
        wallet.updated_at = Utc::now();
        let tx = WalletTransaction {
            id: Uuid::new_v4(),
            tenant_id,
            wallet_id: wallet.id,
            transaction_type: WalletTransactionType::Expiration,
            amount_units: -debit,
            balance_after_units: wallet.balance_units,
            idempotency_key: idempotency_key.to_owned(),
            reference_type: Some("promotional_credit".to_owned()),
            reference_id: None,
            description: description.map(str::to_owned),
            created_at: Utc::now(),
        };
        txs.push(tx.clone());
        Ok(Some(tx))
    }

    async fn grant_promotional_credit(
        &self,
        tenant_id: Uuid,
        amount_units: i64,
        expires_at: DateTime<Utc>,
        idempotency_key: &str,
        description: Option<&str>,
    ) -> WalletRepoResult<WalletTransaction> {
        if expires_at <= Utc::now() {
            return Err("promotional credit already expired".into());
        }
        let tx = self
            .credit(
                tenant_id,
                amount_units,
                WalletTransactionType::PromotionalCredit,
                idempotency_key,
                description,
            )
            .await?;
        let mut promos = self.promos.lock().unwrap();
        if !promos
            .iter()
            .any(|p| p.tenant_id == tenant_id && p.id == tx.id)
        {
            promos.push(Promo {
                id: tx.id,
                tenant_id,
                remaining: amount_units,
                expires_at,
            });
        }
        Ok(tx)
    }

    async fn expire_promotional_credits(&self, limit: u32) -> WalletRepoResult<u64> {
        let now = Utc::now();

        // Extract owned expiration work first. The MutexGuard does not
        // survive this block, so the await below is Send-safe.
        let ids: Vec<(Uuid, Uuid, i64)> = {
            let promos = self.promos.lock().unwrap();

            promos
                .iter()
                .filter(|p| p.expires_at <= now && p.remaining > 0)
                .take(limit.min(500) as usize)
                .map(|p| (p.id, p.tenant_id, p.remaining))
                .collect()
        };

        let mut n = 0u64;

        for (id, tenant, remaining) in ids {
            self.expire_credit(
                tenant,
                remaining,
                &format!("expiration:{id}"),
                Some("Promotional credit expired"),
            )
            .await?;

            {
                let mut promos = self.promos.lock().unwrap();

                if let Some(p) = promos.iter_mut().find(|p| p.id == id) {
                    p.remaining = 0;
                }
            }

            n += 1;
        }

        Ok(n)
    }

    async fn auto_top_up_candidates(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<domain::AutoTopUpCandidate>> {
        let settings = self.settings.lock().unwrap().clone();
        let attempts = self.attempts.lock().unwrap().clone();
        let wallets = self
            .inner
            .lock()
            .unwrap()
            .values()
            .map(|(w, _, _)| w.clone())
            .collect::<Vec<_>>();
        let today = Utc::now().date_naive();
        let month = Utc::now().month();
        let mut out = Vec::new();
        for w in wallets {
            let Some(s) = settings.get(&w.tenant_id) else {
                continue;
            };
            if !s.auto_top_up_enabled || w.balance_units >= s.auto_top_up_threshold_units {
                continue;
            }
            let daily = attempts
                .iter()
                .filter(|a| {
                    a.tenant_id == w.tenant_id
                        && a.attempted_at.date_naive() == today
                        && a.status == "SUCCEEDED"
                })
                .map(|a| a.amount_units)
                .sum();
            let monthly = attempts
                .iter()
                .filter(|a| {
                    a.tenant_id == w.tenant_id
                        && a.attempted_at.month() == month
                        && a.attempted_at.year() == Utc::now().year()
                        && a.status == "SUCCEEDED"
                })
                .map(|a| a.amount_units)
                .sum();
            let daily_attempts = attempts
                .iter()
                .filter(|a| a.tenant_id == w.tenant_id && a.attempted_at.date_naive() == today)
                .count() as i32;
            out.push(domain::AutoTopUpCandidate {
                tenant_id: w.tenant_id,
                balance_units: w.balance_units,
                settings: s.clone(),
                daily_spend_units: daily,
                monthly_spend_units: monthly,
                daily_attempts,
            });
            if out.len() >= limit as usize {
                break;
            }
        }
        Ok(out)
    }

    async fn record_balance_notification(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        dedupe_key: &str,
        message: &str,
    ) -> WalletRepoResult<bool> {
        let mut g = self.notifications.lock().unwrap();
        if g.iter().any(|n| {
            n.tenant_id == tenant_id && n.event_type == event_type && n.dedupe_key == dedupe_key
        }) {
            return Ok(false);
        }
        g.push(WalletNotification {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: event_type.to_owned(),
            dedupe_key: dedupe_key.to_owned(),
            message: message.to_owned(),
            read_at: None,
            email_sent_at: None,
            email_processing_at: None,
            created_at: Utc::now(),
        });
        Ok(true)
    }

    async fn list_balance_notifications(
        &self,
        tenant_id: Uuid,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>> {
        Ok(self
            .notifications
            .lock()
            .unwrap()
            .iter()
            .rev()
            .filter(|n| n.tenant_id == tenant_id)
            .take(limit.min(100) as usize)
            .cloned()
            .collect())
    }

    async fn claim_pending_email_notifications(
        &self,
        limit: u32,
    ) -> WalletRepoResult<Vec<WalletNotification>> {
        let mut g = self.notifications.lock().unwrap();
        let now = Utc::now();
        let mut out = Vec::new();
        for n in g
            .iter_mut()
            .filter(|n| n.email_sent_at.is_none() && n.email_processing_at.is_none())
            .take(limit.min(100) as usize)
        {
            n.email_processing_at = Some(now);
            out.push(n.clone());
        }
        Ok(out)
    }

    async fn mark_balance_notification_emailed(&self, id: Uuid) -> WalletRepoResult<bool> {
        let mut g = self.notifications.lock().unwrap();
        if let Some(n) = g
            .iter_mut()
            .find(|n| n.id == id && n.email_sent_at.is_none())
        {
            n.email_sent_at = Some(Utc::now());
            n.email_processing_at = None;
            return Ok(true);
        }
        Ok(false)
    }
}
