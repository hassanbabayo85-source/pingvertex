mod common;

use application::ports::monitor_repository::MonitorRepository;
use application::ports::notification_channel_repository::NotificationChannelRepository;
use common::{insert_user, sample_monitor_with_user, setup_pool};
use domain::entities::{CreateEmailChannel, CreateWebhookChannel, NotificationChannel};
use domain::shared::ids::{NotificationChannelId, UserId};
use domain::value_objects::{Email, Url};
use infra::{PostgresMonitorRepository, PostgresNotificationChannelRepository};
use serial_test::serial;
use uuid::Uuid;

#[serial]
#[tokio::test]
async fn create_and_find_by_id_for_user_round_trips_email_channel() {
    let pool = setup_pool().await;
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let monitor = sample_monitor_with_user(&pool, "Google", "https://google.com").await;
    monitor_repo.save(&monitor).await.unwrap();

    let channel = NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId::new(),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        email: Email::new("alerts@example.com").unwrap(),
    });

    repo.create(&channel).await.unwrap();

    let found = repo
        .find_by_id_for_user(channel.id, monitor.user_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, channel.id);
    assert_eq!(found.monitor_id, monitor.id);
    assert_eq!(found.target, "alerts@example.com");
    assert_eq!(found.signing_secret, None);
}

#[serial]
#[tokio::test]
async fn create_webhook_channel_round_trips_with_signing_secret() {
    let pool = setup_pool().await;
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let monitor = sample_monitor_with_user(&pool, "Google", "https://google.com").await;
    monitor_repo.save(&monitor).await.unwrap();

    let channel = NotificationChannel::create_webhook(CreateWebhookChannel {
        id: NotificationChannelId::new(),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        url: Url::new("https://hooks.example.com/alert").unwrap(),
        signing_secret: "whsec_test_secret".to_string(),
    });

    repo.create(&channel).await.unwrap();

    let found = repo
        .find_by_id_for_user(channel.id, monitor.user_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.target, "https://hooks.example.com/alert");
    assert_eq!(found.signing_secret.as_deref(), Some("whsec_test_secret"));
}

#[serial]
#[tokio::test]
async fn find_by_id_for_user_returns_none_for_wrong_user() {
    let pool = setup_pool().await;
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let monitor = sample_monitor_with_user(&pool, "Google", "https://google.com").await;
    monitor_repo.save(&monitor).await.unwrap();

    let channel = NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId::new(),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        email: Email::new("owner@example.com").unwrap(),
    });
    repo.create(&channel).await.unwrap();

    // Owner can read it back.
    assert!(
        repo.find_by_id_for_user(channel.id, monitor.user_id)
            .await
            .unwrap()
            .is_some()
    );

    // A different user cannot, even with the correct channel id (IDOR).
    let attacker_id = UserId(Uuid::new_v4());
    insert_user(&pool, attacker_id).await;

    let stolen = repo
        .find_by_id_for_user(channel.id, attacker_id)
        .await
        .unwrap();
    assert!(stolen.is_none());
}

#[serial]
#[tokio::test]
async fn list_by_monitor_for_user_returns_none_when_monitor_not_owned() {
    let pool = setup_pool().await;
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let monitor = sample_monitor_with_user(&pool, "Google", "https://google.com").await;
    monitor_repo.save(&monitor).await.unwrap();

    let attacker_id = UserId(Uuid::new_v4());
    insert_user(&pool, attacker_id).await;

    // `None` here, not `Some(vec![])` — this is what lets the handler
    // return 404 instead of an empty 200 that would leak "this monitor id
    // exists, it's just not yours".
    let result = repo
        .list_by_monitor_for_user(monitor.id, attacker_id)
        .await
        .unwrap();
    assert!(result.is_none());
}

