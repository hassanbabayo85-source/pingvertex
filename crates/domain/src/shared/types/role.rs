use serde::{Deserialize, Serialize};

use super::permission::Permission;

/// Coarse-grained role for authorization.
///
/// Roles are mapped to permissions in application policy code.
/// Keep the mapping explicit and testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Role {
    Operator,
    Admin,
    Owner,
    Auditor,
    Viewer,
}

impl Role {
    pub fn permissions(self) -> &'static [Permission] {
        use Permission::*;

        match self {
            Role::Viewer => &[
                ViewOwnProfile,
                UpdateOwnProfile,
                ChangeOwnPassword,
                ViewMonitor,
                ViewIncident,
                ViewSupportTickets,
            ],
            Role::Operator => &[
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
                ViewSupportTickets,
                CreateSupportTicket,
                ReplySupportTicket,
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
                ViewSupportTickets,
                CreateSupportTicket,
                ReplySupportTicket,
                ManageSupportTicketStatus,
                ViewBilling,
                // Note: ManageBilling is explicitly excluded by product policy.
                // Admin must NOT manage billing semantics ambiguously.
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
                ViewBilling,
                ViewAuditLog,
                ViewSupportTickets,
                CreateSupportTicket,
                ReplySupportTicket,
                ManageSupportTicketStatus,
            ],
            Role::Auditor => &[
                ViewOwnProfile,
                ViewBilling,
                ViewMonitor,
                ViewIncident,
                ViewTenantSettings,
                ViewSupportTickets,
                ViewAuditLog,
            ],
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
    fn owner_has_full_authority() {
        let p = Role::Owner.permissions();
        assert!(p.contains(&Permission::CreateMonitor));
        assert!(p.contains(&Permission::ManageBilling));
        assert!(p.contains(&Permission::ManageUsers));
        assert!(p.contains(&Permission::UpdateTenantSettings));
    }

    #[test]
    fn admin_has_admin_operations_but_no_billing() {
        let p = Role::Admin.permissions();
        assert!(p.contains(&Permission::CreateMonitor));
        assert!(p.contains(&Permission::ManageUsers));
        assert!(p.contains(&Permission::UpdateTenantSettings));
        assert!(!p.contains(&Permission::ManageBilling)); // Explicitly disabled
    }

    #[test]
    fn operator_has_monitor_operations_only() {
        let p = Role::Operator.permissions();
        assert!(p.contains(&Permission::CreateMonitor));
        assert!(p.contains(&Permission::UpdateMonitor));
        assert!(p.contains(&Permission::ViewMonitor));
        assert!(!p.contains(&Permission::ManageUsers));
        assert!(!p.contains(&Permission::ManageBilling));
        assert!(!p.contains(&Permission::UpdateTenantSettings));
    }

    #[test]
    fn viewer_is_read_only() {
        let p = Role::Viewer.permissions();
        assert!(p.contains(&Permission::ViewMonitor));
        assert!(!p.contains(&Permission::CreateMonitor));
        assert!(!p.contains(&Permission::UpdateMonitor));
        assert!(!p.contains(&Permission::ManageUsers));
        assert!(!p.contains(&Permission::ManageBilling));
        assert!(!p.contains(&Permission::UpdateTenantSettings));
    }
}
