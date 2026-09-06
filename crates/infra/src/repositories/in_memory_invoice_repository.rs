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

    async fn find_by_id_for_tenant(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> InvoiceRepoResult<Option<Invoice>> {
        Ok(self
            .invoices
            .lock()
            .unwrap()
            .iter()
            .find(|(i, _)| i.id == id && i.tenant_id == tenant_id)
            .map(|(i, _)| i.clone()))
    }

    async fn update_status_for_tenant(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        status: domain::InvoiceStatus,
        paid_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> InvoiceRepoResult<()> {
        let mut invoices = self.invoices.lock().unwrap();

        let invoice = invoices
            .iter_mut()
            .find(|(i, _)| i.id == id && i.tenant_id == tenant_id)
            .map(|(i, _)| i)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "invoice not found")
            })?;

        invoice.status = status;
        invoice.paid_at = paid_at;

        Ok(())
    }
}
