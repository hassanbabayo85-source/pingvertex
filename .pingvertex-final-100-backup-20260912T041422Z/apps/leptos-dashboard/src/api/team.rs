use crate::api::client::{self, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Member {
    pub user_id: String,
    pub role: String,
    pub invited_at: String,
    pub joined_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Invitation {
    pub id: String,
    pub email: String,
    pub role: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreatedInvitation {
    pub id: String,
    pub email: String,
    pub role: String,
    pub expires_at: String,
    pub created_at: String,
    pub invitation_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddMemberRequest {
    pub user_id: String,
    pub role: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct RoleRequest {
    pub role: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct CreateInvitationRequest {
    pub email: String,
    pub role: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct AcceptInvitationRequest {
    pub token: String,
}

pub async fn list_members() -> Result<Vec<Member>, ApiError> {
    client::get_json("/members").await
}
pub async fn add_member(request: &AddMemberRequest) -> Result<Member, ApiError> {
    client::post_json("/members", request).await
}
pub async fn change_role(user_id: &str, role: &str) -> Result<(), ApiError> {
    client::put_json_no_content(
        &format!("/members/{user_id}/role"),
        &RoleRequest {
            role: role.to_string(),
        },
    )
    .await
}
pub async fn remove_member(user_id: &str) -> Result<(), ApiError> {
    client::delete_json_no_content(&format!("/members/{user_id}")).await
}
pub async fn list_invitations() -> Result<Vec<Invitation>, ApiError> {
    client::get_json("/invitations/pending").await
}
pub async fn create_invitation(
    request: &CreateInvitationRequest,
) -> Result<CreatedInvitation, ApiError> {
    client::post_json("/invitations", request).await
}
pub async fn revoke_invitation(id: &str) -> Result<(), ApiError> {
    client::delete_json_no_content(&format!("/invitations/{id}")).await
}
pub async fn accept_invitation(token: &str) -> Result<Member, ApiError> {
    client::post_json(
        "/invitations/accept",
        &AcceptInvitationRequest {
            token: token.to_string(),
        },
    )
    .await
}
