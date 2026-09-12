use std::fmt;

/// Authorization role assigned to a user.
///
/// NOTE:
/// This enum is intentionally small.
/// Additional roles can be added later without
/// changing the authentication architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UserRole {
    #[default]
    User,
    Admin,
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserRole::User => write!(f, "user"),
            UserRole::Admin => write!(f, "admin"),
        }
    }
}
