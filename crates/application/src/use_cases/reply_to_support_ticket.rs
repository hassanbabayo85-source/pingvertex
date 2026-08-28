use std::error::Error;

use domain::entities::{CreateSupportMessage, SupportMessage};
use domain::shared::ids::{SupportMessageId, SupportTicketId};
use domain::shared::policies::AuthorizationPolicy;
use domain::shared::types::AuthenticatedUser;

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
        let ticket = match self.repository.find_ticket(ticket_id).await? {
            Some(ticket) => ticket,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(&authenticated_user.tenant_context(), ticket.tenant_id)
        {
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
