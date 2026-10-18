use application::pagination::{Cursor, Page, PageRequest};
use application::ports::SupportTicketRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{SupportMessage, SupportTicket};
use domain::enums::{SupportCategory, SupportTicketStatus};
use domain::shared::ids::{SupportMessageId, SupportTicketId, TenantId, UserId};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresSupportTicketRepository {
    pool: PgPool,
}

impl PostgresSupportTicketRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct SupportTicketRow {
    id: Uuid,
    ticket_number: String,
    tenant_id: Uuid,
    created_by: Uuid,
    subject: String,
    category: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SupportTicketRow {
    fn into_domain(self) -> Result<SupportTicket, Box<dyn Error + Send + Sync>> {
        let category: SupportCategory = self
            .category
            .parse()
            .map_err(|e: String| std::io::Error::other(e))?;
        let status: SupportTicketStatus = self
            .status
            .parse()
            .map_err(|e: String| std::io::Error::other(e))?;
        Ok(SupportTicket {
            id: SupportTicketId(self.id),
            ticket_number: self.ticket_number,
            tenant_id: TenantId(self.tenant_id),
            created_by: UserId(self.created_by),
            subject: self.subject,
            category,
            status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct SupportMessageRow {
    id: Uuid,
    ticket_id: Uuid,
    author_id: Uuid,
    message: String,
    created_at: DateTime<Utc>,
}

impl SupportMessageRow {
    fn into_domain(self) -> SupportMessage {
        SupportMessage {
            id: SupportMessageId(self.id),
            ticket_id: SupportTicketId(self.ticket_id),
            author_id: UserId(self.author_id),
            message: self.message,
            created_at: self.created_at,
        }
    }
}

#[async_trait]
impl SupportTicketRepository for PostgresSupportTicketRepository {
    async fn create_ticket(&self, ticket: &SupportTicket) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO support_tickets (id, ticket_number, tenant_id, created_by, subject, category, status, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(ticket.id.0)
        .bind(&ticket.ticket_number)
        .bind(ticket.tenant_id.0)
        .bind(ticket.created_by.0)
        .bind(&ticket.subject)
        .bind(ticket.category.as_str())
        .bind(ticket.status.as_str())
        .bind(ticket.created_at)
        .bind(ticket.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_ticket(&self, id: SupportTicketId) -> RepoResult<Option<SupportTicket>> {
        let row = sqlx::query_as::<_, SupportTicketRow>(
            "SELECT id, ticket_number, tenant_id, created_by, subject, category, status, created_at, updated_at \
             FROM support_tickets WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(SupportTicketRow::into_domain).transpose()
    }

    async fn list_user_tickets_paginated(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
        page: PageRequest,
    ) -> RepoResult<Page<SupportTicket>> {
        // SQL-level cursor pagination.
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, ticket_number, tenant_id, created_by, subject, category, status, \
                    created_at, updated_at \
             FROM support_tickets \
             WHERE tenant_id = ",
        );
        qb.push_bind(tenant_id.0);
        qb.push(" AND created_by = ").push_bind(user_id.0);
        if let Some(c) = page.cursor {
            qb.push(" AND (created_at < ")
                .push_bind(c.created_at)
                .push(" OR (created_at = ")
                .push_bind(c.created_at)
                .push(" AND id < ")
                .push_bind(c.id)
                .push("))");
        }
        qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind(page.fetch_size());

        let rows = qb
            .build_query_as::<SupportTicketRow>()
            .fetch_all(&self.pool)
            .await?;

        let items: Vec<SupportTicket> = rows
            .into_iter()
            .map(SupportTicketRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Page::from_rows(items, page.limit, |t| {
            Cursor::new(t.created_at, t.id.0)
        }))
    }

    async fn list_tenant_tickets_paginated(
        &self,
        tenant_id: TenantId,
        page: PageRequest,
    ) -> RepoResult<Page<SupportTicket>> {
        // SQL-level cursor pagination.
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, ticket_number, tenant_id, created_by, subject, category, status, \
                    created_at, updated_at \
             FROM support_tickets WHERE tenant_id = ",
        );
        qb.push_bind(tenant_id.0);
        if let Some(c) = page.cursor {
            qb.push(" AND (created_at < ")
                .push_bind(c.created_at)
                .push(" OR (created_at = ")
                .push_bind(c.created_at)
                .push(" AND id < ")
                .push_bind(c.id)
                .push("))");
        }
        qb.push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind(page.fetch_size());

        let rows = qb
            .build_query_as::<SupportTicketRow>()
            .fetch_all(&self.pool)
            .await?;

        let items: Vec<SupportTicket> = rows
            .into_iter()
            .map(SupportTicketRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Page::from_rows(items, page.limit, |t| {
            Cursor::new(t.created_at, t.id.0)
        }))
    }

    async fn list_user_tickets(
        &self,
        tenant_id: TenantId,
        user_id: UserId,
    ) -> RepoResult<Vec<SupportTicket>> {
        let rows = sqlx::query_as::<_, SupportTicketRow>(
            "SELECT id, ticket_number, tenant_id, created_by, subject, category, status, created_at, updated_at \
             FROM support_tickets WHERE tenant_id = $1 AND created_by = $2 ORDER BY created_at DESC",
        )
        .bind(tenant_id.0)
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(SupportTicketRow::into_domain)
            .collect()
    }

    async fn list_tenant_tickets(&self, tenant_id: TenantId) -> RepoResult<Vec<SupportTicket>> {
        let rows = sqlx::query_as::<_, SupportTicketRow>(
            "SELECT id, ticket_number, tenant_id, created_by, subject, category, status, created_at, updated_at \
             FROM support_tickets WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(SupportTicketRow::into_domain)
            .collect()
    }

    async fn create_message(&self, message: &SupportMessage) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO support_messages (id, ticket_id, author_id, message, created_at) \
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(message.id.0)
        .bind(message.ticket_id.0)
        .bind(message.author_id.0)
        .bind(&message.message)
        .bind(message.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_messages(&self, ticket_id: SupportTicketId) -> RepoResult<Vec<SupportMessage>> {
        let rows = sqlx::query_as::<_, SupportMessageRow>(
            "SELECT id, ticket_id, author_id, message, created_at \
             FROM support_messages WHERE ticket_id = $1 ORDER BY created_at ASC",
        )
        .bind(ticket_id.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(SupportMessageRow::into_domain)
            .collect())
    }

    async fn update_ticket_status(
        &self,
        id: SupportTicketId,
        status: SupportTicketStatus,
    ) -> RepoResult<()> {
        sqlx::query("UPDATE support_tickets SET status = $2, updated_at = now() WHERE id = $1")
            .bind(id.0)
            .bind(status.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
