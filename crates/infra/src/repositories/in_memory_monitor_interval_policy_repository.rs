use application::ports::MonitorIntervalPolicyRepository;
use async_trait::async_trait;
use domain::MonitorKind;
use std::error::Error;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct InMemoryMonitorIntervalPolicyRepository {
    policies: Arc<RwLock<HashMap<String, u32>>>,
}

impl InMemoryMonitorIntervalPolicyRepository {
    /// Override one policy in tests without introducing a second fake.
    pub fn with_policy(self, kind: MonitorKind, min_interval_seconds: u32) -> Self {
        let key = match kind {
            MonitorKind::Http => "http",
            MonitorKind::Tcp => "tcp",
            MonitorKind::Dns => "dns",
            MonitorKind::Heartbeat => "heartbeat",
            MonitorKind::Flow => "flow",
        };
        self.policies
            .write()
            .unwrap()
            .insert(key.to_owned(), min_interval_seconds);
        self
    }
}

impl Default for InMemoryMonitorIntervalPolicyRepository {
    fn default() -> Self {
        let policies = [
            ("http".to_owned(), 30),
            ("tcp".to_owned(), 30),
            ("dns".to_owned(), 30),
            ("heartbeat".to_owned(), 30),
            ("flow".to_owned(), 60),
        ]
        .into_iter()
        .collect();
        Self {
            policies: Arc::new(RwLock::new(policies)),
        }
    }
}

#[async_trait]
impl MonitorIntervalPolicyRepository for InMemoryMonitorIntervalPolicyRepository {
    async fn minimum_interval_seconds(&self, kind: MonitorKind) -> RepoResult<u32> {
        let key = match kind {
            MonitorKind::Http => "http",
            MonitorKind::Tcp => "tcp",
            MonitorKind::Dns => "dns",
            MonitorKind::Heartbeat => "heartbeat",
            MonitorKind::Flow => "flow",
        };
        self.policies
            .read()
            .unwrap()
            .get(key)
            .copied()
            .ok_or_else(|| "missing interval policy".into())
    }
}
