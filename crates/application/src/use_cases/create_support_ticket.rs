use std::error::Error;
use std::fmt;

use domain::entities::{CreateSupportTicket, SupportTicket};
use domain::enums::{SupportCategory, SupportTicketStatus};
use domain::shared::ids::SupportTicketId;
use domain::shared::types::{AuthenticatedUser, Permission};
use uuid::Uuid;

use crate::ports::SupportTicketRepository;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
pub enum CreateSupportTicketError {
    /// Caller's role does not carry `Permission::CreateSupportTicket`
    /// (currently only `Auditor`/`Viewer` lack it — both are meant to be
    /// read-only). See P1 "Support Ticket Authorization" in the 2026-09
    /// security review.
    Forbidden,
    Validation(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for CreateSupportTicketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forbidden => write!(f, "you do not have permission to create support tickets"),
            Self::Validation(msg) => write!(f, "{msg}"),
            Self::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CreateSupportTicketError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Forbidden => None, // <-- Wannan shine layin da zaka kara
            Self::Validation(_) => None,
            Self::Other(err) => Some(err.as_ref()),
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
        authenticated_user: AuthenticatedUser,
        subject: String,
        category: SupportCategory,
    ) -> Result<SupportTicket, CreateSupportTicketError> {
        if !authenticated_user.can(Permission::CreateSupportTicket) {
            return Err(CreateSupportTicketError::Forbidden);
        }

        let tenant_id = authenticated_user.tenant_context().tenant_id();
        let created_by = authenticated_user.user_id();

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
