use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a help article.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HelpArticleId(pub Uuid);

impl HelpArticleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for HelpArticleId {
    fn default() -> Self {
        Self::new()
    }
}
