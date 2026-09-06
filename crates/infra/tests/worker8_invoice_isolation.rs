use application::ports::InvoiceRepository;
use chrono::Utc;
use domain::{Invoice, InvoiceStatus};
use infra::InMemoryInvoiceRepository;
use rust_decimal::Decimal;
use uuid::Uuid;

#[tokio::test]
async fn invoice_lookup_requires_authenticated_tenant() {
    let repo=InMemoryInvoiceRepository::new();
    let tenant_a=Uuid::new_v4(); let tenant_b=Uuid::new_v4(); let id=Uuid::new_v4();
    repo.create_invoice(&Invoice{id,tenant_id:tenant_a,invoice_number:"INV-1".into(),currency:"USD".into(),subtotal:Decimal::ONE,tax:Decimal::ZERO,total:Decimal::ONE,status:InvoiceStatus::Open,period_start:Utc::now(),period_end:Utc::now()+chrono::Duration::days(30),created_at:Utc::now(),paid_at:None},&[]).await.unwrap();
    assert!(repo.find_by_id_for_tenant(id,tenant_b).await.unwrap().is_none());
    assert!(repo.find_by_id_for_tenant(id,tenant_a).await.unwrap().is_some());
}

