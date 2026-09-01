diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/api/src/handlers/create_api_key.rs work/crates/api/src/handlers/create_api_key.rs
--- orig/crates/api/src/handlers/create_api_key.rs	2026-08-27 15:16:09.000000000 +0000
+++ work/crates/api/src/handlers/create_api_key.rs	2026-09-01 22:32:05.730146186 +0000
@@ -48,7 +48,9 @@
         )
         .await
         .map_err(|err| match err {
-            CreateApiKeyError::Forbidden => ApiError::forbidden(err.to_string()),
+            CreateApiKeyError::Forbidden | CreateApiKeyError::ScopeExceedsPermissions(_) => {
+                ApiError::forbidden(err.to_string())
+            }
             CreateApiKeyError::EmptyName
             | CreateApiKeyError::NameTooLong
             | CreateApiKeyError::EmptyScopes
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/api/src/handlers/support.rs work/crates/api/src/handlers/support.rs
--- orig/crates/api/src/handlers/support.rs	2026-08-27 22:18:01.000000000 +0000
+++ work/crates/api/src/handlers/support.rs	2026-09-01 22:30:52.510104426 +0000
@@ -42,14 +42,10 @@
 
     let use_case = CreateSupportTicketUseCase::new(state.support_ticket_repository.clone());
     let ticket = use_case
-        .execute(
-            authenticated_user.tenant_context().tenant_id(),
-            authenticated_user.user_id(),
-            payload.subject,
-            category,
-        )
+        .execute(authenticated_user, payload.subject, category)
         .await
         .map_err(|err| match err {
+            CreateSupportTicketError::Forbidden => ApiError::forbidden(err.to_string()),
             CreateSupportTicketError::Validation(msg) => ApiError::bad_request(msg),
             CreateSupportTicketError::Other(err) => {
                 ApiError::internal_with_source("failed to create support ticket", err)
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/ports/mod.rs work/crates/application/src/ports/mod.rs
--- orig/crates/application/src/ports/mod.rs	2026-09-01 21:46:07.000000000 +0000
+++ work/crates/application/src/ports/mod.rs	2026-09-01 22:26:27.546534246 +0000
@@ -91,7 +91,7 @@
 // both the API (get_public_status) and the worker (IncidentManager).
 pub use status_page_cache::{CacheResult, PUBLIC_STATUS_LIST_CACHE_KEY, StatusPageCache};
 pub use status_page_repository::StatusPageRepository;
-pub use tenant_membership_repository::TenantMembershipRepository;
+pub use tenant_membership_repository::{LastOwnerGuardOutcome, TenantMembershipRepository};
 pub use tenant_repository::TenantRepository;
 
 pub mod logo_storage;
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/ports/tenant_membership_repository.rs work/crates/application/src/ports/tenant_membership_repository.rs
--- orig/crates/application/src/ports/tenant_membership_repository.rs	2026-08-27 15:16:09.000000000 +0000
+++ work/crates/application/src/ports/tenant_membership_repository.rs	2026-09-01 22:26:09.922603969 +0000
@@ -12,6 +12,25 @@
 
 type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
 
+/// Outcome of a "last owner" guarded mutation (`remove_member_if_not_last_owner`,
+/// `update_role_if_not_last_owner`).
+///
+/// These operations must check-and-act atomically (locking the tenant's
+/// membership rows for the duration of the check) so that two concurrent
+/// requests can never both observe "owners > 1" and both proceed, which
+/// would leave the tenant with zero Owners. See the 2026-09 security
+/// review: SEC-P0-4 (Last Owner Race Condition).
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum LastOwnerGuardOutcome {
+    /// The mutation was applied.
+    Applied,
+    /// No membership row existed for that tenant/user pair.
+    NotFound,
+    /// The mutation was refused because it would have left the tenant
+    /// with zero Owners.
+    WouldRemoveLastOwner,
+}
+
 /// Persistence port for `TenantMembership`.
 ///
 /// `find_by_tenant_and_user` is deliberately first-class here because
@@ -41,6 +60,34 @@
     /// is left to the caller; this method performs a `DELETE ... WHERE ...`
     /// and does not error when 0 rows were affected.
     async fn delete(&self, tenant_id: TenantId, user_id: UserId) -> RepoResult<()>;
+
+    /// Atomically deletes `user_id`'s membership in `tenant_id`, refusing
+    /// if — and only if — the target is an `Owner` and is the tenant's
+    /// last remaining `Owner`.
+    ///
+    /// The check (counting current Owners) and the act (deleting the row)
+    /// MUST happen under the same row lock / transaction so that two
+    /// concurrent calls targeting two different Owners can never both
+    /// succeed and leave the tenant ownerless. Implementations backed by
+    /// Postgres should lock the tenant's membership rows (e.g. `SELECT
+    /// ... FOR UPDATE`) before counting.
+    async fn remove_member_if_not_last_owner(
+        &self,
+        tenant_id: TenantId,
+        user_id: UserId,
+    ) -> RepoResult<LastOwnerGuardOutcome>;
+
+    /// Atomically changes `user_id`'s role to `new_role` in `tenant_id`,
+    /// refusing if — and only if — the target is currently the tenant's
+    /// last remaining `Owner` and `new_role` is not `Owner`.
+    ///
+    /// Same atomicity requirement as `remove_member_if_not_last_owner`.
+    async fn update_role_if_not_last_owner(
+        &self,
+        tenant_id: TenantId,
+        user_id: UserId,
+        new_role: Role,
+    ) -> RepoResult<LastOwnerGuardOutcome>;
 }
 
 #[async_trait]
@@ -80,4 +127,25 @@
     async fn delete(&self, tenant_id: TenantId, user_id: UserId) -> RepoResult<()> {
         (**self).delete(tenant_id, user_id).await
     }
+
+    async fn remove_member_if_not_last_owner(
+        &self,
+        tenant_id: TenantId,
+        user_id: UserId,
+    ) -> RepoResult<LastOwnerGuardOutcome> {
+        (**self)
+            .remove_member_if_not_last_owner(tenant_id, user_id)
+            .await
+    }
+
+    async fn update_role_if_not_last_owner(
+        &self,
+        tenant_id: TenantId,
+        user_id: UserId,
+        new_role: Role,
+    ) -> RepoResult<LastOwnerGuardOutcome> {
+        (**self)
+            .update_role_if_not_last_owner(tenant_id, user_id, new_role)
+            .await
+    }
 }
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/use_cases/change_member_role.rs work/crates/application/src/use_cases/change_member_role.rs
--- orig/crates/application/src/use_cases/change_member_role.rs	2026-08-27 15:16:09.000000000 +0000
+++ work/crates/application/src/use_cases/change_member_role.rs	2026-09-01 22:27:33.322634637 +0000
@@ -5,7 +5,7 @@
     types::{AuthenticatedUser, Permission, Role},
 };
 
-use crate::ports::TenantMembershipRepository;
+use crate::ports::{LastOwnerGuardOutcome, TenantMembershipRepository};
 
 use super::membership_common::MembershipError;
 
@@ -67,29 +67,26 @@
             return Err(MembershipError::Forbidden);
         }
 
-        // Last-owner guard for self-demotion
-        if actor.user_id() == command.user_id
-            && target.role == Role::Owner
-            && command.new_role != Role::Owner
+        // The last-Owner guard applies whenever an Owner is being moved
+        // to a non-Owner role — not only on self-demotion. (An Owner
+        // demoting a *peer* Owner is just as capable of leaving the
+        // tenant ownerless as self-demotion is, once two such calls race
+        // each other.) The check and the write happen atomically in the
+        // repository (row-locked transaction for Postgres, single mutex
+        // critical section in-memory), which is what actually closes the
+        // race: two concurrent calls can no longer both observe
+        // "owners > 1" and both proceed.
+        match self
+            .tenant_membership_repository
+            .update_role_if_not_last_owner(tenant_id, command.user_id, command.new_role)
+            .await
+            .map_err(MembershipError::Other)?
         {
-            let owners = self
-                .tenant_membership_repository
-                .list_by_tenant(tenant_id)
-                .await
-                .map_err(MembershipError::Other)?
-                .into_iter()
-                .filter(|m| m.role == Role::Owner)
-                .count();
-            if owners <= 1 {
-                return Err(MembershipError::InvalidOperation(
-                    "cannot demote the last Owner".to_string(),
-                ));
-            }
+            LastOwnerGuardOutcome::Applied => Ok(()),
+            LastOwnerGuardOutcome::NotFound => Err(MembershipError::NotFound),
+            LastOwnerGuardOutcome::WouldRemoveLastOwner => Err(MembershipError::InvalidOperation(
+                "cannot demote the last Owner".to_string(),
+            )),
         }
-
-        self.tenant_membership_repository
-            .update_role(tenant_id, command.user_id, command.new_role)
-            .await
-            .map_err(MembershipError::Other)
     }
 }
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/use_cases/create_api_key.rs work/crates/application/src/use_cases/create_api_key.rs
--- orig/crates/application/src/use_cases/create_api_key.rs	2026-08-27 15:16:09.000000000 +0000
+++ work/crates/application/src/use_cases/create_api_key.rs	2026-09-01 22:32:02.530749618 +0000
@@ -4,6 +4,7 @@
 use domain::{
     AuditAction, AuditLogEntry, AuditLogEntryId,
     entities::{ApiKey, CreateApiKey as CreateApiKeyEntity},
+    enums::ApiKeyScope,
     shared::types::{AuthenticatedUser, Permission},
 };
 use serde_json::json;
