use serde::{Deserialize, Serialize};

use super::permission::Permission;

/// Coarse-grained role for authorization.
///
/// Roles are mapped to permissions in application policy code.
/// Keep the mapping explicit and testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Role {
    Member,
    Admin,
    Owner,
    Auditor,
    Viewer,
}

impl Role {
    pub fn permissions(self) -> &'static [Permission] {
        use Permission::*;

        match self {
            Role::Member => &[
                ViewOwnProfile,
                UpdateOwnProfile,
                ChangeOwnPassword,
                ViewMonitor,
                ViewIncident,
            ],
            Role::Admin => &[
                ViewOwnProfile,
                UpdateOwnProfile,
                ChangeOwnPassword,
                ViewMonitor,
                CreateMonitor,
                UpdateMonitor,
                DeleteMonitor,
                PauseMonitor,
                ViewIncident,
                AcknowledgeIncident,
                ResolveIncident,
                ViewTenantSettings,
                UpdateTenantSettings,
                ManageUsers,
                ManageApiKeys,
            ],
            Role::Owner => &[
                ViewOwnProfile,
                UpdateOwnProfile,
                ChangeOwnPassword,
                ViewMonitor,
                CreateMonitor,
                UpdateMonitor,
                DeleteMonitor,
                PauseMonitor,
                ViewIncident,
                AcknowledgeIncident,
                ResolveIncident,
                ViewTenantSettings,
                UpdateTenantSettings,
                ManageUsers,
                ManageApiKeys,
                ManageBilling,
            ],
            Role::Auditor => &[
                ViewOwnProfile,
                ViewMonitor,
                ViewIncident,
                ViewTenantSettings,
            ],
            Role::Viewer => &[ViewOwnProfile, ViewMonitor, ViewIncident],
        }
    }

    pub fn has_permission(self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_has_only_read_permissions() {
        let perms = Role::Viewer.permissions();
        assert!(perms.contains(&Permission::ViewOwnProfile));
        assert!(perms.contains(&Permission::ViewMonitor));
        assert!(perms.contains(&Permission::ViewIncident));
        assert!(!perms.contains(&Permission::CreateMonitor));
        assert!(!perms.contains(&Permission::UpdateMonitor));
        assert!(!perms.contains(&Permission::DeleteMonitor));
        assert!(!perms.contains(&Permission::ManageUsers));
        assert!(!perms.contains(&Permission::ManageApiKeys));
    }

    #[test]
    fn admin_can_manage_users() {
        assert!(Role::Admin.has_permission(Permission::ManageUsers));
    }

    #[test]
    fn owner_can_manage_users_and_billing() {
        assert!(Role::Owner.has_permission(Permission::ManageUsers));
        assert!(Role::Owner.has_permission(Permission::ManageBilling));
    }

    #[test]
    fn member_does_not_have_manage_users() {
        assert!(!Role::Member.has_permission(Permission::ManageUsers));
    }
}
