use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorId(pub Uuid);

impl MonitorId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn generate() -> Self {
        Self::new()
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn into_uuid(self) -> Uuid {
        self.0
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl Default for MonitorId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MonitorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for MonitorId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl From<MonitorId> for Uuid {
    fn from(value: MonitorId) -> Self {
        value.0
    }
}
