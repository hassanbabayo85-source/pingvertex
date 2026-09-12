use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::{SupportCategory, SupportTicketStatus};
use crate::errors::DomainError;
use crate::shared::ids::{SupportTicketId, TenantId, UserId};

/// Data required to create a support ticket.
#[derive(Debug, Clone)]
pub struct CreateSupportTicket {
    pub id: SupportTicketId,
    pub ticket_number: String,
    pub tenant_id: TenantId,
    pub created_by: UserId,
    pub subject: String,
    pub category: SupportCategory,
    pub status: SupportTicketStatus,
}

/// A support ticket raised by a tenant member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportTicket {
    pub id: SupportTicketId,
    pub ticket_number: String,
    pub tenant_id: TenantId,
    pub created_by: UserId,
    pub subject: String,
    pub category: SupportCategory,
    pub status: SupportTicketStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SupportTicket {
    pub const MAX_SUBJECT_LENGTH: usize = 200;
    pub const MAX_TICKET_NUMBER_LENGTH: usize = 50;

    /// Creates a new support ticket with validation.
    pub fn create(input: CreateSupportTicket) -> Result<Self, DomainError> {
        let subject = input.subject.trim();
        if subject.is_empty() {
            return Err(DomainError::EmptySupportSubject);
        }
        if subject.chars().count() > Self::MAX_SUBJECT_LENGTH {
            return Err(DomainError::SupportSubjectTooLong);
        }

        let ticket_number = input.ticket_number.trim();
        if ticket_number.trim().is_empty() {
            return Err(DomainError::EmptySupportTicketNumber);
        }
        if ticket_number.chars().count() > Self::MAX_TICKET_NUMBER_LENGTH {
            return Err(DomainError::SupportTicketNumberTooLong);
        }

        let now = Utc::now();

        Ok(Self {
            id: input.id,
            ticket_number: ticket_number.to_owned(),
            tenant_id: input.tenant_id,
            created_by: input.created_by,
            subject: subject.to_owned(),
            category: input.category,
            status: input.status,
            created_at: now,
            updated_at: now,
        })
    }

    /// Attempts to transition the ticket to `target`.
    pub fn update_status(&mut self, target: SupportTicketStatus) -> Result<(), DomainError> {
        if !self.status.can_transition_to(target) {
            return Err(DomainError::InvalidSupportTicketTransition(
                self.status.as_str().to_string(),
                target.as_str().to_string(),
            ));
        }
        self.status = target;
        self.updated_at = Utc::now();
        Ok(())
    }
}
