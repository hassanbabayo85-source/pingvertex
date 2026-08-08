use super::permission::Permission;

/// Coarse-grained role for authorization.
///
/// Roles are mapped to permissions in application policy code.
/// Keep the mapping explicit and testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Member,
    Admin,
    Owner,
    Auditor,
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
        }
    }

    pub fn has_permission(self, permission: Permission) -> bool {
        self.permissions().contains(&permission)
    }
}
