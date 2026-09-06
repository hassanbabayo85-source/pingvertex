use crate::ports::InvoiceRepository;
use chrono::{DateTime, Utc};
use domain::{Invoice, InvoiceLine, InvoiceLineType, InvoiceStatus};
use rust_decimal::Decimal;
use std::{error::Error, fmt, sync::Arc};
use uuid::Uuid;
#[derive(Debug)]
pub enum GenerateInvoiceError {
    Invalid(String),
    Repository(Box<dyn Error + Send + Sync>),
}
impl fmt::Display for GenerateInvoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "{s}"),
            Self::Repository(e) => write!(f, "{e}"),
        }
    }
}
impl Error for GenerateInvoiceError {}
pub struct GenerateInvoiceUseCase {
    repo: Arc<dyn InvoiceRepository>,
}
impl GenerateInvoiceUseCase {
    pub fn new(repo: Arc<dyn InvoiceRepository>) -> Self {
        Self { repo }
    }
    pub async fn execute(
        &self,
        invoice_id: Uuid,
        tenant_id: Uuid,
        invoice_number: String,
        currency: String,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        lines: Vec<InvoiceLine>,
    ) -> Result<Invoice, GenerateInvoiceError> {
        if end <= start {
            return Err(GenerateInvoiceError::Invalid(
                "invoice period must end after it starts".into(),
            ));
        }
        if currency.trim().is_empty() {
            return Err(GenerateInvoiceError::Invalid(
                "invoice currency is required".into(),
            ));
        }
        for l in &lines {
            if l.invoice_id != invoice_id {
                return Err(GenerateInvoiceError::Invalid(
                    "invoice line invoice_id mismatch".into(),
                ));
            }
            if l.amount != (l.quantity * l.unit_price).round_dp(6) {
                return Err(GenerateInvoiceError::Invalid(
                    "invoice line amount is not explainable".into(),
                ));
            }
        }
        let subtotal = lines
            .iter()
            .filter(|l| l.line_type != InvoiceLineType::Tax)
            .map(|l| l.amount)
            .sum::<Decimal>();
        let tax = lines
            .iter()
            .filter(|l| l.line_type == InvoiceLineType::Tax)
            .map(|l| l.amount)
            .sum::<Decimal>();
        if tax < Decimal::ZERO {
            return Err(GenerateInvoiceError::Invalid(
                "invoice tax cannot be negative".into(),
            ));
        }
        let i = Invoice {
            id: invoice_id,
            tenant_id,
            invoice_number,
            currency,
            subtotal,
            tax,
            total: (subtotal + tax).round_dp(6),
            status: InvoiceStatus::Open,
            period_start: start,
            period_end: end,
            created_at: Utc::now(),
            paid_at: None,
        };
        self.repo
            .create_invoice(&i, &lines)
            .await
            .map_err(GenerateInvoiceError::Repository)?;
        Ok(i)
    }
}
