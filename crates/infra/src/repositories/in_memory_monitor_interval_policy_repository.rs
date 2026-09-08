use application::ports::MonitorIntervalPolicyRepository;
use async_trait::async_trait;
use domain::MonitorKind;
use std::{collections::HashMap, sync::{Arc, RwLock}};
use std::error::Error;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone)]
pub struct InMemoryMonitorIntervalPolicyRepository {
    policies: Arc<RwLock<HashMap<String, u32>>>,
}

impl Default for InMemoryMonitorIntervalPolicyRepository {
    fn default() -> Self {
        let policies = [
            ("http".to_owned(), 30),
            ("tcp".to_owned(), 30),
            ("dns".to_owned(), 30),
            ("heartbeat".to_owned(), 30),
            ("flow".to_owned(), 60),
        ].into_iter().collect();
        Self { policies: Arc::new(RwLock::new(policies)) }
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

