use domain::StatusPageId;

/// Command for removing (unclaiming) a status page's custom domain,
/// regardless of its current verification state (`Pending` or
/// `Verified`). Idempotent: removing when there is no custom domain set
/// is a no-op, not an error.
#[derive(Debug, Clone)]
pub struct RemoveCustomDomain {
    pub id: StatusPageId,
}
