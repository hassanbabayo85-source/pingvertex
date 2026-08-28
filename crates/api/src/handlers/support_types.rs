use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateSupportTicketRequest {
    pub subject: String,
    pub category: String,
}

#[derive(Debug, Deserialize)]
pub struct ReplyToSupportTicketRequest {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSupportTicketStatusRequest {
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct SupportTicketResponse {
    pub id: String,
    pub ticket_number: String,
    pub subject: String,
    pub category: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<domain::entities::SupportTicket> for SupportTicketResponse {
    fn from(t: domain::entities::SupportTicket) -> Self {
        Self {
            id: t.id.0.to_string(),
            ticket_number: t.ticket_number,
            subject: t.subject,
            category: t.category.as_str().to_string(),
            status: t.status.as_str().to_string(),
            created_at: t.created_at.to_rfc3339(),
            updated_at: t.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SupportMessageResponse {
    pub id: String,
    pub ticket_id: String,
    pub author_id: String,
    pub message: String,
    pub created_at: String,
}

impl From<domain::entities::SupportMessage> for SupportMessageResponse {
    fn from(m: domain::entities::SupportMessage) -> Self {
        Self {
            id: m.id.0.to_string(),
            ticket_id: m.ticket_id.0.to_string(),
            author_id: m.author_id.0.to_string(),
            message: m.message,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}
