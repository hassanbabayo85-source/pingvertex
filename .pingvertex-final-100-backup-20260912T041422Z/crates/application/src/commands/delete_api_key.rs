use domain::shared::ids::ApiKeyId;

#[derive(Debug, Clone, Copy)]
pub struct DeleteApiKey {
    pub id: ApiKeyId,
}
