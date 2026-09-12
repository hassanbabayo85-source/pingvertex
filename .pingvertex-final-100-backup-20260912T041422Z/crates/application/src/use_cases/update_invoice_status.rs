use crate::ports::{FinancialAuditLogRepository, InvoiceRepository};
use chrono::Utc;
use domain::InvoiceStatus;
use std::{error::Error, sync::Arc};
use uuid::Uuid;

pub struct UpdateInvoiceStatusUseCase {
    invoice_repository: Arc<dyn InvoiceRepository>,
    audit_repository: Arc<dyn FinancialAuditLogRepository>,
}

impl UpdateInvoiceStatusUseCase {
    pub fn new(
        invoice_repository: Arc<dyn InvoiceRepository>,
        audit_repository: Arc<dyn FinancialAuditLogRepository>,
    ) -> Self {
        Self {
            invoice_repository,
            audit_repository,
        }
    }

    pub async fn mark_paid(
        &self,
        tenant_id: Uuid,
        invoice_id: Uuid,
        actor: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let invoice = self
            .invoice_repository
            .find_by_id_for_tenant(invoice_id, tenant_id)
            .await?
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "invoice not found")
            })?;

        if invoice.status == InvoiceStatus::Paid {
            return Ok(());
        }

        self.invoice_repository
            .update_status_for_tenant(invoice_id, tenant_id, InvoiceStatus::Paid, Some(Utc::now()))
            .await?;

        self.audit_repository
            .create(&domain::FinancialAuditLogEntry::new(
                actor,
                Some(tenant_id),
                "invoice_paid",
                "invoice",
                Some(invoice_id.to_string()),
                serde_json::json!({
                    "previous_status": invoice.status.as_str(),
                    "total": invoice.total.to_string(),
                    "currency": invoice.currency
                }),
            ))
            .await?;

        Ok(())
    }

    pub async fn void(
        &self,
        tenant_id: Uuid,
        invoice_id: Uuid,
        actor: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let invoice = self
            .invoice_repository
            .find_by_id_for_tenant(invoice_id, tenant_id)
            .await?
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "invoice not found")
            })?;

        if invoice.status == InvoiceStatus::Void {
            return Ok(());
        }

        self.invoice_repository
            .update_status_for_tenant(invoice_id, tenant_id, InvoiceStatus::Void, None)
            .await?;

        self.audit_repository
            .create(&domain::FinancialAuditLogEntry::new(
                actor,
                Some(tenant_id),
                "invoice_voided",
                "invoice",
                Some(invoice_id.to_string()),
                serde_json::json!({
                    "previous_status": invoice.status.as_str(),
                    "total": invoice.total.to_string(),
                    "currency": invoice.currency
                }),
            ))
            .await?;

        Ok(())
    }
}
