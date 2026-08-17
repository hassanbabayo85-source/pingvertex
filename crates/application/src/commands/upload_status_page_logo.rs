use domain::{StatusPageId, UserId};

/// Command for uploading a status page logo.
///
/// `data` is the raw uploaded image bytes. The use case validates them via
/// `domain::LogoImage`, which performs magic-byte detection and size
/// enforcement — the client-supplied filename/content-type are intentionally
/// not part of this command.
#[derive(Debug, Clone)]
pub struct UploadStatusPageLogo {
    pub id: StatusPageId,
    pub user_id: UserId,
    pub data: Vec<u8>,
}
