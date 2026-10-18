//! In-memory `RecurringProxyMonitorRepository` for tests.
//!
//! Stores monitors as decomposed fields (not as the entity) because
//! `RecurringProxyMonitor` is not `Clone` — `ProxyCredentials` refuses
//! to derive `Clone` by design. Every read reconstructs the entity, so
//! `ProxyCredentials` is rebuilt from its plaintext components in this
//! test-only implementation. Production code uses
//! `PostgresRecurringProxyMonitorRepository`, which stores them
//! encrypted.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use application::ports::monitor_repository::QuotaCheckOutcome;
use application::ports::{
    ClaimedRecurringProxyMonitor, RecurringProxyMonitorRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{RecurringProxyCheck, RecurringProxyMonitor};
use domain::enums::ProxyScheme;
use domain::shared::ids::{TenantId, UserId};
use domain::value_objects::{ProxyCredentials, ProxyHost};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ── Stored form ─────────────────────────────────────────────────────

#[derive(Clone)]
struct StoredMonitor {
    id: Uuid,
    tenant_id: TenantId,
    user_id: UserId,
    name: String,
    host: ProxyHost,
    port: u16,
    scheme: ProxyScheme,
    credentials: Option<(String, String)>,
    interval_seconds: u32,
    user_enabled: bool,
    entitlement_suspended: bool,
    next_run_at: Option<DateTime<Utc>>,
    last_run_at: Option<DateTime<Utc>>,
    worker_id: Option<String>,
    lease_expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl StoredMonitor {
    fn from_entity(m: &RecurringProxyMonitor) -> Self {
        let credentials = m
            .credentials
            .as_ref()
            .map(|c| (c.username().to_owned(), c.password().to_owned()));
        Self {
            id: m.id,
            tenant_id: m.tenant_id,
            user_id: m.user_id,
            name: m.name.clone(),
            host: m.host.clone(),
            port: m.port,
            scheme: m.scheme.clone(),
            credentials,
            interval_seconds: m.interval_seconds,
            user_enabled: m.user_enabled,
            entitlement_suspended: m.entitlement_suspended,
            next_run_at: m.next_run_at,
            last_run_at: m.last_run_at,
            // Worker-claim metadata is not part of the domain entity;
            // it lives only in the repository. Preserve it across
            // `from_entity` calls so an update doesn't silently
            // release an active lease.
            worker_id: None,
            lease_expires_at: None,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }

    fn to_entity(&self) -> RepoResult<RecurringProxyMonitor> {
        let credentials = match &self.credentials {
            Some((u, p)) => {
                Some(ProxyCredentials::new(u.clone(), p.clone()).map_err(|e| {
                    format!("stored proxy credentials failed validation: {e}")
                })?)
            }
            None => None,
        };
        Ok(RecurringProxyMonitor {
            id: self.id,
            tenant_id: self.tenant_id,
            user_id: self.user_id,
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port,
            scheme: self.scheme.clone(),
            credentials,
            interval_seconds: self.interval_seconds,
            user_enabled: self.user_enabled,
            entitlement_suspended: self.entitlement_suspended,
            next_run_at: self.next_run_at,
            last_run_at: self.last_run_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }

    /// Applies the configurable fields of `m` while **preserving** the
    /// lease metadata (`worker_id`, `lease_expires_at`) that lives only
    /// in the repository.
    fn apply_entity(&mut self, m: &RecurringProxyMonitor) {
        self.name = m.name.clone();
        self.host = m.host.clone();
        self.port = m.port;
        self.scheme = m.scheme.clone();
        self.credentials = m
            .credentials
            .as_ref()
            .map(|c| (c.username().to_owned(), c.password().to_owned()));
        self.interval_seconds = m.interval_seconds;
        self.user_enabled = m.user_enabled;
        self.entitlement_suspended = m.entitlement_suspended;
        self.next_run_at = m.next_run_at;
        self.last_run_at = m.last_run_at;
        self.updated_at = m.updated_at;
    }
}

// ── Repository ──────────────────────────────────────────────────────

pub struct InMemoryRecurringProxyMonitorRepository {
    monitors: Arc<RwLock<HashMap<Uuid, StoredMonitor>>>,
    checks: Arc<RwLock<HashMap<Uuid, RecurringProxyCheck>>>,
    quota_lock: Arc<tokio::sync::Mutex<()>>,
    claim_lock: Arc<tokio::sync::Mutex<()>>,
}

impl InMemoryRecurringProxyMonitorRepository {
    pub fn new() -> Self {
        Self {
            monitors: Arc::new(RwLock::new(HashMap::new())),
            checks: Arc::new(RwLock::new(HashMap::new())),
            quota_lock: Arc::new(tokio::sync::Mutex::new(())),
            claim_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}

impl Default for InMemoryRecurringProxyMonitorRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RecurringProxyMonitorRepository for InMemoryRecurringProxyMonitorRepository {
    async fn save_monitor(&self, monitor: &RecurringProxyMonitor) -> RepoResult<()> {
        let mut monitors = self.monitors.write().unwrap();
        match monitors.get_mut(&monitor.id) {
            Some(existing) => existing.apply_entity(monitor),
            None => {
                monitors.insert(monitor.id, StoredMonitor::from_entity(monitor));
            }
        }
        Ok(())
    }

    async fn save_monitor_if_within_quota(
        &self,
        monitor: &RecurringProxyMonitor,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        let _guard = self.quota_lock.lock().await;
        let current = self
            .monitors
            .read()
            .unwrap()
            .values()
            .filter(|m| m.tenant_id == tenant_id)
            .count() as u32;
        if current >= limit {
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }
        self.save_monitor(monitor).await?;
        Ok(QuotaCheckOutcome::Saved)
    }

    async fn find_by_id(
        &self,
        id: Uuid,
        tenant_id: TenantId,
    ) -> RepoResult<Option<RecurringProxyMonitor>> {
        let monitors = self.monitors.read().unwrap();
        match monitors.get(&id) {
            Some(m) if m.tenant_id == tenant_id => Ok(Some(m.to_entity()?)),
            _ => Ok(None),
        }
    }

    async fn list_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepoResult<Vec<RecurringProxyMonitor>> {
        let monitors = self.monitors.read().unwrap();
        let mut out = Vec::new();
        for m in monitors.values() {
            if m.tenant_id == tenant_id {
                out.push(m.to_entity()?);
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    async fn update_monitor(&self, monitor: &RecurringProxyMonitor) -> RepoResult<()> {
        let mut monitors = self.monitors.write().unwrap();
        let Some(existing) = monitors.get_mut(&monitor.id) else {
            return Err("monitor not found".into());
        };
        if existing.tenant_id != monitor.tenant_id {
            return Err("tenant mismatch".into());
        }
        existing.apply_entity(monitor);
        Ok(())
    }

    async fn delete_for_tenant(&self, id: Uuid, tenant_id: TenantId) -> RepoResult<bool> {
        let mut monitors = self.monitors.write().unwrap();
        match monitors.get(&id) {
            Some(m) if m.tenant_id == tenant_id => {
                monitors.remove(&id);
                // Cascade: remove all checks for this monitor.
                let mut checks = self.checks.write().unwrap();
                checks.retain(|_, c| c.monitor_id != id);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn count_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        Ok(self
            .monitors
            .read()
            .unwrap()
            .values()
            .filter(|m| m.tenant_id == tenant_id)
            .count() as u64)
    }

    async fn claim_due_monitors(
        &self,
        now: DateTime<Utc>,
        worker_id: &str,
        lease_until: DateTime<Utc>,
        limit: u32,
    ) -> RepoResult<Vec<ClaimedRecurringProxyMonitor>> {
        let _guard = self.claim_lock.lock().await;
        let mut monitors = self.monitors.write().unwrap();

        // Find due monitor ids, sorted by next_run_at (earliest first).
        let mut due: Vec<Uuid> = monitors
            .iter()
            .filter(|(_, m)| {
                m.user_enabled
                    && !m.entitlement_suspended
                    && m.next_run_at.is_some_and(|t| t <= now)
                    && m.lease_expires_at.map_or(true, |t| t < now)
            })
            .map(|(id, _)| *id)
            .collect();
        due.sort_by_key(|id| monitors[id].next_run_at);

        let mut claimed = Vec::new();
        for id in due.into_iter().take(limit as usize) {
            let m = monitors.get_mut(&id).unwrap();
            m.worker_id = Some(worker_id.to_owned());
            m.lease_expires_at = Some(lease_until);
            claimed.push(ClaimedRecurringProxyMonitor {
                monitor: m.to_entity()?,
                worker_id: worker_id.to_owned(),
                lease_expires_at: lease_until,
                claimed_at: now,
            });
        }
        Ok(claimed)
    }

    async fn reschedule(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        worker_id: &str,
        last_run_at: DateTime<Utc>,
        next_run_at: DateTime<Utc>,
    ) -> RepoResult<bool> {
        let mut monitors = self.monitors.write().unwrap();
        let Some(m) = monitors.get_mut(&id) else {
            return Ok(false);
        };
        if m.tenant_id != tenant_id || m.worker_id.as_deref() != Some(worker_id) {
            return Ok(false);
        }
        m.worker_id = None;
        m.lease_expires_at = None;
        m.last_run_at = Some(last_run_at);
        m.next_run_at = Some(next_run_at);
        m.updated_at = Utc::now();
        Ok(true)
    }

    async fn release_lease(
        &self,
        id: Uuid,
        tenant_id: TenantId,
        worker_id: &str,
    ) -> RepoResult<bool> {
        let mut monitors = self.monitors.write().unwrap();
        let Some(m) = monitors.get_mut(&id) else {
            return Ok(false);
        };
        if m.tenant_id != tenant_id || m.worker_id.as_deref() != Some(worker_id) {
            return Ok(false);
        }
        m.worker_id = None;
        m.lease_expires_at = None;
        m.updated_at = Utc::now();
        Ok(true)
    }

    async fn save_check(&self, check: &RecurringProxyCheck) -> RepoResult<()> {
        self.checks.write().unwrap().insert(check.id, check.clone());
        Ok(())
    }

    async fn finish_check(&self, check: &RecurringProxyCheck) -> RepoResult<bool> {
        let mut checks = self.checks.write().unwrap();
        if let Some(existing) = checks.get_mut(&check.id) {
            // Only the terminal fields are allowed to change; identity
            // and `started_at` are immutable.
            existing.status = check.status;
            existing.finished_at = check.finished_at;
            existing.result = check.result.clone();
            existing.error_message = check.error_message.clone();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_checks(
        &self,
        monitor_id: Uuid,
        tenant_id: TenantId,
        offset: u32,
        limit: u32,
    ) -> RepoResult<(Vec<RecurringProxyCheck>, u64)> {
        let checks = self.checks.read().unwrap();
        let mut all: Vec<RecurringProxyCheck> = checks
            .values()
            .filter(|c| c.monitor_id == monitor_id && c.tenant_id == tenant_id)
            .cloned()
            .collect();
        all.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        let total = all.len() as u64;
        let page = all
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        Ok((page, total))
    }

    async fn find_latest_check(
        &self,
        monitor_id: Uuid,
        tenant_id: TenantId,
    ) -> RepoResult<Option<RecurringProxyCheck>> {
        let checks = self.checks.read().unwrap();
        Ok(checks
            .values()
            .filter(|c| c.monitor_id == monitor_id && c.tenant_id == tenant_id)
            .max_by_key(|c| c.started_at)
            .cloned())
    }
}
