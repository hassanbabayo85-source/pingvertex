use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::shared::ids::{MonitorId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorCreated {
    pub monitor_id: MonitorId,
    pub user_id: UserId,
    pub occurred_at: DateTime<Utc>,
}

impl MonitorCreated {
    pub fn new(monitor_id: MonitorId, user_id: UserId) -> Self {
        Self {
            monitor_id,
            user_id,
            occurred_at: Utc::now(),
        }
    }
}
