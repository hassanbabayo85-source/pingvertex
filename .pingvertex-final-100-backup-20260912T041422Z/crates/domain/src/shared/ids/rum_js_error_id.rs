use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RumJsErrorId(pub Uuid);
impl RumJsErrorId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for RumJsErrorId {
    fn default() -> Self {
        Self::new()
    }
}
impl From<Uuid> for RumJsErrorId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}
impl std::fmt::Display for RumJsErrorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
