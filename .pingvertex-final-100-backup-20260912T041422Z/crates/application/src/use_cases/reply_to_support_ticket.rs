use std::error::Error;

use domain::entities::{CreateSupportMessage, SupportMessage};
use domain::shared::ids::{SupportMessageId, SupportTicketId};
use domain::shared::policies::AuthorizationPolicy;
use domain::shared::types::{AuthenticatedUser, Permission};

use crate::ports::SupportTicketRepository;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Adds a message to a support ticket, enforcing tenant isolation.
pub struct ReplyToSupportTicketUseCase<R> {
    repository: R,
}

impl<R> ReplyToSupportTicketUseCase<R>
where
    R: SupportTicketRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        ticket_id: SupportTicketId,
        authenticated_user: AuthenticatedUser,
        message: String,
    ) -> UseCaseResult<Option<SupportMessage>> {
        if !authenticated_user.can(Permission::ReplySupportTicket) {
            return Ok(None);
        }

        let ticket = match self.repository.find_ticket(ticket_id).await? {
            Some(ticket) => ticket,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(&authenticated_user.tenant_context(), ticket.tenant_id)
        {
            return Ok(None);
        }

        // SECURITY: same ownership rule as `GetSupportTicketUseCase` —
        // only the ticket's own creator, or an Admin/Owner (via
        // `ManageSupportTicketStatus`), may reply to it. Without this, any
        // tenant member holding the base `ReplySupportTicket` permission
        // could post messages onto another member's private ticket.
        let is_owner = ticket.created_by == authenticated_user.user_id();
        let is_tenant_manager = authenticated_user.can(Permission::ManageSupportTicketStatus);
        if !is_owner && !is_tenant_manager {
            return Ok(None);
        }

        let message_id = SupportMessageId::new();
        let create = CreateSupportMessage {
            id: message_id,
            ticket_id,
            author_id: authenticated_user.user_id(),
            message,
        };

        let msg = SupportMessage::create(create)?;
        self.repository.create_message(&msg).await?;
        Ok(Some(msg))
    }
}
