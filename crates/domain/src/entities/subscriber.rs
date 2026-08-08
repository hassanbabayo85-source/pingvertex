use chrono::{DateTime, Utc};

use crate::{
    shared::ids::{MonitorId, SubscriberId},
    value_objects::Email,
};

#[derive(Debug, Clone)]
pub struct CreateSubscriber {
    pub id: SubscriberId,
    pub monitor_id: MonitorId,
    pub email: Email,
}

#[derive(Debug, Clone)]
pub struct Subscriber {
    pub id: SubscriberId,
    pub monitor_id: MonitorId,
    pub email: Email,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
}

impl Subscriber {
    pub fn create(input: CreateSubscriber) -> Self {
        Self {
            id: input.id,
            monitor_id: input.monitor_id,
            email: input.email,
            verified: false,
            created_at: Utc::now(),
        }
    }

    pub fn verify(&mut self) {
        self.verified = true;
    }
}
