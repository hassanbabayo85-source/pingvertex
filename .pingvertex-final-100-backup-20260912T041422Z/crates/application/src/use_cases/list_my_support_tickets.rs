use std::error::Error;

use domain::entities::SupportTicket;
use domain::shared::types::{AuthenticatedUser, Permission};

use crate::ports::SupportTicketRepository;

pub type UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Lists support tickets for the authenticated user.
pub struct ListMySupportTicketsUseCase<R> {
    repository: R,
}

impl<R> ListMySupportTicketsUseCase<R>
where
    R: SupportTicketRepository,
{
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        authenticated_user: AuthenticatedUser,
    ) -> UseCaseResult<Vec<SupportTicket>> {
        if !authenticated_user.can(Permission::ViewSupportTickets) {
            return Ok(Vec::new());
        }

        let tenant_id = authenticated_user.tenant_context().tenant_id();
        let user_id = authenticated_user.user_id();
        self.repository.list_user_tickets(tenant_id, user_id).await
    }
}