@@ -34,6 +35,14 @@
     /// guarantee entirely. See the 2026-08-20 security review of
     /// Feature 10.
     Forbidden,
+    /// One or more requested scopes grant more than the caller's own
+    /// `Role` permits (e.g. a `Role::Member` — which lacks
+    /// `ManageApiKeys` and so can't even reach this far — or, more
+    /// realistically, an `Admin` requesting a scope this codebase later
+    /// gates behind an Owner-only permission). Kept distinct from
+    /// `Forbidden` so the caller gets an actionable message naming the
+    /// offending scope rather than a generic "not allowed".
+    ScopeExceedsPermissions(ApiKeyScope),
     EmptyName,
     NameTooLong,
     EmptyScopes,
@@ -48,6 +57,10 @@
                 f,
                 "you do not have permission to create api keys for this tenant"
             ),
+            Self::ScopeExceedsPermissions(scope) => write!(
+                f,
+                "requested scope '{scope}' exceeds your own role's permissions"
+            ),
             Self::EmptyName => write!(f, "api key name cannot be empty"),
             Self::NameTooLong => write!(
                 f,
@@ -130,6 +143,18 @@
             return Err(CreateApiKeyError::Forbidden);
         }
 
+        // Defense in depth for "an API key must never have more power
+        // than its creator": even though every scope that exists today
+        // maps to a permission already implied by `ManageApiKeys`
+        // (Admin/Owner), this check is what keeps that true automatically
+        // as new, more powerful scopes are added later — see
+        // `ApiKeyScope::required_permission`.
+        for scope in &command.scopes {
+            if !authenticated_user.can(scope.required_permission()) {
+                return Err(CreateApiKeyError::ScopeExceedsPermissions(*scope));
+            }
+        }
+
         let name = command.name.trim();
         if name.is_empty() {
             return Err(CreateApiKeyError::EmptyName);
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/use_cases/create_support_ticket.rs work/crates/application/src/use_cases/create_support_ticket.rs
--- orig/crates/application/src/use_cases/create_support_ticket.rs	2026-08-27 21:13:12.000000000 +0000
+++ work/crates/application/src/use_cases/create_support_ticket.rs	2026-09-01 22:30:47.910460549 +0000
@@ -3,7 +3,8 @@
 
 use domain::entities::{CreateSupportTicket, SupportTicket};
 use domain::enums::{SupportCategory, SupportTicketStatus};
-use domain::shared::ids::{SupportTicketId, TenantId, UserId};
+use domain::shared::ids::SupportTicketId;
+use domain::shared::types::{AuthenticatedUser, Permission};
 use uuid::Uuid;
 
 use crate::ports::SupportTicketRepository;
@@ -12,6 +13,11 @@
 
 #[derive(Debug)]
 pub enum CreateSupportTicketError {
+    /// Caller's role does not carry `Permission::CreateSupportTicket`
+    /// (currently only `Auditor`/`Viewer` lack it — both are meant to be
+    /// read-only). See P1 "Support Ticket Authorization" in the 2026-09
+    /// security review.
+    Forbidden,
     Validation(String),
     Other(Box<dyn Error + Send + Sync>),
 }
@@ -19,6 +25,7 @@
 impl fmt::Display for CreateSupportTicketError {
     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
         match self {
+            Self::Forbidden => write!(f, "you do not have permission to create support tickets"),
             Self::Validation(msg) => write!(f, "{msg}"),
             Self::Other(err) => write!(f, "{err}"),
         }
@@ -49,11 +56,17 @@
 
     pub async fn execute(
         &self,
-        tenant_id: TenantId,
-        created_by: UserId,
+        authenticated_user: AuthenticatedUser,
         subject: String,
         category: SupportCategory,
     ) -> Result<SupportTicket, CreateSupportTicketError> {
+        if !authenticated_user.can(Permission::CreateSupportTicket) {
+            return Err(CreateSupportTicketError::Forbidden);
+        }
+
+        let tenant_id = authenticated_user.tenant_context().tenant_id();
+        let created_by = authenticated_user.user_id();
+
         let ticket_id = SupportTicketId::new();
         let ticket_number = format!(
             "PV-{}",
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/use_cases/get_support_ticket.rs work/crates/application/src/use_cases/get_support_ticket.rs
--- orig/crates/application/src/use_cases/get_support_ticket.rs	2026-08-27 21:18:51.000000000 +0000
+++ work/crates/application/src/use_cases/get_support_ticket.rs	2026-09-01 22:31:02.810355066 +0000
@@ -3,7 +3,7 @@
 use domain::entities::{SupportMessage, SupportTicket};
 use domain::shared::ids::SupportTicketId;
 use domain::shared::policies::AuthorizationPolicy;
-use domain::shared::types::AuthenticatedUser;
+use domain::shared::types::{AuthenticatedUser, Permission};
 
 use crate::ports::SupportTicketRepository;
 
@@ -27,6 +27,10 @@
         id: SupportTicketId,
         authenticated_user: AuthenticatedUser,
     ) -> UseCaseResult<Option<(SupportTicket, Vec<SupportMessage>)>> {
+        if !authenticated_user.can(Permission::ViewSupportTickets) {
+            return Ok(None);
+        }
+
         let ticket = match self.repository.find_ticket(id).await? {
             Some(ticket) => ticket,
             None => return Ok(None),
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/use_cases/list_my_support_tickets.rs work/crates/application/src/use_cases/list_my_support_tickets.rs
--- orig/crates/application/src/use_cases/list_my_support_tickets.rs	2026-08-27 20:58:23.000000000 +0000
+++ work/crates/application/src/use_cases/list_my_support_tickets.rs	2026-09-01 22:31:22.658389729 +0000
@@ -1,7 +1,7 @@
 use std::error::Error;
 
 use domain::entities::SupportTicket;
-use domain::shared::types::AuthenticatedUser;
+use domain::shared::types::{AuthenticatedUser, Permission};
 
 use crate::ports::SupportTicketRepository;
 
@@ -24,6 +24,10 @@
         &self,
         authenticated_user: AuthenticatedUser,
     ) -> UseCaseResult<Vec<SupportTicket>> {
+        if !authenticated_user.can(Permission::ViewSupportTickets) {
+            return Ok(Vec::new());
+        }
+
         let tenant_id = authenticated_user.tenant_context().tenant_id();
         let user_id = authenticated_user.user_id();
         self.repository.list_user_tickets(tenant_id, user_id).await
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/use_cases/remove_member.rs work/crates/application/src/use_cases/remove_member.rs
--- orig/crates/application/src/use_cases/remove_member.rs	2026-08-27 15:16:09.000000000 +0000
+++ work/crates/application/src/use_cases/remove_member.rs	2026-09-01 22:27:07.991014923 +0000
@@ -5,7 +5,7 @@
     types::{AuthenticatedUser, Permission, Role},
 };
 
-use crate::ports::TenantMembershipRepository;
+use crate::ports::{LastOwnerGuardOutcome, TenantMembershipRepository};
 
 use super::membership_common::MembershipError;
 
@@ -66,26 +66,22 @@
             }
         }
 
-        // Guard against removing the last Owner.
-        if target.role == Role::Owner {
-            let owners = self
-                .tenant_membership_repository
-                .list_by_tenant(tenant_id)
-                .await
-                .map_err(MembershipError::Other)?
-                .into_iter()
-                .filter(|m| m.role == Role::Owner)
-                .count();
-            if owners <= 1 {
-                return Err(MembershipError::InvalidOperation(
-                    "cannot remove the last Owner".to_string(),
-                ));
-            }
-        }
-
-        self.tenant_membership_repository
-            .delete(tenant_id, target_user_id)
+        // Removal + the last-Owner guard happen atomically in the
+        // repository (row-locked transaction for Postgres, single mutex
+        // critical section in-memory) so that two concurrent removals of
+        // two different Owners can't both observe "owners > 1" and both
+        // proceed, which would leave the tenant with zero Owners.
+        match self
+            .tenant_membership_repository
+            .remove_member_if_not_last_owner(tenant_id, target_user_id)
             .await
-            .map_err(MembershipError::Other)
+            .map_err(MembershipError::Other)?
+        {
+            LastOwnerGuardOutcome::Applied => Ok(()),
+            LastOwnerGuardOutcome::NotFound => Err(MembershipError::NotFound),
+            LastOwnerGuardOutcome::WouldRemoveLastOwner => Err(MembershipError::InvalidOperation(
+                "cannot remove the last Owner".to_string(),
+            )),
+        }
     }
 }
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/use_cases/reply_to_support_ticket.rs work/crates/application/src/use_cases/reply_to_support_ticket.rs
--- orig/crates/application/src/use_cases/reply_to_support_ticket.rs	2026-08-27 21:18:51.000000000 +0000
+++ work/crates/application/src/use_cases/reply_to_support_ticket.rs	2026-09-01 22:31:08.491471004 +0000
@@ -3,7 +3,7 @@
 use domain::entities::{CreateSupportMessage, SupportMessage};
 use domain::shared::ids::{SupportMessageId, SupportTicketId};
 use domain::shared::policies::AuthorizationPolicy;
