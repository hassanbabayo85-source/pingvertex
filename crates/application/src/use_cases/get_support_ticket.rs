use std::error::Error;

use domain::entities::{SupportMessage, SupportTicket};
use domain::shared::ids::SupportTicketId;
use domain::shared::policies::AuthorizationPolicy;
use domain::shared::types::{AuthenticatedUser, Permission};

use crate::ports::SupportTicketRepository;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Retrieves a support ticket and its messages, enforcing tenant isolation.
pub struct GetSupportTicketUseCase<R> {
    repository: R,
}

impl<R> GetSupportTicketUseCase<R>
where
    R: SupportTicketRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        id: SupportTicketId,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<(SupportTicket, Vec<SupportMessage>)>> {
        if !authenticated_user.can(Permission::ViewSupportTickets) {
            return Ok(None);
        }

        let ticket = match self.repository.find_ticket(id).await? {
            Some(ticket) => ticket,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(&authenticated_user.tenant_context(), ticket.tenant_id)
        {
            return Ok(None);
        }

        let messages = self.repository.list_messages(id).await?;
        Ok(Some((ticket, messages)))
    }
}
