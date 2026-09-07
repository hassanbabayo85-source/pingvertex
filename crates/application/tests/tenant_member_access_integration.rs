mod common;
use common::*;

use std::sync::Arc;

use application::{
    commands::{
        CreateNotificationChannel, DeleteMonitor, DeleteNotificationChannel, RequestCustomDomain,
        UpdateMonitor, UpdateStatusPage,
    },
    dto::{CreateMonitorRequest, CreateStatusPageRequest},
    ports::{NotificationService, NotifyResult, SecretCipher, SecretCipherError},
    queries::GetMonitor,
    use_cases::{
        CreateMonitorUseCase, CreateNotificationChannelUseCase, CreateStatusPageUseCase,
        DeleteMonitorUseCase, DeleteNotificationChannelUseCase, DeleteStatusPageUseCase,
        GetMonitorUseCase, ListNotificationChannelsUseCase, ListStatusPagesUseCase,
        RequestCustomDomainUseCase, SendTestNotificationUseCase, UpdateMonitorUseCase,
        UpdateStatusPageUseCase, request_custom_domain::RequestCustomDomainError,
        send_test_notification::SendTestNotificationError,
        update_status_page::UpdateStatusPageError,
    },
};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel,
    enums::{HttpMethod, MonitorKind, NotificationChannelKind},
    notification::NotificationEvent,
    shared::ids::{TenantId, UserId},
    shared::types::{AuthenticatedUser, PermissionDenied, Role, TenantContext},
};
use infra::{
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository, InMemoryStatusPageRepository,
    UuidIdGenerator,
};
use uuid::Uuid;

/// `Role::Member` is intentionally read-only tenant-wide (see `role.rs`).
/// Per-resource ACLs that would let an Admin grant a specific Member
/// read+write on a *specific* monitor are a planned follow-up (the
/// "755/600"-style grants) and don't exist yet — until then, Member means
/// read-only, full stop, even for resources in the member's own tenant.
fn auth_member(user_id: UserId, tenant_id: TenantId) -> AuthenticatedUser {
    AuthenticatedUser::new(
        user_id,
        Role::Member,
        TenantContext::with_actor(tenant_id, user_id),
    )
}

fn auth_owner(user_id: UserId, tenant_id: TenantId) -> AuthenticatedUser {
    AuthenticatedUser::new(
        user_id,
        Role::Owner,
        TenantContext::with_actor(tenant_id, user_id),
    )
}

fn monitor_request(name: &str) -> CreateMonitorRequest {
    CreateMonitorRequest {
        name: name.to_string(),
        url: "https://example.com".to_string(),
        method: HttpMethod::Get,
        expected_status: 200,
        interval_seconds: 300,
        timeout_seconds: 30,
        assertions: vec![],
        is_public: false,
        ssl_check_enabled: false,
        ssl_expiry_warning_days: 30,
        kind: MonitorKind::Http,
        tcp_port: None,
        dns_record_type: None,
        dns_expected_value: None,
        heartbeat_interval_seconds: None,
        heartbeat_grace_period_seconds: None,
    }
}

#[derive(Clone)]
struct StubSecretCipher;