-use domain::shared::types::AuthenticatedUser;
+use domain::shared::types::{AuthenticatedUser, Permission};
 
 use crate::ports::SupportTicketRepository;
 
@@ -28,6 +28,10 @@
         authenticated_user: AuthenticatedUser,
         message: String,
     ) -> UseCaseResult<Option<SupportMessage>> {
+        if !authenticated_user.can(Permission::ReplySupportTicket) {
+            return Ok(None);
+        }
+
         let ticket = match self.repository.find_ticket(ticket_id).await? {
             Some(ticket) => ticket,
             None => return Ok(None),
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/application/src/use_cases/update_support_ticket_status.rs work/crates/application/src/use_cases/update_support_ticket_status.rs
--- orig/crates/application/src/use_cases/update_support_ticket_status.rs	2026-08-27 21:18:51.000000000 +0000
+++ work/crates/application/src/use_cases/update_support_ticket_status.rs	2026-09-01 22:31:14.235404635 +0000
@@ -3,7 +3,7 @@
 use domain::enums::SupportTicketStatus;
 use domain::shared::ids::SupportTicketId;
 use domain::shared::policies::AuthorizationPolicy;
-use domain::shared::types::AuthenticatedUser;
+use domain::shared::types::{AuthenticatedUser, Permission};
 
 use crate::ports::SupportTicketRepository;
 
@@ -28,6 +28,10 @@
         target_status: SupportTicketStatus,
         authenticated_user: AuthenticatedUser,
     ) -> UseCaseResult<Option<()>> {
+        if !authenticated_user.can(Permission::ManageSupportTicketStatus) {
+            return Ok(None);
+        }
+
         let ticket = match self.repository.find_ticket(ticket_id).await? {
             Some(ticket) => ticket,
             None => return Ok(None),
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/domain/src/enums/api_key_scope.rs work/crates/domain/src/enums/api_key_scope.rs
--- orig/crates/domain/src/enums/api_key_scope.rs	2026-08-27 15:16:09.000000000 +0000
+++ work/crates/domain/src/enums/api_key_scope.rs	2026-09-01 22:31:44.983078990 +0000
@@ -73,6 +73,36 @@
     pub fn from_str_opt(value: &str) -> Option<Self> {
         Self::ALL.iter().copied().find(|s| s.as_str() == value)
     }
+
+    /// The `Permission` a tenant member must hold on their own `Role` in
+    /// order to be allowed to mint a key carrying this scope.
+    ///
+    /// This is what makes `CreateApiKeyUseCase` enforce "an API key must
+    /// never have more power than its creator": every scope maps back to
+    /// the same coarse-grained permission that gates the equivalent
+    /// first-party action, so a role that can't perform an action itself
+    /// can't hand a key that grants it either. See P0/P1 "API Key
+    /// Authorization Parity" / "API Key Scope Escalation" in the 2026-09
+    /// security review.
+    ///
+    /// Deliberately exhaustive (no wildcard fallback) so that adding a
+    /// new `ApiKeyScope` variant without adding an arm here is a compile
+    /// error, not a silently-unguarded scope.
+    pub fn required_permission(&self) -> crate::shared::types::Permission {
+        use crate::shared::types::Permission;
+        match self {
+            Self::MonitorRead => Permission::ViewMonitor,
+            Self::MonitorWrite => Permission::UpdateMonitor,
+            Self::StatusPageRead => Permission::ViewMonitor,
+            Self::StatusPageWrite => Permission::UpdateMonitor,
+            Self::IncidentRead => Permission::ViewIncident,
+            Self::IncidentWrite => Permission::AcknowledgeIncident,
+            Self::NotificationRead => Permission::ViewMonitor,
+            Self::NotificationWrite => Permission::UpdateMonitor,
+            Self::ApiFlowRead => Permission::ViewMonitor,
+            Self::ApiFlowWrite => Permission::UpdateMonitor,
+        }
+    }
 }
 
 impl std::fmt::Display for ApiKeyScope {
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/domain/src/shared/types/permission.rs work/crates/domain/src/shared/types/permission.rs
--- orig/crates/domain/src/shared/types/permission.rs	2026-08-31 17:00:38.000000000 +0000
+++ work/crates/domain/src/shared/types/permission.rs	2026-09-01 22:29:26.024940964 +0000
@@ -30,4 +30,17 @@
 
     // Audit log actions
     ViewAuditLog,
+
+    // Support ticket actions
+    //
+    // Split into four permissions (view / create / reply / manage-status)
+    // rather than folded into `ManageUsers` because a tenant's read-only
+    // roles (`Auditor`, `Viewer`) should still be able to see the
+    // tenant's support conversations without being able to write to
+    // them, and status changes are a step further still — see the
+    // 2026-09 security review: P1 "Support Ticket Authorization".
+    ViewSupportTickets,
+    CreateSupportTicket,
+    ReplySupportTicket,
+    ManageSupportTicketStatus,
 }
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/domain/src/shared/types/role.rs work/crates/domain/src/shared/types/role.rs
--- orig/crates/domain/src/shared/types/role.rs	2026-08-31 17:00:38.000000000 +0000
+++ work/crates/domain/src/shared/types/role.rs	2026-09-01 22:29:49.634979408 +0000
@@ -27,6 +27,9 @@
                 ChangeOwnPassword,
                 ViewMonitor,
                 ViewIncident,
+                ViewSupportTickets,
+                CreateSupportTicket,
+                ReplySupportTicket,
             ],
             Role::Admin => &[
                 ViewOwnProfile,
@@ -44,6 +47,10 @@
                 UpdateTenantSettings,
                 ManageUsers,
                 ManageApiKeys,
+                ViewSupportTickets,
+                CreateSupportTicket,
+                ReplySupportTicket,
+                ManageSupportTicketStatus,
             ],
             Role::Owner => &[
                 ViewOwnProfile,
@@ -62,14 +69,19 @@
                 ManageUsers,
                 ManageApiKeys,
                 ManageBilling,
+                ViewSupportTickets,
+                CreateSupportTicket,
+                ReplySupportTicket,
+                ManageSupportTicketStatus,
             ],
             Role::Auditor => &[
                 ViewOwnProfile,
                 ViewMonitor,
                 ViewIncident,
                 ViewTenantSettings,
+                ViewSupportTickets,
             ],
-            Role::Viewer => &[ViewOwnProfile, ViewMonitor, ViewIncident],
+            Role::Viewer => &[ViewOwnProfile, ViewMonitor, ViewIncident, ViewSupportTickets],
         }
     }
 
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/infra/src/repositories/in_memory_tenant_membership_repository.rs work/crates/infra/src/repositories/in_memory_tenant_membership_repository.rs
--- orig/crates/infra/src/repositories/in_memory_tenant_membership_repository.rs	2026-08-27 15:16:09.000000000 +0000
+++ work/crates/infra/src/repositories/in_memory_tenant_membership_repository.rs	2026-09-01 22:26:52.580810304 +0000
@@ -2,7 +2,7 @@
 use std::error::Error;
 use std::sync::{Arc, Mutex};
 
