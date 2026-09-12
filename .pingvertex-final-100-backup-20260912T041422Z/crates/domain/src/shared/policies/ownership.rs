use crate::shared::ids::UserId;
use crate::shared::types::AuthenticatedUser;

/// Generic ownership check.
///
/// Used to prevent IDOR (Insecure Direct Object Reference)
/// vulnerabilities by ensuring the authenticated user owns
/// the requested resource.
pub trait Ownable {
    fn owner_id(&self) -> UserId;
}

pub struct OwnershipPolicy;

impl OwnershipPolicy {
    /// Fail-closed ownership verification.
    #[inline]
    pub fn can_access<T: Ownable>(actor: &AuthenticatedUser, resource: &T) -> bool {
        actor.user_id() == resource.owner_id()
    }
}
