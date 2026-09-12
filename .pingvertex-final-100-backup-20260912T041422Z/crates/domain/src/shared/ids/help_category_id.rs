use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a help category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HelpCategoryId(pub Uuid);

impl HelpCategoryId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for HelpCategoryId {
    fn default() -> Self {
        Self::new()
    }
}
