use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyCheckJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}
