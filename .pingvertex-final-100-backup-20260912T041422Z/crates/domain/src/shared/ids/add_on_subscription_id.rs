use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AddOnSubscriptionId(pub Uuid);
impl AddOnSubscriptionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
    pub fn value(&self) -> Uuid {
        self.0
    }
}
impl Default for AddOnSubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}
