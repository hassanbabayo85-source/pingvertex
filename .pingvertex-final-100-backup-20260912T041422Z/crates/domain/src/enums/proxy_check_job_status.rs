use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyCheckJobStatus {
    #[serde(alias = "Pending")]
    Pending,
    #[serde(alias = "Running")]
    Running,
    #[serde(alias = "Completed")]
    Completed,
    #[serde(alias = "Failed")]
    Failed,
    #[serde(alias = "Canceled")]
    Canceled,
}
