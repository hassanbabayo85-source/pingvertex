use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use application::ports::{AgentLease, AgentLeaseRepository, RepoResult, SubmitAgentResultOutcome};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::shared::ids::{CheckId, MonitorId};
use domain::{Monitor, RegionCode};
use uuid::Uuid;

#[derive(Default)]
struct State {
    due_monitors: Vec<Monitor>,
    leases: HashMap<Uuid, StoredLease>,
}

struct StoredLease {
    monitor_id: MonitorId,
    region: RegionCode,
    agent_id: String,
    lease_expires_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Default)]
pub struct InMemoryAgentLeaseRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryAgentLeaseRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed_due_monitor(&self, monitor: Monitor) {
        self.state.lock().unwrap().due_monitors.push(monitor);
    }

    pub fn seed_lease(
        &self,
        check_id: CheckId,
        monitor_id: MonitorId,
        region: RegionCode,
        agent_id: &str,
        lease_expires_at: DateTime<Utc>,
        completed_at: Option<DateTime<Utc>>,
    ) {
        self.state.lock().unwrap().leases.insert(
            check_id.0,
            StoredLease {
                monitor_id,
                region,
                agent_id: agent_id.to_string(),
                lease_expires_at,
                completed_at,
            },
        );
    }
}

#[async_trait]
impl AgentLeaseRepository for InMemoryAgentLeaseRepository {
    async fn claim_due_checks(
        &self,
        agent_id: &str,
        region: &RegionCode,
        limit: u32,
        lease_duration: Duration,
        now: DateTime<Utc>,
    ) -> RepoResult<Vec<AgentLease>> {
        let mut state = self.state.lock().unwrap();

        state
            .leases
            .retain(|_, lease| lease.completed_at.is_some() || lease.lease_expires_at > now);

        let mut claimed = Vec::new();
        let mut remaining = Vec::new();

        let due_monitors = std::mem::take(&mut state.due_monitors);
        for monitor in due_monitors {
            if claimed.len() >= limit as usize {
                remaining.push(monitor);
                continue;
            }

            if !monitor.regions.iter().any(|r| r == region) {
                remaining.push(monitor);
                continue;
            }

            let check_id = CheckId::new();
            let lease_expires_at = now + chrono::Duration::seconds(lease_duration.as_secs() as i64);

            state.leases.insert(
                check_id.0,
                StoredLease {
                    monitor_id: monitor.id,
                    region: region.clone(),
                    agent_id: agent_id.to_string(),
                    lease_expires_at,
                    completed_at: None,
                },
            );

            claimed.push(AgentLease {
                check_id,
                monitor_id: monitor.id,
                region: region.clone(),
                lease_expires_at,
            });
        }

        state.due_monitors = remaining;
        Ok(claimed)
    }

    async fn submit_result(
        &self,
        agent_id: &str,
        check_id: CheckId,
        monitor_id: MonitorId,
        region: &RegionCode,
        now: DateTime<Utc>,
    ) -> RepoResult<SubmitAgentResultOutcome> {
        let mut state = self.state.lock().unwrap();

        let Some(lease) = state.leases.get_mut(&check_id.0) else {
            return Ok(SubmitAgentResultOutcome::Stale);
        };

        if lease.completed_at.is_some() {
            return Ok(SubmitAgentResultOutcome::Duplicate);
        }

        if lease.monitor_id != monitor_id || &lease.region != region || lease.agent_id != agent_id {
            return Ok(SubmitAgentResultOutcome::Stale);
        }

        if lease.lease_expires_at <= now {
            return Ok(SubmitAgentResultOutcome::Stale);
        }

        lease.completed_at = Some(now);
        Ok(SubmitAgentResultOutcome::Accepted)
    }

    async fn recover_expired_leases(&self, now: DateTime<Utc>) -> RepoResult<u64> {
        let mut state = self.state.lock().unwrap();
        let before = state.leases.len();
        state
            .leases
            .retain(|_, lease| lease.completed_at.is_some() || lease.lease_expires_at > now);
        Ok((before - state.leases.len()) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::shared::ids::{TenantId, UserId};
    use domain::{
        CheckInterval, ExpectedStatus, HttpMethod, MonitorBuilder, MonitorName, Timeout, Url,
    };

    fn build_monitor() -> Monitor {
        MonitorBuilder::new(MonitorId::new(), UserId::new(), TenantId::new())
            .name(MonitorName::new("agent lease test monitor").unwrap())
            .url(Url::new("http://example.com").unwrap())
            .method(HttpMethod::Get)
            .expected_status(ExpectedStatus::new(200).unwrap())
            .interval(CheckInterval::new(60).unwrap())
            .timeout(Timeout::new(5).unwrap())
            .build()
    }

    #[tokio::test]
    async fn recover_expired_lease_allows_monitor_to_be_claimed_again() {
        let repo = InMemoryAgentLeaseRepository::new();
        let monitor = build_monitor();
        let region = RegionCode::new("us-east").unwrap();

        repo.seed_due_monitor(monitor.clone());
        repo.seed_lease(
            CheckId::new(),
            monitor.id,
            region.clone(),
            "agent-1",
            Utc::now() - chrono::Duration::seconds(1),
            None,
        );

        let claimed = repo
            .claim_due_checks("agent-1", &region, 10, Duration::from_secs(60), Utc::now())
            .await
            .unwrap();

        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].monitor_id, monitor.id);
        assert_eq!(claimed[0].region, region);
    }
}