-use application::ports::TenantMembershipRepository;
+use application::ports::{LastOwnerGuardOutcome, TenantMembershipRepository};
 use async_trait::async_trait;
 use chrono::Utc;
 use domain::TenantMembership;
@@ -124,4 +124,59 @@
             .remove(&(tenant_id, user_id));
         Ok(())
     }
+
+    async fn remove_member_if_not_last_owner(
+        &self,
+        tenant_id: TenantId,
+        user_id: UserId,
+    ) -> Result<LastOwnerGuardOutcome, Box<dyn Error + Send + Sync>> {
+        // A single `std::sync::Mutex` guard held across the check-then-act
+        // below makes this atomic with respect to any other call on this
+        // repository instance — no `.await` happens while the lock is
+        // held, so no other task can interleave.
+        let mut map = self.memberships.lock().unwrap();
+        let Some(target) = map.get(&(tenant_id, user_id)) else {
+            return Ok(LastOwnerGuardOutcome::NotFound);
+        };
+
+        if target.role == Role::Owner {
+            let owner_count = map
+                .values()
+                .filter(|m| m.tenant_id == tenant_id && m.role == Role::Owner)
+                .count();
+            if owner_count <= 1 {
+                return Ok(LastOwnerGuardOutcome::WouldRemoveLastOwner);
+            }
+        }
+
+        map.remove(&(tenant_id, user_id));
+        Ok(LastOwnerGuardOutcome::Applied)
+    }
+
+    async fn update_role_if_not_last_owner(
+        &self,
+        tenant_id: TenantId,
+        user_id: UserId,
+        new_role: Role,
+    ) -> Result<LastOwnerGuardOutcome, Box<dyn Error + Send + Sync>> {
+        let mut map = self.memberships.lock().unwrap();
+        let Some(target) = map.get(&(tenant_id, user_id)) else {
+            return Ok(LastOwnerGuardOutcome::NotFound);
+        };
+
+        if target.role == Role::Owner && new_role != Role::Owner {
+            let owner_count = map
+                .values()
+                .filter(|m| m.tenant_id == tenant_id && m.role == Role::Owner)
+                .count();
+            if owner_count <= 1 {
+                return Ok(LastOwnerGuardOutcome::WouldRemoveLastOwner);
+            }
+        }
+
+        if let Some(membership) = map.get_mut(&(tenant_id, user_id)) {
+            membership.role = new_role;
+        }
+        Ok(LastOwnerGuardOutcome::Applied)
+    }
 }
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/crates/infra/src/repositories/postgres_tenant_membership_repository.rs work/crates/infra/src/repositories/postgres_tenant_membership_repository.rs
--- orig/crates/infra/src/repositories/postgres_tenant_membership_repository.rs	2026-08-27 15:16:09.000000000 +0000
+++ work/crates/infra/src/repositories/postgres_tenant_membership_repository.rs	2026-09-01 22:26:41.013350168 +0000
@@ -1,6 +1,6 @@
 use std::error::Error;
 
