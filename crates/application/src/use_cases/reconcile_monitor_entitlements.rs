//! TASK 5 — Reconcile a tenant's monitors against its current
//! subscription entitlements.
//!
//! # Policy (deterministic, documented in migration 0128)
//!
//! For a tenant with `N` monitors and an entitlement limit `L`:
//!
//!   * `N <= L` → all monitors are eligible. Any monitor currently
//!     `entitlement_suspended = true` is **un-suspended**.
//!   * `N >  L` → the **oldest** `L` monitors remain (or become)
//!     eligible; the **newest** `N - L` are suspended.
//!
//! The word "oldest" is measured by `created_at ASC`, with `id` as a
//! stable tiebreaker so the choice is reproducible across runs (and
//! across concurrent reconcilers).
//!
//! # What this NEVER touches
//!
//! `enabled` (the customer's own enable/disable choice) is left exactly
//! as it was. If a customer had manually disabled a monitor before the
//! reconciliation ran, that choice survives untouched. Only
//! `entitlement_suspended` is mutated.
//!
//! # Idempotency
//!
//! Running `reconcile_tenant` twice in a row on the same data has no
//! further effect on the second run (`newly_suspended = 0`,
//! `newly_unsuspended = 0`). This is what allows the sweep worker to
//! run periodically without churning `updated_at`.

use std::{error::Error, sync::Arc};

use crate::ports::{EntitlementResolver, MonitorRepository};
use domain::{Monitor, shared::ids::TenantId};

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantReconciliationResult {
    pub tenant_id: TenantId,
    pub limit: u32,
    pub total_monitors: u64,
    pub newly_suspended: u64,
    pub newly_unsuspended: u64,
}

pub struct ReconcileMonitorEntitlementsUseCase<R> {
    entitlement_resolver: Arc<dyn EntitlementResolver>,
    monitor_repository: R,
}

