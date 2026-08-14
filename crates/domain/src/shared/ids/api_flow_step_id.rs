use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApiFlowStepId(pub Uuid);
impl ApiFlowStepId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for ApiFlowStepId {
    fn default() -> Self {
        Self::new()
    }
}
impl From<Uuid> for ApiFlowStepId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}
impl std::fmt::Display for ApiFlowStepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
