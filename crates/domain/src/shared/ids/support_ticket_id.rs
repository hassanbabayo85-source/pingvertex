use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a support ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SupportTicketId(pub Uuid);

impl SupportTicketId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SupportTicketId {
    fn default() -> Self {
        Self::new()
    }
}
