use std::error::Error;
use std::fmt;

use domain::entities::{CreateSupportTicket, SupportTicket};
use domain::enums::{SupportCategory, SupportTicketStatus};
use domain::shared::ids::{SupportTicketId, TenantId, UserId};
use uuid::Uuid;

use crate::ports::SupportTicketRepository;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum CreateSupportTicketError {
    Validation(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateSupportTicketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(msg) => write!(f, "{msg}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateSupportTicketError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Other(err) => Some(err.as_ref()),
            Self::Validation(_) => None,
        }
    }
}

/// Creates a new support ticket for a member.
pub struct CreateSupportTicketUseCase<R> {
    repository: R,
}

impl<R> CreateSupportTicketUseCase<R>
where
    R: SupportTicketRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        tenant_id: TenantId,
        created_by: UserId,
        subject: String,
        category: SupportCategory,
    ) -> Result<SupportTicket, CreateSupportTicketError> {
        let ticket_id = SupportTicketId::new();
        let ticket_number = format!(
            "PV-{}",
            Uuid::new_v4().simple().to_string()[..8].to_uppercase()
        );

        let create = CreateSupportTicket {
            id: ticket_id,
            ticket_number,
            tenant_id,
            created_by,
            subject,
            category,
            status: SupportTicketStatus::Open,
        };

        let ticket = SupportTicket::create(create)
            .map_err(|e| CreateSupportTicketError::Validation(e.to_string()))?;

        self.repository
            .create_ticket(&ticket)
            .await
            .map_err(CreateSupportTicketError::Other)?;

        Ok(ticket)
    }
}