impl<R> ReconcileMonitorEntitlementsUseCase<R>
where
    R: MonitorRepository,
{
    pub fn new(entitlement_resolver: Arc<dyn EntitlementResolver>, monitor_repository: R) -> Self {
        Self {
            entitlement_resolver,
            monitor_repository,
        }
    }

    pub async fn reconcile_tenant(
        &self,
        tenant_id: TenantId,
    ) -> UseCaseResult<TenantReconciliationResult> {
        let entitlements = self.entitlement_resolver.resolve(tenant_id).await?;
        let limit = entitlements.max_uptime_monitors;

        let monitors = self.monitor_repository.find_all_for_tenant(tenant_id).await?;
        let total_monitors = monitors.len() as u64;

        let plan = plan_suspension(&monitors, limit);

        let newly_suspended = if plan.to_suspend.is_empty() {
            0
        } else {
            self.monitor_repository
                .set_entitlement_suspended_for_tenant(tenant_id, &plan.to_suspend, true)
                .await?
        };

        let newly_unsuspended = if plan.to_unsuspend.is_empty() {
            0
        } else {
            self.monitor_repository
                .set_entitlement_suspended_for_tenant(tenant_id, &plan.to_unsuspend, false)
                .await?
        };

        Ok(TenantReconciliationResult {
            tenant_id,
            limit,
            total_monitors,
            newly_suspended,
            newly_unsuspended,
        })
    }

    /// Reconciles every tenant in `tenant_ids` and returns per-tenant
    /// results. A failure on one tenant does NOT abort the batch — it
    /// appears as an entry with `error: Some(...)` and the sweep moves
    /// on. This is the shape the periodic sweep worker consumes.
    pub async fn reconcile_batch(
        &self,
        tenant_ids: &[TenantId],
    ) -> Vec<BatchReconciliationEntry> {
        let mut out = Vec::with_capacity(tenant_ids.len());
        for &tenant_id in tenant_ids {
            match self.reconcile_tenant(tenant_id).await {
                Ok(result) => out.push(BatchReconciliationEntry {
                    tenant_id,
                    outcome: Ok(result),
                }),
                Err(err) => out.push(BatchReconciliationEntry {
                    tenant_id,
                    outcome: Err(err.to_string()),
                }),
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct BatchReconciliationEntry {
    pub tenant_id: TenantId,
    pub outcome: Result<TenantReconciliationResult, String>,
}

// ─── Pure policy (unit-testable without I/O) ───────────────────────

#[derive(Debug, Default, PartialEq, Eq)]
struct SuspensionPlan {
    to_suspend: Vec<domain::MonitorId>,
    to_unsuspend: Vec<domain::MonitorId>,
}

fn plan_suspension(monitors: &[Monitor], limit: u32) -> SuspensionPlan {
    let mut sorted: Vec<&Monitor> = monitors.iter().collect();
    sorted.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.0.cmp(&b.id.0))
    });

    let limit_usize = limit as usize;
    let mut plan = SuspensionPlan::default();

    for (index, monitor) in sorted.into_iter().enumerate() {
        let should_be_suspended = index >= limit_usize;
        match (should_be_suspended, monitor.entitlement_suspended) {
            (true, false) => plan.to_suspend.push(monitor.id),
            (false, true) => plan.to_unsuspend.push(monitor.id),
            _ => {}
        }
    }

    plan
}

// ─── Unit tests for the pure policy ────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use domain::{
        MonitorBuilder, entities::Monitor,
        enums::{HttpMethod, MonitorKind},
        shared::ids::{MonitorId, TenantId, UserId},
        value_objects::{
            CheckInterval, ExpectedStatus, MonitorName, Timeout, Url,
        },
    };

    fn m(
        id: u128,
        age_secs: i64,
        suspended: bool,
    ) -> Monitor {
        let mut monitor = MonitorBuilder::new(
            MonitorId(uuid_from_u128(id)),
            UserId::new(),
            TenantId::new(),
        )
        .name(MonitorName::new(format!("m-{id}")).unwrap())
        .url(Url::new("https://example.com").unwrap())
        .method(HttpMethod::Get)
        .expected_status(ExpectedStatus::new(200).unwrap())
        .interval(CheckInterval::new(300).unwrap())
        .timeout(Timeout::new(10).unwrap())
        .kind(MonitorKind::Http)
        .build();
        monitor.created_at = Utc::now() - Duration::seconds(age_secs);
        monitor.entitlement_suspended = suspended;
        monitor
    }

    fn uuid_from_u128(n: u128) -> uuid::Uuid {
        uuid::Uuid::from_u128(n)
    }

    // ── Under limit: no changes ─────────────────────────────────────

    #[test]
    fn under_limit_does_nothing() {
        let monitors = vec![m(1, 100, false), m(2, 50, false), m(3, 10, false)];
        let plan = plan_suspension(&monitors, 10);
        assert!(plan.to_suspend.is_empty());
        assert!(plan.to_unsuspend.is_empty());
    }

    // ── Exactly at limit: no changes ────────────────────────────────

    #[test]
    fn exactly_at_limit_does_nothing() {
        let monitors = vec![m(1, 100, false), m(2, 50, false)];
        let plan = plan_suspension(&monitors, 2);
        assert!(plan.to_suspend.is_empty());
        assert!(plan.to_unsuspend.is_empty());
    }

    // ── Over limit: suspend the NEWEST ones ─────────────────────────

    #[test]
    fn over_limit_suspends_the_newest_monitors() {
        // created_at: 1 is oldest (age 100s), 4 is newest (age 1s).
        let oldest = m(1, 100, false);
        let second = m(2, 50, false);
        let third = m(3, 10, false);
        let newest = m(4, 1, false);
        let monitors = vec![third.clone(), newest.clone(), oldest.clone(), second.clone()];

        let plan = plan_suspension(&monitors, 2);

        // Keep oldest two (id=1, id=2), suspend the rest.
        assert_eq!(plan.to_unsuspend, Vec::<MonitorId>::new());
        assert_eq!(plan.to_suspend.len(), 2);
        let suspended_set: std::collections::HashSet<_> =
            plan.to_suspend.iter().copied().collect();
        assert!(suspended_set.contains(&third.id));
        assert!(suspended_set.contains(&newest.id));
    }

    // ── Idempotency: over limit, already suspended correctly ───────

    #[test]
    fn over_limit_with_already_correct_state_does_nothing() {
        let oldest = m(1, 100, false);
        let newest_suspended = m(2, 1, true);
        let monitors = vec![oldest, newest_suspended];

        let plan = plan_suspension(&monitors, 1);

        assert!(plan.to_suspend.is_empty());
        assert!(plan.to_unsuspend.is_empty());
    }

    // ── Upgrade path: back under limit unsuspends ──────────────────

    #[test]
    fn upgrade_unsuspends_previously_suspended() {
        // All three exist, but limit is now 5 → all eligible.
        let s1 = m(1, 100, true);
        let s2 = m(2, 50, true);
        let s3 = m(3, 10, false);
        let monitors = vec![s1.clone(), s2.clone(), s3.clone()];

        let plan = plan_suspension(&monitors, 5);

        assert!(plan.to_suspend.is_empty());
        assert_eq!(plan.to_unsuspend.len(), 2);
        let unsuspended_set: std::collections::HashSet<_> =
            plan.to_unsuspend.iter().copied().collect();
        assert!(unsuspended_set.contains(&s1.id));
        assert!(unsuspended_set.contains(&s2.id));
        assert!(!unsuspended_set.contains(&s3.id));
    }

    // ── Determinism: same input → same output ──────────────────────

    #[test]
    fn same_input_produces_same_plan_twice() {
        let monitors = vec![
            m(3, 30, false),
            m(1, 100, false),
            m(2, 60, false),
            m(4, 10, false),
        ];
        let a = plan_suspension(&monitors, 2);
        let b = plan_suspension(&monitors, 2);
        assert_eq!(a, b);
    }

    // ── Empty monitors: no-op ──────────────────────────────────────

    #[test]
    fn empty_monitor_list_returns_empty_plan() {
        let plan = plan_suspension(&[], 5);
        assert!(plan.to_suspend.is_empty());
        assert!(plan.to_unsuspend.is_empty());
    }

    // ── Zero limit: suspend everything ─────────────────────────────

    #[test]
    fn zero_limit_suspends_all() {
        let m1 = m(1, 100, false);
        let m2 = m(2, 50, false);
        let monitors = vec![m1.clone(), m2.clone()];

        let plan = plan_suspension(&monitors, 0);

        assert!(plan.to_unsuspend.is_empty());
        assert_eq!(plan.to_suspend.len(), 2);
        let suspended_set: std::collections::HashSet<_> =
            plan.to_suspend.iter().copied().collect();
        assert!(suspended_set.contains(&m1.id));
        assert!(suspended_set.contains(&m2.id));
    }
}
