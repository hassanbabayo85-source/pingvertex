use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RumSiteId(pub Uuid);
impl RumSiteId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for RumSiteId {
    fn default() -> Self {
        Self::new()
    }
}
impl From<Uuid> for RumSiteId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}
impl std::fmt::Display for RumSiteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