impl SecretCipher for StubSecretCipher {
    fn encrypt_bytes(&self, _plaintext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Ok(vec![])
    }
    fn decrypt_bytes(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, SecretCipherError> {
        Ok(vec![])
    }
}

struct StubNotificationService;

#[async_trait]
impl NotificationService for StubNotificationService {
    async fn notify(
        &self,
        _channel: &NotificationChannel,
        _event: &NotificationEvent,
    ) -> NotifyResult {
        Ok(())
    }
}

/// Asserts `err` is (or wraps) `PermissionDenied` — the shape every
/// `UseCaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>` use case
/// returns when `AuthenticatedUser::require(...)` fails.
fn assert_permission_denied(err: &(dyn std::error::Error + Send + Sync + 'static)) {
    assert!(
        err.downcast_ref::<PermissionDenied>().is_some(),
        "expected PermissionDenied, got: {err}"
    );
}

#[tokio::test]
async fn member_can_read_but_not_write_monitor_in_same_tenant() {
    let tid = tenant_id();
    let alice_id = UserId(Uuid::new_v4());
    let bob_id = UserId(Uuid::new_v4());

    let monitor_repo = InMemoryMonitorRepository::new();
    let create_monitor = CreateMonitorUseCase::new(
        monitor_repo.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    );

    let monitor = create_monitor
        .execute(monitor_request("Alice Monitor"), auth_owner(alice_id, tid))
        .await
        .unwrap();

    // Bob (Member) can VIEW Alice's monitor — Member keeps ViewMonitor.
    let get_use_case = GetMonitorUseCase::new(monitor_repo.clone());
    let fetched = get_use_case
        .execute(GetMonitor { id: monitor.id }, auth_member(bob_id, tid))
        .await
        .unwrap();
    assert_eq!(fetched.unwrap().id, monitor.id);

    // Bob (Member) CANNOT update Alice's monitor — Member is read-only.
    let update_use_case = UpdateMonitorUseCase::new(monitor_repo.clone(), new_audit_repo());
    let update_err = update_use_case
        .execute(
            UpdateMonitor {
                id: monitor.id,
                name: "Bob Updated".to_string(),
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                expected_status: 200,
                interval_seconds: 300,
                timeout_seconds: 30,
                assertions: vec![],
                is_public: false,
                ssl_check_enabled: false,
                ssl_expiry_warning_days: 30,
                kind: MonitorKind::Http,
                tcp_port: None,
                dns_record_type: None,
                dns_expected_value: None,
                heartbeat_interval_seconds: None,
                heartbeat_grace_period_seconds: None,
            },
            auth_member(bob_id, tid),
        )
        .await
        .unwrap_err();
    assert_permission_denied(&*update_err);

    // Confirm the monitor was NOT actually changed by the rejected update.
    let still_original = get_use_case
        .execute(GetMonitor { id: monitor.id }, auth_owner(alice_id, tid))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(still_original.name.as_str(), "Alice Monitor");

    // Bob (Member) CANNOT delete Alice's monitor either.
    let delete_use_case = DeleteMonitorUseCase::new(monitor_repo.clone(), new_audit_repo());
    let delete_err = delete_use_case
        .execute(DeleteMonitor { id: monitor.id }, auth_member(bob_id, tid))
        .await
        .unwrap_err();
    assert_permission_denied(&*delete_err);

    // Still there afterwards.
    let still_exists = get_use_case
        .execute(GetMonitor { id: monitor.id }, auth_owner(alice_id, tid))
        .await
        .unwrap();
    assert!(still_exists.is_some());
}

#[tokio::test]
async fn outsider_cannot_access_monitor_in_other_tenant() {
    let tid = tenant_id();
    let alice_id = UserId(Uuid::new_v4());
    let outsider_id = UserId(Uuid::new_v4());
    let other_tid = tenant_id();

    let monitor_repo = InMemoryMonitorRepository::new();
    let create_monitor = CreateMonitorUseCase::new(
        monitor_repo.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    );

    let monitor = create_monitor
        .execute(monitor_request("Alice Monitor"), auth_owner(alice_id, tid))
        .await
        .unwrap();

    // Outsider ba zai iya gani ba
    let get_use_case = GetMonitorUseCase::new(monitor_repo.clone());
    let fetched = get_use_case
        .execute(
            GetMonitor { id: monitor.id },
            auth_owner(outsider_id, other_tid),
        )
        .await
        .unwrap();
    assert!(fetched.is_none());

    // Outsider ba zai iya gyara ba — even though this outsider is an Owner
    // (so has UpdateMonitor), tenant isolation still blocks cross-tenant
    // access and returns None, same as before this permission patch.
    let update_use_case = UpdateMonitorUseCase::new(monitor_repo.clone(), new_audit_repo());
    let updated = update_use_case
        .execute(
            UpdateMonitor {
                id: monitor.id,
                name: "Hacker".to_string(),
                url: "https://evil.com".to_string(),
                method: HttpMethod::Get,
                expected_status: 200,
                interval_seconds: 300,
                timeout_seconds: 30,
                assertions: vec![],
                is_public: false,
                ssl_check_enabled: false,
                ssl_expiry_warning_days: 30,
                kind: MonitorKind::Http,
                tcp_port: None,
                dns_record_type: None,
                dns_expected_value: None,
                heartbeat_interval_seconds: None,
                heartbeat_grace_period_seconds: None,
            },
            auth_owner(outsider_id, other_tid),
        )
        .await
        .unwrap();
    assert!(updated.is_none());

    // Outsider ba zai iya share ba
    let delete_use_case = DeleteMonitorUseCase::new(monitor_repo.clone(), new_audit_repo());
    let deleted = delete_use_case
        .execute(
            DeleteMonitor { id: monitor.id },
            auth_owner(outsider_id, other_tid),
        )
        .await
        .unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn member_can_read_but_not_write_status_pages_or_custom_domain_in_same_tenant() {
    let tid = tenant_id();
    let alice_id = UserId(Uuid::new_v4());
    let bob_id = UserId(Uuid::new_v4());

    let monitor_repo = InMemoryMonitorRepository::new();
    let status_page_repo = InMemoryStatusPageRepository::new();

    let create_monitor = CreateMonitorUseCase::new(
        monitor_repo.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    );
    let monitor = create_monitor
        .execute(monitor_request("Alice Monitor"), auth_owner(alice_id, tid))
        .await
        .unwrap();

    let create_status_page = CreateStatusPageUseCase::new(
        status_page_repo.clone(),
        UuidIdGenerator::new(),
        monitor_repo.clone(),
        new_audit_repo(),
    );
    let status_page = create_status_page
        .execute(
            CreateStatusPageRequest {
                name: "Alice Status".to_string(),
                subdomain: "alice-status".to_string(),
                monitor_ids: vec![monitor.id],
            },
            auth_owner(alice_id, tid),
        )
        .await
        .unwrap();

    // Bob (Member) can LIST status pages — read access is unaffected.
    let list_use_case = ListStatusPagesUseCase::new(status_page_repo.clone());
    let pages = list_use_case
        .execute(auth_member(bob_id, tid))
        .await
        .unwrap();
    assert_eq!(pages.len(), 1);

    // Bob (Member) CANNOT update the status page.
    let update_use_case = UpdateStatusPageUseCase::new(
        status_page_repo.clone(),
        monitor_repo.clone(),
        new_audit_repo(),
    );
    let update_err = update_use_case
        .execute(
            UpdateStatusPage {
                id: status_page.id,
                name: "Bob Updated Status".to_string(),
                monitor_ids: vec![monitor.id],
                logo_url: None,
                primary_color: Some("#ffffff".to_string()),
            },
            auth_member(bob_id, tid),
        )
        .await
        .unwrap_err();
    assert!(matches!(update_err, UpdateStatusPageError::Forbidden));

    // Bob (Member) CANNOT request a custom domain.
    let request_domain_use_case =
        RequestCustomDomainUseCase::new(status_page_repo.clone(), new_audit_repo());
    let domain_err = request_domain_use_case
        .execute(
            RequestCustomDomain {
                id: status_page.id,
                domain: "status.acme.com".to_string(),
            },
            auth_member(bob_id, tid),
        )
        .await
        .unwrap_err();
    assert!(matches!(domain_err, RequestCustomDomainError::Forbidden));

    // Confirm nothing actually changed under Bob's rejected requests.
    let unchanged = list_use_case
        .execute(auth_owner(alice_id, tid))
        .await
        .unwrap();
    assert_eq!(unchanged[0].name, "Alice Status");
    assert!(unchanged[0].custom_domain.is_none());

    // Bob (Member) CANNOT delete the status page.
    let delete_use_case = DeleteStatusPageUseCase::new(status_page_repo.clone(), new_audit_repo());
    let delete_err = delete_use_case
        .execute(status_page.id, auth_member(bob_id, tid))
        .await
        .unwrap_err();
    assert_permission_denied(&*delete_err);

    // Still there afterwards.
    let still_there = list_use_case
        .execute(auth_owner(alice_id, tid))
        .await
        .unwrap();
    assert_eq!(still_there.len(), 1);
}

#[tokio::test]
async fn member_can_read_but_not_write_notification_channels_in_same_tenant() {
    let tid = tenant_id();
    let alice_id = UserId(Uuid::new_v4());
    let bob_id = UserId(Uuid::new_v4());

    let monitor_repo = InMemoryMonitorRepository::new();
    let notification_repo = InMemoryNotificationChannelRepository::new(monitor_repo.clone());

    let create_monitor = CreateMonitorUseCase::new(
        monitor_repo.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    );
    let monitor = create_monitor
        .execute(monitor_request("Alice Monitor"), auth_owner(alice_id, tid))
        .await
        .unwrap();

    let create_channel_use_case = CreateNotificationChannelUseCase::new(
        Arc::new(monitor_repo.clone()),
        Arc::new(notification_repo.clone()),
        Arc::new(UuidIdGenerator::new()),
        Arc::new(StubSecretCipher),
        new_audit_repo(),
    );
    let channel = create_channel_use_case
        .execute(
            CreateNotificationChannel {
                monitor_id: monitor.id,
                kind: NotificationChannelKind::Email,
                target: "alice@example.com".to_string(),
                chat_id: None,
            },
            auth_owner(alice_id, tid),
        )
        .await
        .unwrap();

    // Bob (Member) can LIST notification channels — read access is
    // unaffected.
    let list_use_case = ListNotificationChannelsUseCase::new(
        Arc::new(monitor_repo.clone()),
        Arc::new(notification_repo.clone()),
    );
    let channels = list_use_case
        .execute(monitor.id, auth_member(bob_id, tid))
        .await
        .unwrap();
    assert_eq!(channels.len(), 1);

    // Bob (Member) CANNOT send a test notification.
    let send_test_use_case = SendTestNotificationUseCase::new(
        Arc::new(notification_repo.clone()),
        Arc::new(monitor_repo.clone()),
        Arc::new(StubNotificationService),
    );
    let send_err = send_test_use_case
        .execute(channel.id, auth_member(bob_id, tid))
        .await
        .unwrap_err();
    assert!(matches!(send_err, SendTestNotificationError::Forbidden));

    // Bob (Member) CANNOT delete the notification channel.
    let delete_use_case =
        DeleteNotificationChannelUseCase::new(notification_repo.clone(), new_audit_repo());
    let delete_err = delete_use_case
        .execute(
            DeleteNotificationChannel { id: channel.id },
            auth_member(bob_id, tid),
        )
        .await
        .unwrap_err();
    assert_permission_denied(&*delete_err);

    // Still there afterwards.
    let still_there = list_use_case
        .execute(monitor.id, auth_owner(alice_id, tid))
        .await
        .unwrap();
    assert_eq!(still_there.len(), 1);
}

/// Owner (and, by the same `Role::permissions()` table, Admin) retains
/// full write access within their own tenant — this patch only tightens
/// `Member`/`Viewer`/`Auditor`, it doesn't touch what privileged roles can
/// already do.
#[tokio::test]
async fn owner_can_still_write_monitor_in_own_tenant() {
    let tid = tenant_id();
    let alice_id = UserId(Uuid::new_v4());

    let monitor_repo = InMemoryMonitorRepository::new();
    let create_monitor = CreateMonitorUseCase::new(
        monitor_repo.clone(),
        UuidIdGenerator::new(),
        new_audit_repo(),
    );
    let monitor = create_monitor
        .execute(monitor_request("Alice Monitor"), auth_owner(alice_id, tid))
        .await
        .unwrap();

    let update_use_case = UpdateMonitorUseCase::new(monitor_repo.clone(), new_audit_repo());
    let updated = update_use_case
        .execute(
            UpdateMonitor {
                id: monitor.id,
                name: "Alice Updated".to_string(),
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                expected_status: 200,
                interval_seconds: 300,
                timeout_seconds: 30,
                assertions: vec![],
                is_public: false,
                ssl_check_enabled: false,
                ssl_expiry_warning_days: 30,
                kind: MonitorKind::Http,
                tcp_port: None,
                dns_record_type: None,
                dns_expected_value: None,
                heartbeat_interval_seconds: None,
                heartbeat_grace_period_seconds: None,
            },
            auth_owner(alice_id, tid),
        )
        .await
        .unwrap();
    assert_eq!(updated.unwrap().name.as_str(), "Alice Updated");
}
