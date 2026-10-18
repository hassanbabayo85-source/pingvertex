use async_trait::async_trait;
use std::error::Error;

use crate::pagination::{Cursor, Page, PageRequest, paginate_in_memory};

use domain::entities::{SupportMessage, SupportTicket};
use domain::shared::ids::{SupportTicketId, TenantId, UserId};

pub type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Support ticket storage with tenant isolation.
#[async_trait]
pub trait SupportTicketRepository: Send + Sync {
    async fn create_ticket(&self, ticket: &SupportTicket) -> RepoResult<()>;
    async fn find_ticket(&self, id: SupportTicketId) -> RepoResult<Option<SupportTicket>>;
    async fn list_user_tickets(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<Vec<SupportTicket>>;
    async fn list_tenant_tickets(&self, tenant_id: TenantId) -> RepoResult<Vec<SupportTicket>>;
    async fn create_message(&self, message: &SupportMessage) -> RepoResult<()>;
    async fn list_messages(&self, ticket_id: SupportTicketId) -> RepoResult<Vec<SupportMessage>>;
    async fn update_ticket_status(
        &self,
        id: SupportTicketId,
        status: domain::enums::SupportTicketStatus,
    ) -> RepoResult<()>;

    /// PART 8 — cursor-paginated variant of `list_user_tickets`.
    async fn list_user_tickets_paginated(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        page: PageRequest,
    ) -> RepoResult<Page<SupportTicket>> {
        let all = self.list_user_tickets(tenant_id, user_id).await?;
        Ok(paginate_in_memory(all, page, |t| {
            Cursor::new(t.created_at, t.id.0)
        }))
    }

    /// PART 8 — cursor-paginated variant of `list_tenant_tickets`.
    async fn list_tenant_tickets_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<SupportTicket>> {
        let all = self.list_tenant_tickets(tenant_id).await?;
        Ok(paginate_in_memory(all, page, |t| {
            Cursor::new(t.created_at, t.id.0)
        }))
    }
}

#[async_trait]
impl<T> SupportTicketRepository for std::sync::Arc<T>
where
    T: SupportTicketRepository + Send + Sync + ?Sized,
{
    async fn create_ticket(&self, ticket: &SupportTicket) -> RepoResult<()> {
        (**self).create_ticket(ticket).await
    }
    async fn find_ticket(&self, id: SupportTicketId) -> RepoResult<Option<SupportTicket>> {
        (**self).find_ticket(id).await
    }
    async fn list_user_tickets(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<Vec<SupportTicket>> {
        (**self).list_user_tickets(tenant_id, user_id).await
    }
    async fn list_tenant_tickets(&self, tenant_id: TenantId) -> RepoResult<Vec<SupportTicket>> {
        (**self).list_tenant_tickets(tenant_id).await
    }
    async fn create_message(&self, message: &SupportMessage) -> RepoResult<()> {
        (**self).create_message(message).await
    }
    async fn list_messages(&self, ticket_id: SupportTicketId) -> RepoResult<Vec<SupportMessage>> {
        (**self).list_messages(ticket_id).await
    }
    async fn update_ticket_status(
        &self,
        id: SupportTicketId,
        status: domain::enums::SupportTicketStatus,
    ) -> RepoResult<()> {
        (**self).update_ticket_status(id, status).await
    }

    async fn list_user_tickets_paginated(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        page: PageRequest,
    ) -> RepoResult<Page<SupportTicket>> {
        (**self)
            .list_user_tickets_paginated(tenant_id, user_id, page)
            .await
    }

    async fn list_tenant_tickets_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<SupportTicket>> {
        (**self)
            .list_tenant_tickets_paginated(tenant_id, page)
            .await
    }
}
