use application::ports::*;
use async_trait::async_trait;
use domain::{Invoice, InvoiceLine};
use std::sync::{Arc, Mutex};
use uuid::Uuid;
type InvoiceStore = Vec<(Invoice, Vec<InvoiceLine>)>;

#[derive(Clone, Default)]
pub struct InMemoryInvoiceRepository {
    pub invoices: Arc<Mutex<InvoiceStore>>,
}
impl InMemoryInvoiceRepository {
    pub fn new() -> Self {
        Self::default()
    }
}
#[async_trait]
impl InvoiceRepository for InMemoryInvoiceRepository {
    async fn create_invoice(&self, i: &Invoice, l: &[InvoiceLine]) -> InvoiceRepoResult<()> {
        self.invoices.lock().unwrap().push((i.clone(), l.to_vec()));
        Ok(())
    }
    async fn list_by_tenant(&self, t: Uuid, n: u32) -> InvoiceRepoResult<Vec<Invoice>> {
        Ok(self
            .invoices
            .lock()
            .unwrap()
            .iter()
            .rev()
            .filter(|(i, _)| i.tenant_id == t)
            .take(n.min(100) as usize)
            .map(|(i, _)| i.clone())
            .collect())
    }
}
