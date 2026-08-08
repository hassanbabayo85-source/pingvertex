use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssertionId(pub Uuid);

impl AssertionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AssertionId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for AssertionId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for AssertionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
