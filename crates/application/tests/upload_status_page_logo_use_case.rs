use application::ports::UsageEventRepository;
use application::ports::WalletRepository;
mod common;
use common::*;

use application::{
    commands::UploadStatusPageLogo, ports::StatusPageRepository,
    use_cases::UploadStatusPageLogoUseCase,
};
use domain::{
    entities::{CreateStatusPage, StatusPage},
    shared::ids::{StatusPageId, UserId},
    value_objects::Subdomain,
};
use infra::{InMemoryLogoStorage, InMemoryStatusPageRepository};

fn sample_page(user_id: UserId, tenant_id: domain::TenantId) -> StatusPage {
    StatusPage::create(CreateStatusPage {
        id: StatusPageId::new(),
        user_id,
        tenant_id,
        name: "Acme Status".to_string(),
        subdomain: Subdomain::new("acme").unwrap(),
        monitor_ids: vec![],
    })
    .unwrap()
}

#[tokio::test]
async fn uploads_valid_png_and_updates_status_page() {
    let repo = InMemoryStatusPageRepository::new();
    let storage = InMemoryLogoStorage::new();
    let user_id = UserId::new();
    let tid = tenant_id();
    let page = sample_page(user_id, tid);
    repo.save(&page).await.unwrap();

    let use_case =
        UploadStatusPageLogoUseCase::new(repo.clone(), storage.clone(), new_audit_repo());
    let png = b"\x89PNG\r\n\x1a\n".to_vec();

    let updated = use_case
        .execute(
            UploadStatusPageLogo {
                id: page.id,
                data: png,
            },
            auth_user(user_id, tid),
        )
        .await
        .unwrap()
        .expect("status page should exist and logo should upload");

    let logo_url = updated.logo_url.as_deref().expect("logo_url should be set");
    assert!(logo_url.starts_with("https://cdn.test/logos/inmemory/"));
    assert!(logo_url.ends_with(".png"));

    let persisted = repo
        .find_by_id_for_user(page.id, user_id)
        .await
        .unwrap()
        .expect("status page should still exist");
    assert_eq!(persisted.logo_url.as_deref(), Some(logo_url));
}

#[tokio::test]
async fn returns_none_when_caller_is_from_another_tenant() {
    let repo = InMemoryStatusPageRepository::new();
    let storage = InMemoryLogoStorage::new();
    let owner = UserId::new();
    let owner_tid = tenant_id();
    let attacker = UserId::new();
    let attacker_tid = tenant_id();
    let page = sample_page(owner, owner_tid);
    repo.save(&page).await.unwrap();

    let use_case =
        UploadStatusPageLogoUseCase::new(repo.clone(), storage.clone(), new_audit_repo());

    let result = use_case
        .execute(
            UploadStatusPageLogo {
                id: page.id,
                data: b"\x89PNG\r\n\x1a\n".to_vec(),
            },
            auth_user(attacker, attacker_tid),
        )
        .await
        .unwrap();

    assert!(result.is_none());
}

#[tokio::test]
async fn metered_logo_upload_charges_ceil_mb_and_is_idempotent() {
    use chrono::{Duration, Utc};
    use domain::PAYGPricing;
    use infra::{
        InMemoryPAYGPricingRepository, InMemoryUsageEventRepository, InMemoryWalletRepository,
    };
    use std::sync::Arc;
    use uuid::Uuid;

    let repo = InMemoryStatusPageRepository::new();
    let storage = InMemoryLogoStorage::new();
    let user_id = UserId::new();
    let tid = tenant_id();
    let page = sample_page(user_id, tid);
    repo.save(&page).await.unwrap();

    let wallet = Arc::new(InMemoryWalletRepository::default());
    wallet.get_or_create(tid.0, "USD").await.unwrap();
    wallet
        .credit(
            tid.0,
            100,
            domain::WalletTransactionType::TopUp,
            "logo-topup",
            None,
        )
        .await
        .unwrap();
    let pricing = Arc::new(InMemoryPAYGPricingRepository::new());
    let _ = application::ports::PAYGPricingRepository::create_version(
        &*pricing,
        "storage",
        "MB-month",
        20,
        "USD",
        chrono::Utc::now() - chrono::Duration::minutes(1),
    )
    .await;
    let usage = Arc::new(InMemoryUsageEventRepository::default());
    let metering = Arc::new(application::MeterUsageUseCase::new(
        wallet.clone(),
        pricing,
        usage.clone(),
    ));

    let use_case = UploadStatusPageLogoUseCase::new(repo.clone(), storage, new_audit_repo())
        .with_metering(metering);
    let request_id = "storage-test-request".to_owned();
    let png = b"\x89PNG\r\n\x1a\n".to_vec();

    use_case
        .execute_with_request_id(
            UploadStatusPageLogo {
                id: page.id,
                data: png.clone(),
            },
            auth_user(user_id, tid),
            request_id.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        wallet
            .get_or_create(tid.0, "USD")
            .await
            .unwrap()
            .balance_units,
        80
    );

    // A retried request with the same idempotency key must not perform a
    // second storage charge or invoke the storage operation again.
    use_case
        .execute_with_request_id(
            UploadStatusPageLogo {
                id: page.id,
                data: png,
            },
            auth_user(user_id, tid),
            request_id.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        wallet
            .get_or_create(tid.0, "USD")
            .await
            .unwrap()
            .balance_units,
        80
    );
    assert_eq!(
        usage
            .find_by_request(tid.0, &format!("storage:{request_id}"))
            .await
            .unwrap()
            .unwrap()
            .charge_units,
        20
    );
}
