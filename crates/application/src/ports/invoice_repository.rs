use async_trait::async_trait;
use domain::{Invoice, InvoiceLine};
use std::{error::Error, sync::Arc};
use uuid::Uuid;
pub type InvoiceRepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
#[async_trait]
pub trait InvoiceRepository: Send + Sync {
    async fn create_invoice(&self, i: &Invoice, lines: &[InvoiceLine]) -> InvoiceRepoResult<()>;
    async fn list_by_tenant(&self, t: Uuid, limit: u32) -> InvoiceRepoResult<Vec<Invoice>>;
    async fn find_by_id_for_tenant(&self, id: Uuid, tenant_id: Uuid) -> InvoiceRepoResult<Option<Invoice>>;
}
#[async_trait]
impl<T> InvoiceRepository for Arc<T>
where
    T: InvoiceRepository + Send + Sync + ?Sized,
{
    async fn create_invoice(&self, i: &Invoice, l: &[InvoiceLine]) -> InvoiceRepoResult<()> {
        (**self).create_invoice(i, l).await
    }
    async fn list_by_tenant(&self, t: Uuid, n: u32) -> InvoiceRepoResult<Vec<Invoice>> {
        (**self).list_by_tenant(t, n).await
    }
    async fn find_by_id_for_tenant(&self, id: Uuid, tenant_id: Uuid) -> InvoiceRepoResult<Option<Invoice>> {
        (**self).find_by_id_for_tenant(id, tenant_id).await
    }
}

