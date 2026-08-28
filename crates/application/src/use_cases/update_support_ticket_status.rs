use std::error::Error;

use domain::enums::SupportTicketStatus;
use domain::shared::ids::SupportTicketId;
use domain::shared::policies::AuthorizationPolicy;
use domain::shared::types::AuthenticatedUser;

use crate::ports::SupportTicketRepository;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Updates a support ticket status, enforcing tenant isolation.
pub struct UpdateSupportTicketStatusUseCase<R> {
    repository: R,
}

impl<R> UpdateSupportTicketStatusUseCase<R>
where
    R: SupportTicketRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        ticket_id: SupportTicketId,
        target_status: SupportTicketStatus,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Option<()>> {
        let ticket = match self.repository.find_ticket(ticket_id).await? {
            Some(ticket) => ticket,
            None => return Ok(None),
        };

        if !AuthorizationPolicy::same_tenant(&authenticated_user.tenant_context(), ticket.tenant_id)
        {
            return Ok(None);
        }

        let mut updated = ticket.clone();
        if updated.update_status(target_status).is_err() {
            return Ok(None);
        }

        self.repository
            .update_ticket_status(ticket_id, target_status)
            .await?;
        Ok(Some(()))
    }
}