#[serial]
#[tokio::test]
async fn list_by_monitor_for_user_returns_only_the_owners_channels() {
    let pool = setup_pool().await;
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let monitor = sample_monitor_with_user(&pool, "Google", "https://google.com").await;
    monitor_repo.save(&monitor).await.unwrap();

    let email_channel = NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId::new(),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        email: Email::new("one@example.com").unwrap(),
    });
    let webhook_channel = NotificationChannel::create_webhook(CreateWebhookChannel {
        id: NotificationChannelId::new(),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        url: Url::new("https://hooks.example.com/two").unwrap(),
        signing_secret: "whsec_two".to_string(),
    });

    repo.create(&email_channel).await.unwrap();
    repo.create(&webhook_channel).await.unwrap();

    let owner_list = repo
        .list_by_monitor_for_user(monitor.id, monitor.user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(owner_list.len(), 2);
}

#[serial]
#[tokio::test]
async fn delete_for_user_removes_channel_only_for_owner() {
    let pool = setup_pool().await;
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let monitor = sample_monitor_with_user(&pool, "Google", "https://google.com").await;
    monitor_repo.save(&monitor).await.unwrap();

    let channel = NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId::new(),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        email: Email::new("owner@example.com").unwrap(),
    });
    repo.create(&channel).await.unwrap();

    let attacker_id = UserId(Uuid::new_v4());
    insert_user(&pool, attacker_id).await;

    // Attacker cannot delete the owner's channel.
    let attacker_delete = repo.delete_for_user(channel.id, attacker_id).await.unwrap();
    assert!(!attacker_delete);

    // It's still there afterwards.
    assert!(
        repo.find_by_id_for_user(channel.id, monitor.user_id)
            .await
            .unwrap()
            .is_some()
    );

    // The real owner can delete it.
    let owner_delete = repo
        .delete_for_user(channel.id, monitor.user_id)
        .await
        .unwrap();
    assert!(owner_delete);

    assert!(
        repo.find_by_id_for_user(channel.id, monitor.user_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[serial]
#[tokio::test]
async fn delete_for_user_on_nonexistent_channel_returns_false() {
    let pool = setup_pool().await;
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let user_id = UserId(Uuid::new_v4());
    insert_user(&pool, user_id).await;

    let deleted = repo
        .delete_for_user(NotificationChannelId::new(), user_id)
        .await
        .unwrap();

    assert!(!deleted);
}

#[serial]
#[tokio::test]
async fn deleting_monitor_cascades_to_notification_channels() {
    let pool = setup_pool().await;
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let monitor = sample_monitor_with_user(&pool, "Google", "https://google.com").await;
    monitor_repo.save(&monitor).await.unwrap();

    let channel = NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId::new(),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        email: Email::new("owner@example.com").unwrap(),
    });
    repo.create(&channel).await.unwrap();

    monitor_repo.delete(monitor.id).await.unwrap();

    let found = repo
        .find_by_id_for_user(channel.id, monitor.user_id)
        .await
        .unwrap();
    assert!(found.is_none());
}

#[serial]
#[tokio::test]
async fn list_enabled_by_monitor_returns_channels_without_ownership_check() {
    let pool = setup_pool().await;
    let monitor_repo = PostgresMonitorRepository::new(pool.clone());
    let repo = PostgresNotificationChannelRepository::new(pool.clone());

    let monitor = sample_monitor_with_user(&pool, "Google", "https://google.com").await;
    monitor_repo.save(&monitor).await.unwrap();

    let channel = NotificationChannel::create_email(CreateEmailChannel {
        id: NotificationChannelId::new(),
        monitor_id: monitor.id,
        user_id: monitor.user_id,
        email: Email::new("owner@example.com").unwrap(),
    });
    repo.create(&channel).await.unwrap();

    // No `user_id` argument at all — this is the worker-only method used
    // by `IncidentManager::notify_channels`, which has no authenticated
    // caller to check ownership against.
    let channels = repo.list_enabled_by_monitor(monitor.id).await.unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, channel.id);
}