-use application::ports::TenantMembershipRepository;
+use application::ports::{LastOwnerGuardOutcome, TenantMembershipRepository};
 use async_trait::async_trait;
 use chrono::{DateTime, Utc};
 use domain::{
@@ -163,4 +163,92 @@
             .await?;
         Ok(())
     }
+
+    async fn remove_member_if_not_last_owner(
+        &self,
+        tenant_id: TenantId,
+        user_id: UserId,
+    ) -> RepoResult<LastOwnerGuardOutcome> {
+        let mut tx = self.pool.begin().await?;
+
+        // Lock EVERY membership row for this tenant for the duration of
+        // the transaction. This serializes concurrent calls to
+        // `remove_member_if_not_last_owner` / `update_role_if_not_last_owner`
+        // against each other (they all take the same row locks), which is
+        // exactly what closes the last-owner race: a second concurrent
+        // caller blocks here until the first transaction commits or rolls
+        // back, and then re-reads the post-commit state.
+        let rows = sqlx::query_as::<_, TenantMembershipRow>(
+            "SELECT tenant_id, user_id, role, invited_at, joined_at \
+             FROM tenant_memberships WHERE tenant_id = $1 FOR UPDATE",
+        )
+        .bind(tenant_id.0)
+        .fetch_all(&mut *tx)
+        .await?;
+
+        let Some(target) = rows.iter().find(|r| r.user_id == user_id.0) else {
+            return Ok(LastOwnerGuardOutcome::NotFound);
+        };
+
+        if target.role == "Owner" {
+            let owner_count = rows.iter().filter(|r| r.role == "Owner").count();
+            if owner_count <= 1 {
+                return Ok(LastOwnerGuardOutcome::WouldRemoveLastOwner);
+            }
+        }
+
+        sqlx::query("DELETE FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2")
+            .bind(tenant_id.0)
+            .bind(user_id.0)
+            .execute(&mut *tx)
+            .await?;
+
+        tx.commit().await?;
+        Ok(LastOwnerGuardOutcome::Applied)
+    }
+
+    async fn update_role_if_not_last_owner(
+        &self,
+        tenant_id: TenantId,
+        user_id: UserId,
+        new_role: Role,
+    ) -> RepoResult<LastOwnerGuardOutcome> {
+        let mut tx = self.pool.begin().await?;
+
+        // Same row-locking strategy as `remove_member_if_not_last_owner`
+        // — see comment there. Locking here matters just as much: a
+        // self-demotion (`update_role`) racing a peer Owner's
+        // self-demotion is the exact scenario that previously allowed a
+        // tenant to end up with zero Owners.
+        let rows = sqlx::query_as::<_, TenantMembershipRow>(
+            "SELECT tenant_id, user_id, role, invited_at, joined_at \
+             FROM tenant_memberships WHERE tenant_id = $1 FOR UPDATE",
+        )
+        .bind(tenant_id.0)
+        .fetch_all(&mut *tx)
+        .await?;
+
+        let Some(target) = rows.iter().find(|r| r.user_id == user_id.0) else {
+            return Ok(LastOwnerGuardOutcome::NotFound);
+        };
+
+        if target.role == "Owner" && new_role != Role::Owner {
+            let owner_count = rows.iter().filter(|r| r.role == "Owner").count();
+            if owner_count <= 1 {
+                return Ok(LastOwnerGuardOutcome::WouldRemoveLastOwner);
+            }
+        }
+
+        sqlx::query(
+            "UPDATE tenant_memberships SET role = $3 WHERE tenant_id = $1 AND user_id = $2",
+        )
+        .bind(tenant_id.0)
+        .bind(user_id.0)
+        .bind(role_to_db(&new_role))
+        .execute(&mut *tx)
+        .await?;
+
+        tx.commit().await?;
+        Ok(LastOwnerGuardOutcome::Applied)
+    }
 }
