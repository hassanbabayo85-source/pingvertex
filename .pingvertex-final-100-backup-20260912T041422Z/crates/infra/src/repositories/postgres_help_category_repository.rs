use application::ports::HelpCategoryRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::HelpCategory;
use domain::shared::ids::HelpCategoryId;
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresHelpCategoryRepository {
    pool: PgPool,
}

impl PostgresHelpCategoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct HelpCategoryRow {
    id: Uuid,
    name: String,
    slug: String,
    description: String,
    sort_order: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HelpCategoryRow {
    fn into_domain(self) -> HelpCategory {
        HelpCategory {
            id: HelpCategoryId(self.id),
            name: self.name,
            slug: self.slug,
            description: self.description,
            sort_order: self.sort_order,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[async_trait]
impl HelpCategoryRepository for PostgresHelpCategoryRepository {
    async fn create(&self, category: &HelpCategory) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO help_categories (id, name, slug, description, sort_order, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(category.id.0)
        .bind(&category.name)
        .bind(&category.slug)
        .bind(&category.description)
        .bind(category.sort_order)
        .bind(category.created_at)
        .bind(category.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: HelpCategoryId) -> RepoResult<Option<HelpCategory>> {
        let row = sqlx::query_as::<_, HelpCategoryRow>(
            "SELECT id, name, slug, description, sort_order, created_at, updated_at \
             FROM help_categories WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(HelpCategoryRow::into_domain))
    }

    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpCategory>> {
        let row = sqlx::query_as::<_, HelpCategoryRow>(
            "SELECT id, name, slug, description, sort_order, created_at, updated_at \
             FROM help_categories WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(HelpCategoryRow::into_domain))
    }

    async fn list_all(&self) -> RepoResult<Vec<HelpCategory>> {
        let rows = sqlx::query_as::<_, HelpCategoryRow>(
            "SELECT id, name, slug, description, sort_order, created_at, updated_at \
             FROM help_categories ORDER BY sort_order ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(HelpCategoryRow::into_domain).collect())
    }
}
