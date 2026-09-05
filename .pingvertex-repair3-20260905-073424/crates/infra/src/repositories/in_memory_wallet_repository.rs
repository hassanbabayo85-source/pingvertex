use application::ports::{WalletRepoResult, WalletRepository};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{CreditReservation, Wallet, WalletTransaction, WalletTransactionType};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct InMemoryWalletRepository {
    inner: Arc<
        Mutex<
            HashMap<
                Uuid,
                (
                    Wallet,
                    Vec<WalletTransaction>,
                    HashMap<Uuid, CreditReservation>,
                ),
            >,
        >,
    >,
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
    ) -> WalletRepoResult<CreditReservation> {
        if amount_units <= 0 {
            return Err("reservation amount must be positive".into());
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, _, reservations) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        let reserved_units: i64 = reservations
            .values()
            .filter(|r| r.status == CreditReservation::PENDING && r.expires_at > Utc::now())
            .map(|r| r.amount_units)
            .sum();
        let available_units = wallet.balance_units.saturating_sub(reserved_units);
        if available_units < amount_units {
            return Err("insufficient wallet balance".into());
        }
        if let Some(existing) = reservations
            .values()
            .find(|r| r.request_id == request_id && r.status == CreditReservation::PENDING)
        {
            return Ok(existing.clone());
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
        Ok(r)
    }

    async fn capture(&self, reservation_id: Uuid) -> WalletRepoResult<WalletTransaction> {
        let mut g = self.inner.lock().unwrap();
        for (wallet, txs, reservations) in g.values_mut() {
            if let Some(r) = reservations.get_mut(&reservation_id) {
                if r.status == CreditReservation::CAPTURED {
                    return txs
                        .iter()
                        .find(|t| t.reference_id.as_deref() == Some(&reservation_id.to_string()))
                        .cloned()
                        .ok_or_else(|| "captured reservation ledger entry missing".into());
                }
                if r.status != CreditReservation::PENDING {
                    return Err("reservation is not capturable".into());
                }
                if wallet.balance_units < r.amount_units {
                    return Err("insufficient wallet balance".into());
                }
                wallet.balance_units -= r.amount_units;
                wallet.updated_at = Utc::now();
                r.status = CreditReservation::CAPTURED.to_owned();
                r.updated_at = Utc::now();
                let tx = WalletTransaction {
                    id: Uuid::new_v4(),
                    wallet_id: wallet.id,
                    transaction_type: WalletTransactionType::Debit,
                    amount_units: -r.amount_units,
                    balance_after_units: wallet.balance_units,
                    idempotency_key: Some(format!("reservation:{}", reservation_id)),
                    reference_type: Some("credit_reservation".to_owned()),
                    reference_id: Some(reservation_id.to_string()),
                    description: Some(r.operation.clone()),
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
        if amount_units <= 0 {
            return Err("credit amount must be positive".into());
        }
        let mut g = self.inner.lock().unwrap();
        let (wallet, txs, _) = g.get_mut(&tenant_id).ok_or("wallet not found")?;
        if let Some(existing) = txs
            .iter()
            .find(|t| t.idempotency_key.as_deref() == Some(idempotency_key))
        {
            return Ok(existing.clone());
        }
        wallet.balance_units = wallet
            .balance_units
            .checked_add(amount_units)
            .ok_or("wallet balance overflow")?;
        wallet.updated_at = Utc::now();
        let tx = WalletTransaction {
            id: Uuid::new_v4(),
            wallet_id: wallet.id,
            transaction_type,
            amount_units,
            balance_after_units: wallet.balance_units,
            idempotency_key: Some(idempotency_key.to_owned()),
            reference_type: None,
            reference_id: None,
            description: description.map(str::to_owned),
            created_at: Utc::now(),
        };
        txs.push(tx.clone());
        Ok(tx)
    }
}
