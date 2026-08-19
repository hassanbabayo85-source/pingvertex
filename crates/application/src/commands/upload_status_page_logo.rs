use domain::StatusPageId;

/// Command for uploading a status page logo.
#[derive(Debug, Clone)]
pub struct UploadStatusPageLogo {
    pub id: StatusPageId,
    pub data: Vec<u8>,
}