diff -ruN '--exclude=geoip.tar.gz' '--exclude=data' orig/migrations/0065_enforce_tenant_integrity_on_monitor_children.sql work/migrations/0065_enforce_tenant_integrity_on_monitor_children.sql
--- orig/migrations/0065_enforce_tenant_integrity_on_monitor_children.sql	1970-01-01 00:00:00.000000000 +0000
+++ work/migrations/0065_enforce_tenant_integrity_on_monitor_children.sql	2026-09-01 22:35:01.228122597 +0000
@@ -0,0 +1,93 @@
+-- SEC-P0-3 (2026-09 security review): "Cross-Tenant Parent/Child
+-- Relationships". A plain `monitor_id UUID REFERENCES monitors(id)`
+-- foreign key only guarantees the referenced monitor exists — it says
+-- nothing about which tenant it belongs to. Every write path already
+-- checks `monitor.tenant_id() == actor's tenant_id` in the application
+-- layer (see CreateApiFlowUseCase, CreateMaintenanceWindowUseCase,
+-- CreateEscalationPolicyUseCase, CreateNotificationChannelUseCase), but
+-- that leaves this invariant enforced in exactly one place: a single
+-- missed check in any current or future write path is a silent
+-- cross-tenant data leak with no database-level backstop.
+--
+-- This migration makes the invariant self-enforcing at the schema level:
+-- a child row's tenant_id must always match its parent monitor's
+-- tenant_id, and Postgres — not application code — is what guarantees
+-- it from here on.
+--
+-- If this migration fails with a foreign key violation, it means
+-- inconsistent cross-tenant rows already exist in the target
+-- environment; that data must be investigated and repaired before this
+-- migration can be applied there.
+
+-- A composite foreign key must reference a UNIQUE (or PK) constraint on
+-- the parent columns. `monitors.id` is already the primary key, so this
+-- is a redundant-but-required superset unique constraint that makes
+-- (id, tenant_id) referenceable.
+ALTER TABLE monitors
+    ADD CONSTRAINT uq_monitors_id_tenant_id UNIQUE (id, tenant_id);
+
+-- ---------------------------------------------------------------------
+-- api_flows
+-- ---------------------------------------------------------------------
+ALTER TABLE api_flows
+    DROP CONSTRAINT fk_api_flows_monitor;
+
+ALTER TABLE api_flows
+    ADD CONSTRAINT fk_api_flows_monitor_tenant
+    FOREIGN KEY (monitor_id, tenant_id)
+    REFERENCES monitors (id, tenant_id)
+    ON DELETE CASCADE;
+
+-- ---------------------------------------------------------------------
+-- maintenance_windows
+-- ---------------------------------------------------------------------
+ALTER TABLE maintenance_windows
+    DROP CONSTRAINT maintenance_windows_monitor_id_fkey;
+
+ALTER TABLE maintenance_windows
+    ADD CONSTRAINT fk_maintenance_windows_monitor_tenant
+    FOREIGN KEY (monitor_id, tenant_id)
+    REFERENCES monitors (id, tenant_id)
+    ON DELETE CASCADE;
+
+-- ---------------------------------------------------------------------
+-- escalation_policies
+--
+-- monitor_id is nullable here (a NULL monitor_id is a tenant-global
+-- policy, not tied to any single monitor). A composite FK with
+-- MATCH SIMPLE (Postgres's default match type) is automatically
+-- satisfied whenever ANY referencing column is NULL, so global policies
+-- are correctly left unconstrained by this FK while monitor-scoped
+-- policies are still checked.
+-- ---------------------------------------------------------------------
+ALTER TABLE escalation_policies
+    ADD CONSTRAINT fk_escalation_policies_monitor_tenant
+    FOREIGN KEY (monitor_id, tenant_id)
+    REFERENCES monitors (id, tenant_id)
+    ON DELETE CASCADE;
+
+-- ---------------------------------------------------------------------
+-- notification_channels
+-- ---------------------------------------------------------------------
+ALTER TABLE notification_channels
+    DROP CONSTRAINT notification_channels_monitor_id_fkey;
+
+ALTER TABLE notification_channels
+    ADD CONSTRAINT fk_notification_channels_monitor_tenant
+    FOREIGN KEY (monitor_id, tenant_id)
+    REFERENCES monitors (id, tenant_id)
+    ON DELETE CASCADE;
+
+-- ---------------------------------------------------------------------
+-- NOTE for follow-up (not done in this migration):
+--
+-- `incidents` and `diagnostic_reports` currently have NO tenant_id
+-- column of their own at all — tenant scoping for them is resolved
+-- entirely by joining through `monitors`/`checks`. Adding a composite FK
+-- there requires first backfilling a `tenant_id` column on those tables,
+-- which is a larger, higher-risk migration (large/partitioned tables)
+-- that deserves its own review and a maintenance window, so it is
+-- deliberately NOT bundled into this fix. Until that lands, continue to
+-- rely on the existing application-layer `monitor.tenant_id() ==
+-- actor_tenant_id` checks for those two tables specifically.
+-- ---------------------------------------------------------------------
