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
        UpdateStatusPageUseCase,
    },
};
use async_trait::async_trait;
use domain::{
    entities::NotificationChannel,
    enums::{HttpMethod, MonitorKind, NotificationChannelKind},
    notification::NotificationEvent,
    shared::ids::{TenantId, UserId},
    shared::types::{AuthenticatedUser, Role, TenantContext},
};
use infra::{
    InMemoryMonitorRepository, InMemoryNotificationChannelRepository, InMemoryStatusPageRepository,
    UuidIdGenerator,
};
use uuid::Uuid;

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
        interval_seconds: 60,
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

#[tokio::test]
async fn member_can_read_update_and_delete_monitor_in_same_tenant() {
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

    // Bob na iya ganin monitor din Alice
    let get_use_case = GetMonitorUseCase::new(monitor_repo.clone());
    let fetched = get_use_case
        .execute(GetMonitor { id: monitor.id }, auth_member(bob_id, tid))
        .await
        .unwrap();
    assert_eq!(fetched.unwrap().id, monitor.id);

    // Bob na iya gyara monitor din Alice
    let update_use_case = UpdateMonitorUseCase::new(monitor_repo.clone(), new_audit_repo());
    let updated = update_use_case
        .execute(
            UpdateMonitor {
                id: monitor.id,
                name: "Bob Updated".to_string(),
                url: "https://example.com".to_string(),
                method: HttpMethod::Get,
                expected_status: 200,
                interval_seconds: 60,
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
        .unwrap();
    assert_eq!(updated.unwrap().name.as_str(), "Bob Updated");

    // Bob na iya share monitor din Alice
    let delete_use_case = DeleteMonitorUseCase::new(monitor_repo.clone(), new_audit_repo());
    let deleted = delete_use_case
        .execute(DeleteMonitor { id: monitor.id }, auth_member(bob_id, tid))
        .await
        .unwrap();
    assert!(deleted);
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

    // Outsider ba zai iya gyara ba
    let update_use_case = UpdateMonitorUseCase::new(monitor_repo.clone(), new_audit_repo());
    let updated = update_use_case
        .execute(
            UpdateMonitor {
                id: monitor.id,
                name: "Hacker".to_string(),
                url: "https://evil.com".to_string(),
                method: HttpMethod::Get,
                expected_status: 200,
                interval_seconds: 60,
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
async fn member_can_access_status_pages_and_custom_domain_in_same_tenant() {
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

    // Bob na iya lissafa status pages
    let list_use_case = ListStatusPagesUseCase::new(status_page_repo.clone());
    let pages = list_use_case
        .execute(auth_member(bob_id, tid))
        .await
        .unwrap();
    assert_eq!(pages.len(), 1);

    // Bob na iya gyara status page
    let update_use_case = UpdateStatusPageUseCase::new(
        status_page_repo.clone(),
        monitor_repo.clone(),
        new_audit_repo(),
    );
    let updated_page = update_use_case
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
        .unwrap()
        .unwrap();
    assert_eq!(updated_page.name, "Bob Updated Status");

    // Bob na iya buƙatar custom domain
    let request_domain_use_case =
        RequestCustomDomainUseCase::new(status_page_repo.clone(), new_audit_repo());
    let page_with_domain = request_domain_use_case
        .execute(
            RequestCustomDomain {
                id: status_page.id,
                domain: "status.acme.com".to_string(),
            },
            auth_member(bob_id, tid),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(page_with_domain.custom_domain.is_some());

    // Bob na iya share status page
    let delete_use_case = DeleteStatusPageUseCase::new(status_page_repo.clone(), new_audit_repo());
    let deleted = delete_use_case
        .execute(status_page.id, auth_member(bob_id, tid))
        .await
        .unwrap();
    assert!(deleted);
}

#[tokio::test]
async fn member_can_access_notification_channels_in_same_tenant() {
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

    // Bob na iya lissafa notification channels
    let list_use_case = ListNotificationChannelsUseCase::new(
        Arc::new(monitor_repo.clone()),
        Arc::new(notification_repo.clone()),
    );
    let channels = list_use_case
        .execute(monitor.id, auth_member(bob_id, tid))
        .await
        .unwrap();
    assert_eq!(channels.len(), 1);

    // Bob na iya aika test notification
    let send_test_use_case = SendTestNotificationUseCase::new(
        Arc::new(notification_repo.clone()),
        Arc::new(monitor_repo.clone()),
        Arc::new(StubNotificationService),
    );
    send_test_use_case
        .execute(channel.id, auth_member(bob_id, tid))
        .await
        .unwrap();

    // Bob na iya share notification channel
    let delete_use_case =
        DeleteNotificationChannelUseCase::new(notification_repo.clone(), new_audit_repo());
    let deleted = delete_use_case
        .execute(
            DeleteNotificationChannel { id: channel.id },
            auth_member(bob_id, tid),
        )
        .await
        .unwrap();
    assert!(deleted);
}
