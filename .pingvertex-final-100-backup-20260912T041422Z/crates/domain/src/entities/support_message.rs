use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::shared::ids::{SupportMessageId, SupportTicketId, UserId};

/// Data required to create a support message.
#[derive(Debug, Clone)]
pub struct CreateSupportMessage {
    pub id: SupportMessageId,
    pub ticket_id: SupportTicketId,
    pub author_id: UserId,
    pub message: String,
}

/// A single message within a support ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportMessage {
    pub id: SupportMessageId,
    pub ticket_id: SupportTicketId,
    pub author_id: UserId,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

impl SupportMessage {
    pub const MAX_MESSAGE_LENGTH: usize = 20_000;

    /// Creates a new support message with validation.
    pub fn create(input: CreateSupportMessage) -> Result<Self, DomainError> {
        let message = input.message.trim();
        if message.is_empty() {
            return Err(DomainError::EmptySupportMessage);
        }
        if message.chars().count() > Self::MAX_MESSAGE_LENGTH {
            return Err(DomainError::SupportMessageTooLong);
        }

        Ok(Self {
            id: input.id,
            ticket_id: input.ticket_id,
            author_id: input.author_id,
            message: message.to_owned(),
            created_at: Utc::now(),
        })
    }
}
