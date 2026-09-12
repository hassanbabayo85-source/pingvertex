use crate::api::{client, monitoring::Monitor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StatusPage {
    pub id: String,
    pub name: String,
    pub subdomain: String,
    pub custom_domain: Option<String>,
    pub custom_domain_verified: bool,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub monitor_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateStatusPageRequest {
    pub name: String,
    pub subdomain: String,
    pub monitor_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatusPageRequest {
    pub name: String,
    pub monitor_ids: Vec<String>,
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomDomainRequest {
    pub domain: String,
}

pub async fn list() -> Result<Vec<StatusPage>, client::ApiError> {
    client::get_json("/status-pages").await
}

pub async fn create(request: &CreateStatusPageRequest) -> Result<StatusPage, client::ApiError> {
    client::post_json("/status-pages", request).await
}

pub async fn update(
    id: &str,
    request: &UpdateStatusPageRequest,
) -> Result<StatusPage, client::ApiError> {
    client::put_json(&format!("/status-pages/{id}"), request).await
}

pub async fn delete(id: &str) -> Result<(), client::ApiError> {
    client::delete_json_no_content(&format!("/status-pages/{id}")).await
}

pub async fn request_custom_domain(id: &str, domain: &str) -> Result<StatusPage, client::ApiError> {
    client::post_json(
        &format!("/status-pages/{id}/custom-domain"),
        &CustomDomainRequest {
            domain: domain.to_string(),
        },
    )
    .await
}

pub async fn remove_custom_domain(id: &str) -> Result<StatusPage, client::ApiError> {
    client::delete_json(&format!("/status-pages/{id}/custom-domain")).await
}

pub async fn upload_logo(
    id: &str,
    bytes: Vec<u8>,
    file_name: &str,
    mime: &str,
) -> Result<StatusPage, client::ApiError> {
    client::post_multipart_file(
        &format!("/status-pages/{id}/logo"),
        "logo",
        file_name,
        mime,
        bytes,
    )
    .await
}

pub async fn monitors() -> Result<Vec<Monitor>, client::ApiError> {
    client::get_json("/monitors").await
}
