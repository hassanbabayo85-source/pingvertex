use serde::{Deserialize, Serialize};

/// Actions that can be recorded in the audit log.
///
/// Keep this list stable and additive. Avoid removing or reusing
/// variant names once they are shipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditAction {
    MonitorCreated,
    MonitorUpdated,
    MonitorDeleted,
    ApiFlowCreated,
    ApiFlowUpdated,
    ApiFlowDeleted,
    ProxyCheckJobCreated,
    NotificationChannelCreated,
    NotificationChannelDeleted,
    StatusPageCreated,
    StatusPageUpdated,
    StatusPageDeleted,
    EscalationPolicyCreated,
    MaintenanceWindowCreated,
    IncidentAcknowledged,
    CustomDomainRequested,
    CustomDomainRemoved,
    MemberInvited,
    MemberRemoved,
    RoleChanged,
    ApiKeyCreated,
    ApiKeyDeleted,
    PlatformCompanySuspended,
    PlatformCompanyActivated,
    PlatformUserSuspended,
    PlatformUserActivated,
    PlatformWalletAdjusted,
    PlatformPromotionalCreditGranted,
    BulkMonitorEnabled,
    BulkMonitorDisabled,
    BulkMonitorPaused,
    BulkMonitorResumed,
    BulkImportPaused,
    BulkImportResumed,
}
