use application::ports::WalletRepository;
use infra::InMemoryWalletRepository;
use uuid::Uuid;

#[tokio::test]
async fn wallet_transactions_are_tenant_scoped() {
    let repo = InMemoryWalletRepository::default();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    repo.get_or_create(tenant_a, "USD").await.unwrap();
    repo.get_or_create(tenant_b, "USD").await.unwrap();

    repo.credit(
        tenant_a,
        2_000_000,
        domain::WalletTransactionType::TopUp,
        "wallet-isolation",
        Some("tenant A only"),
    )
    .await
    .unwrap();

    let tenant_a_rows = repo.list_transactions(tenant_a, 100).await.unwrap();
    let tenant_b_rows = repo.list_transactions(tenant_b, 100).await.unwrap();

    assert_eq!(tenant_a_rows.len(), 1);
    assert!(tenant_b_rows.is_empty());
}
