mod common;

use application::ports::status_page_repository::StatusPageRepository;
use common::sample_status_page_with_user;
use domain::value_objects::{CustomDomain, HexColor};
use infra::PostgresStatusPageRepository;
use serial_test::serial;

#[serial]
#[tokio::test]
async fn save_and_find_by_id_for_user_round_trips_status_page() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repository = PostgresStatusPageRepository::new(pool.clone());

    let page = sample_status_page_with_user(&pool, "Acme Status", "acme").await;
    repository.save(&page).await.unwrap();

    let found = repository
        .find_by_id_for_user(page.id, page.user_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, page.id);
    assert_eq!(found.user_id, page.user_id);
    assert_eq!(found.name, page.name);
    assert_eq!(found.subdomain.as_str(), "acme");
    assert!(found.custom_domain.is_none());
    assert!(found.logo_url.is_none());
    assert!(found.primary_color.is_none());
}

#[serial]
#[tokio::test]
async fn find_by_id_for_user_returns_none_for_wrong_user() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repository = PostgresStatusPageRepository::new(pool.clone());

    let page = sample_status_page_with_user(&pool, "Acme Status", "acme").await;
    repository.save(&page).await.unwrap();

    let attacker = domain::shared::ids::UserId::new();
    let found = repository
        .find_by_id_for_user(page.id, attacker)
        .await
        .unwrap();

    assert!(found.is_none());
}

#[serial]
#[tokio::test]
async fn update_persists_logo_and_color_changes() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repository = PostgresStatusPageRepository::new(pool.clone());

    let mut page = sample_status_page_with_user(&pool, "Acme Status", "acme").await;
    repository.save(&page).await.unwrap();

    page.set_logo_url(Some("https://cdn.pingvertex.com/logo.png".to_string()))
        .unwrap();
    page.set_primary_color(Some(HexColor::new("#1A2B3C").unwrap()));
    repository.update(&page).await.unwrap();

    let found = repository
        .find_by_id_for_user(page.id, page.user_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        found.logo_url.as_deref(),
        Some("https://cdn.pingvertex.com/logo.png")
    );
    assert_eq!(
        found.primary_color.map(|c| c.as_str().to_owned()),
        Some("#1A2B3C".to_string())
    );
}

#[serial]
#[tokio::test]
async fn find_by_subdomain_returns_matching_page() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repository = PostgresStatusPageRepository::new(pool.clone());

    let page = sample_status_page_with_user(&pool, "Acme Status", "acme").await;
    repository.save(&page).await.unwrap();

    let found = repository.find_by_subdomain("acme").await.unwrap().unwrap();
    assert_eq!(found.id, page.id);

    let not_found = repository.find_by_subdomain("nonexistent").await.unwrap();
    assert!(not_found.is_none());
}

#[serial]
#[tokio::test]
async fn find_by_verified_custom_domain_ignores_pending_domains() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repository = PostgresStatusPageRepository::new(pool.clone());

    let mut page = sample_status_page_with_user(&pool, "Acme Status", "acme").await;
    page.request_custom_domain(CustomDomain::new("status.acme.com").unwrap());
    repository.save(&page).await.unwrap();

    // SECURITY: a still-Pending custom domain must NEVER be returned by
    // find_by_verified_custom_domain — this is what makes host-header
    // routing a strict allowlist of verified domains only.
    let found = repository
        .find_by_verified_custom_domain("status.acme.com")
        .await
        .unwrap();
    assert!(
        found.is_none(),
        "a pending (unverified) custom domain must not be matched"
    );
}

#[serial]
#[tokio::test]
async fn find_by_verified_custom_domain_returns_page_once_verified() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repository = PostgresStatusPageRepository::new(pool.clone());

    let mut page = sample_status_page_with_user(&pool, "Acme Status", "acme").await;
    page.request_custom_domain(CustomDomain::new("status.acme.com").unwrap());
    page.mark_custom_domain_verified().unwrap();
    repository.save(&page).await.unwrap();

    let found = repository
        .find_by_verified_custom_domain("status.acme.com")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.id, page.id);
    assert_eq!(found.verified_custom_domain(), Some("status.acme.com"));
}

#[serial]
#[tokio::test]
async fn delete_for_user_removes_page_only_for_owner() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repository = PostgresStatusPageRepository::new(pool.clone());

    let page = sample_status_page_with_user(&pool, "Acme Status", "acme").await;
    repository.save(&page).await.unwrap();

    let attacker = domain::shared::ids::UserId::new();
    let deleted_by_attacker = repository.delete_for_user(page.id, attacker).await.unwrap();
    assert!(!deleted_by_attacker);

    let deleted_by_owner = repository
        .delete_for_user(page.id, page.user_id)
        .await
        .unwrap();
    assert!(deleted_by_owner);

    let found = repository
        .find_by_id_for_user(page.id, page.user_id)
        .await
        .unwrap();
    assert!(found.is_none());
}

#[serial]
#[tokio::test]
async fn find_all_for_user_returns_only_owned_pages() {
    let (pool, _guard) = common::setup_pool_exclusive().await;
    let repository = PostgresStatusPageRepository::new(pool.clone());

    let mine = sample_status_page_with_user(&pool, "Mine", "mine-page").await;
    let theirs = sample_status_page_with_user(&pool, "Theirs", "theirs-page").await;

    repository.save(&mine).await.unwrap();
    repository.save(&theirs).await.unwrap();

    let found = repository.find_all_for_user(mine.user_id).await.unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, mine.id);
}
