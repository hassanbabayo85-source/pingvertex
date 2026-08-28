use application::ports::SupportTicketRepository;
use async_trait::async_trait;
use domain::entities::{SupportMessage, SupportTicket};
use domain::enums::SupportTicketStatus;
use domain::shared::ids::{SupportTicketId, TenantId, UserId};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

type R<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Default)]
pub struct InMemorySupportTicketRepository {
    tickets: Arc<Mutex<HashMap<SupportTicketId, SupportTicket>>>,
    messages: Arc<Mutex<HashMap<SupportTicketId, Vec<SupportMessage>>>>,
}

impl InMemorySupportTicketRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SupportTicketRepository for InMemorySupportTicketRepository {
    async fn create_ticket(&self, ticket: &SupportTicket) -> R<()> {
        self.tickets
            .lock()
            .unwrap()
            .insert(ticket.id, ticket.clone());
        self.messages.lock().unwrap().entry(ticket.id).or_default();
        Ok(())
    }

    async fn find_ticket(&self, id: SupportTicketId) -> R<Option<SupportTicket>> {
        Ok(self.tickets.lock().unwrap().get(&id).cloned())
    }

    async fn list_user_tickets(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> R<Vec<SupportTicket>> {
        Ok(self
            .tickets
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.tenant_id == tenant_id && t.created_by == user_id)
            .cloned()
            .collect())
    }

    async fn list_tenant_tickets(&self, tenant_id: TenantId) -> R<Vec<SupportTicket>> {
        Ok(self
            .tickets
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn create_message(&self, message: &SupportMessage) -> R<()> {
        let mut map = self.messages.lock().unwrap();
        let entry = map.entry(message.ticket_id).or_default();
        entry.push(message.clone());
        Ok(())
    }

    async fn list_messages(&self, ticket_id: SupportTicketId) -> R<Vec<SupportMessage>> {
        Ok(self
            .messages
            .lock()
            .unwrap()
            .get(&ticket_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn update_ticket_status(
        &self,
        id: SupportTicketId,
        status: SupportTicketStatus,
    ) -> R<()> {
        if let Some(ticket) = self.tickets.lock().unwrap().get_mut(&id) {
            ticket.status = status;
            ticket.updated_at = chrono::Utc::now();
        }
        Ok(())
    }
}
