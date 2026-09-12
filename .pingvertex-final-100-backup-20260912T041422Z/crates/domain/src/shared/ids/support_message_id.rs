use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a support ticket message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SupportMessageId(pub Uuid);

impl SupportMessageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SupportMessageId {
    fn default() -> Self {
        Self::new()
    }
}
