use application::ports::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Invoice, InvoiceLine, InvoiceStatus};
use rust_decimal::Decimal;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
pub struct PostgresInvoiceRepository {
    pool: PgPool,
}
impl PostgresInvoiceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
#[derive(FromRow)]
struct Row {
    id: Uuid,
    tenant_id: Uuid,
    invoice_number: String,
    currency: String,
    subtotal: Decimal,
    tax: Decimal,
    total: Decimal,
    status: String,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    created_at: DateTime<Utc>,
    paid_at: Option<DateTime<Utc>>,
}
#[async_trait]
impl InvoiceRepository for PostgresInvoiceRepository {
    async fn create_invoice(&self, i: &Invoice, lines: &[InvoiceLine]) -> InvoiceRepoResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO invoices(id,tenant_id,invoice_number,currency,subtotal,tax,total,status,period_start,period_end,created_at,paid_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(i.id).bind(i.tenant_id).bind(&i.invoice_number).bind(&i.currency).bind(i.subtotal).bind(i.tax).bind(i.total).bind(i.status.as_str()).bind(i.period_start).bind(i.period_end).bind(i.created_at).bind(i.paid_at).execute(&mut *tx).await?;
        for l in lines {
            sqlx::query("INSERT INTO invoice_lines(id,invoice_id,line_type,description,quantity,unit_price,amount,source_type,source_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(l.id).bind(l.invoice_id).bind(l.line_type.as_str()).bind(&l.description).bind(l.quantity).bind(l.unit_price).bind(l.amount).bind(l.source_type.as_str()).bind(l.source_id).bind(l.created_at).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    async fn list_by_tenant(&self, t: Uuid, n: u32) -> InvoiceRepoResult<Vec<Invoice>> {
        let rows=sqlx::query_as::<_,Row>("SELECT id,tenant_id,invoice_number,currency,subtotal,tax,total,status,period_start,period_end,created_at,paid_at FROM invoices WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT $2").bind(t).bind(i64::from(n.min(100))).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| Invoice {
                id: r.id,
                tenant_id: r.tenant_id,
                invoice_number: r.invoice_number,
                currency: r.currency,
                subtotal: r.subtotal,
                tax: r.tax,
                total: r.total,
                status: r.status.parse().unwrap_or(InvoiceStatus::Open),
                period_start: r.period_start,
                period_end: r.period_end,
                created_at: r.created_at,
                paid_at: r.paid_at,
            })
            .collect())
    }
    async fn find_by_id_for_tenant(
        &self,
        id: Uuid,
        tenant_id: Uuid,
    ) -> InvoiceRepoResult<Option<Invoice>> {
        let r=sqlx::query_as::<_,Row>("SELECT id,tenant_id,invoice_number,currency,subtotal,tax,total,status,period_start,period_end,created_at,paid_at FROM invoices WHERE id=$1 AND tenant_id=$2")
            .bind(id).bind(tenant_id).fetch_optional(&self.pool).await?;
        Ok(r.map(|r| Invoice {
            id: r.id,
            tenant_id: r.tenant_id,
            invoice_number: r.invoice_number,
            currency: r.currency,
            subtotal: r.subtotal,
            tax: r.tax,
            total: r.total,
            status: r.status.parse().unwrap_or(InvoiceStatus::Open),
            period_start: r.period_start,
            period_end: r.period_end,
            created_at: r.created_at,
            paid_at: r.paid_at,
        }))
    }
    async fn update_status_for_tenant(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        status: InvoiceStatus,
        paid_at: Option<DateTime<Utc>>,
    ) -> InvoiceRepoResult<()> {
        let result = sqlx::query(
            "UPDATE invoices
             SET status = $1, paid_at = $2
             WHERE id = $3 AND tenant_id = $4",
        )
        .bind(status.as_str())
        .bind(paid_at)
        .bind(id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() != 1 {
            return Err(
                std::io::Error::new(std::io::ErrorKind::NotFound, "invoice not found").into(),
            );
        }

        Ok(())
    }
}
