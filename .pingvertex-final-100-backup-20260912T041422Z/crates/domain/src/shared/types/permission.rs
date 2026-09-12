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
    ViewBilling,
    ManageBilling,

    // Audit log actions
    ViewAuditLog,

    // Support ticket actions
    //
    // Split into four permissions (view / create / reply / manage-status)
    // rather than folded into `ManageUsers` because a tenant's read-only
    // roles (`Auditor`, `Viewer`) should still be able to see the
    // tenant's support conversations without being able to write to
    // them, and status changes are a step further still — see the
    // 2026-09 security review: P1 "Support Ticket Authorization".
    ViewSupportTickets,
    CreateSupportTicket,
    ReplySupportTicket,
    ManageSupportTicketStatus,
}
