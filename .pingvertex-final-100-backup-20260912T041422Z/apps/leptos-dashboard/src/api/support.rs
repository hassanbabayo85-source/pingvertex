use crate::api::client::{self, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HelpCategory {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub sort_order: i32,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HelpArticle {
    pub id: String,
    pub category_id: String,
    pub title: String,
    pub slug: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Ticket {
    pub id: String,
    pub ticket_number: String,
    pub subject: String,
    pub category: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Message {
    pub id: String,
    pub ticket_id: String,
    pub author_id: String,
    pub message: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct CreateTicket {
    pub subject: String,
    pub category: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Reply {
    pub message: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct StatusRequest {
    pub status: String,
}

pub async fn categories() -> Result<Vec<HelpCategory>, ApiError> {
    client::get_json("/help/categories").await
}
pub async fn articles(
    category: Option<&str>,
    q: Option<&str>,
) -> Result<Vec<HelpArticle>, ApiError> {
    let mut query = Vec::new();
    if let Some(v) = category {
        query.push(("category", v));
    }
    if let Some(v) = q {
        query.push(("q", v));
    }
    client::get_json_query("/help/articles", &query).await
}
pub async fn article(slug: &str) -> Result<HelpArticle, ApiError> {
    client::get_json(&format!("/help/articles/{slug}")).await
}
pub async fn search(q: &str) -> Result<Vec<HelpArticle>, ApiError> {
    client::get_json_query("/help/search", &[("q", q)]).await
}
pub async fn tickets() -> Result<Vec<Ticket>, ApiError> {
    client::get_json("/support/tickets").await
}
pub async fn ticket(id: &str) -> Result<(Ticket, Vec<Message>), ApiError> {
    client::get_json(&format!("/support/tickets/{id}")).await
}
pub async fn create_ticket(req: &CreateTicket) -> Result<Ticket, ApiError> {
    client::post_json("/support/tickets", req).await
}
pub async fn reply(id: &str, message: &str) -> Result<Message, ApiError> {
    client::post_json(
        &format!("/support/tickets/{id}/messages"),
        &Reply {
            message: message.to_string(),
        },
    )
    .await
}
pub async fn set_status(id: &str, status: &str) -> Result<(), ApiError> {
    client::patch_json_no_content(
        &format!("/support/tickets/{id}/status"),
        &StatusRequest {
            status: status.to_string(),
        },
    )
    .await
}
