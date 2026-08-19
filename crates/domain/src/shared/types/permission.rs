/// Fine-grained permission used by authorization policies.
///
/// Keep this list stable and additive. Avoid removing or reusing
/// permission names once they are shipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    // User/account actions
    ViewOwnProfile,
    UpdateOwnProfile,
    ChangeOwnPassword,

    // Monitor actions
    ViewMonitor,
    CreateMonitor,
    UpdateMonitor,
    DeleteMonitor,
    PauseMonitor,

    // Incident actions
    ViewIncident,
    AcknowledgeIncident,
    ResolveIncident,

    // Tenant/admin actions
    ViewTenantSettings,
    UpdateTenantSettings,
    ManageUsers,
    ManageApiKeys,
    ManageBilling,

    // Audit log actions
    ViewAuditLog,
}
