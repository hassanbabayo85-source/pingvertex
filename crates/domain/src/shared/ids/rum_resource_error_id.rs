use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RumResourceErrorId(pub Uuid);
impl RumResourceErrorId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
impl Default for RumResourceErrorId {
    fn default() -> Self {
        Self::new()
    }
}
impl From<Uuid> for RumResourceErrorId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}
impl std::fmt::Display for RumResourceErrorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
