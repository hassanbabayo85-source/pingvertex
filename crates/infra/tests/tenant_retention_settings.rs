//! GAP 3 Kashi C: repository tests for `TenantRetentionSettings`.
//!
//! Uses the InMemory implementation. Postgres is covered indirectly
//! through `worker::retention_worker` and the SQL migration.

use application::ports::TenantRetentionSettingsRepository;
use domain::entities::{RetentionSource, TenantRetentionSettings};
use domain::shared::ids::TenantId;
use infra::InMemoryTenantRetentionSettingsRepository;

#[tokio::test]
async fn get_returns_none_before_any_write() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    assert!(repo.get(TenantId::new()).await.unwrap().is_none());
}

#[tokio::test]
async fn upsert_inserts_then_get_returns_it() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    let tenant = TenantId::new();
    let s = TenantRetentionSettings::new(tenant, 90, RetentionSource::Default);
    repo.upsert(&s).await.unwrap();

    let got = repo.get(tenant).await.unwrap().unwrap();
    assert_eq!(got.retention_days, 90);
    assert_eq!(got.source, RetentionSource::Default);
}

#[tokio::test]
async fn upsert_upgrades_from_default_to_addon() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    let tenant = TenantId::new();

    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        30,
        RetentionSource::Default,
    ))
    .await
    .unwrap();
    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        365,
        RetentionSource::Addon,
    ))
    .await
    .unwrap();

    let got = repo.get(tenant).await.unwrap().unwrap();
    assert_eq!(got.retention_days, 365);
    assert_eq!(got.source, RetentionSource::Addon);
}

#[tokio::test]
async fn upsert_does_not_downgrade_from_addon_to_default() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    let tenant = TenantId::new();

    // Addon first.
    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        365,
        RetentionSource::Addon,
    ))
    .await
    .unwrap();

    // A concurrent/default writer tries to overwrite with a lower-priority
    // source. It must NOT clobber the Addon row.
    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        30,
        RetentionSource::Default,
    ))
    .await
    .unwrap();

    let got = repo.get(tenant).await.unwrap().unwrap();
    assert_eq!(got.retention_days, 365);
    assert_eq!(got.source, RetentionSource::Addon);
}

#[tokio::test]
async fn upsert_does_not_downgrade_from_legacy_to_default() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    let tenant = TenantId::new();

    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        3650,
        RetentionSource::Legacy,
    ))
    .await
    .unwrap();

    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        30,
        RetentionSource::Default,
    ))
    .await
    .unwrap();

    let got = repo.get(tenant).await.unwrap().unwrap();
    assert_eq!(got.retention_days, 3650);
    assert_eq!(got.source, RetentionSource::Legacy);
}

#[tokio::test]
async fn effective_retention_uses_row_when_present() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    let tenant = TenantId::new();
    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        90,
        RetentionSource::Addon,
    ))
    .await
    .unwrap();

    let v = repo.effective_retention(tenant, 30, 3650).await.unwrap();
    assert_eq!(v, 90);
}

#[tokio::test]
async fn effective_retention_falls_back_to_default_when_no_row() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    let v = repo
        .effective_retention(TenantId::new(), 30, 3650)
        .await
        .unwrap();
    assert_eq!(v, 30);
}

#[tokio::test]
async fn effective_retention_clamps_to_max() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    let tenant = TenantId::new();
    // Row says 9999; max says 3650. The min() clamps to 3650.
    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        9999,
        RetentionSource::Addon,
    ))
    .await
    .unwrap();
    let v = repo.effective_retention(tenant, 30, 3650).await.unwrap();
    assert_eq!(v, 3650);
}

#[tokio::test]
async fn effective_retention_clamps_minimum_to_one() {
    let repo = InMemoryTenantRetentionSettingsRepository::new();
    let tenant = TenantId::new();
    // Row says 0 (invalid but possible via direct DB write).
    repo.upsert(&TenantRetentionSettings::new(
        tenant,
        0,
        RetentionSource::Default,
    ))
    .await
    .unwrap();
    let v = repo.effective_retention(tenant, 0, 3650).await.unwrap();
    // The .max(1) floor clamps to 1.
    assert_eq!(v, 1);
}

#[test]
fn retention_source_priority_order() {
    assert!(RetentionSource::Addon.priority() > RetentionSource::Legacy.priority());
    assert!(RetentionSource::Legacy.priority() > RetentionSource::Default.priority());
}
