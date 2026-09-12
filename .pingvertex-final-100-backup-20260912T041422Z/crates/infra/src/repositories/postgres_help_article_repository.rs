use application::ports::HelpArticleRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::HelpArticle;
use domain::enums::HelpArticleStatus;
use domain::shared::ids::{HelpArticleId, HelpCategoryId};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct PostgresHelpArticleRepository {
    pool: PgPool,
}

impl PostgresHelpArticleRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct HelpArticleRow {
    id: Uuid,
    category_id: Uuid,
    title: String,
    slug: String,
    content: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HelpArticleRow {
    fn into_domain(self) -> Result<HelpArticle, Box<dyn Error + Send + Sync>> {
        let status: HelpArticleStatus = self
            .status
            .parse()
            .map_err(|e: String| std::io::Error::other(e))?;
        Ok(HelpArticle {
            id: HelpArticleId(self.id),
            category_id: HelpCategoryId(self.category_id),
            title: self.title,
            slug: self.slug,
            content: self.content,
            status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[async_trait]
impl HelpArticleRepository for PostgresHelpArticleRepository {
    async fn create(&self, article: &HelpArticle) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO help_articles (id, category_id, title, slug, content, status, created_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(article.id.0)
        .bind(article.category_id.0)
        .bind(&article.title)
        .bind(&article.slug)
        .bind(&article.content)
        .bind(article.status.as_str())
        .bind(article.created_at)
        .bind(article.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_slug(&self, slug: &str) -> RepoResult<Option<HelpArticle>> {
        let row = sqlx::query_as::<_, HelpArticleRow>(
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        row.map(HelpArticleRow::into_domain).transpose()
    }

    async fn list_by_category(
        &self,
        category_id: HelpCategoryId,
        only_published: bool,
    ) -> RepoResult<Vec<HelpArticle>> {
        let query = if only_published {
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE category_id = $1 AND status = 'published' ORDER BY created_at DESC"
        } else {
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE category_id = $1 ORDER BY created_at DESC"
        };
        let rows = sqlx::query_as::<_, HelpArticleRow>(query)
            .bind(category_id.0)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter().map(HelpArticleRow::into_domain).collect()
    }

    async fn list_published(&self) -> RepoResult<Vec<HelpArticle>> {
        let rows = sqlx::query_as::<_, HelpArticleRow>(
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE status = 'published' ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(HelpArticleRow::into_domain).collect()
    }

    async fn search(&self, query: &str) -> RepoResult<Vec<HelpArticle>> {
        let pattern = format!("%{}%", query);
        let rows = sqlx::query_as::<_, HelpArticleRow>(
            "SELECT id, category_id, title, slug, content, status, created_at, updated_at \
             FROM help_articles WHERE status = 'published' AND (title ILIKE $1 OR content ILIKE $1) ORDER BY created_at DESC",
        )
        .bind(pattern)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(HelpArticleRow::into_domain).collect()
    }
}
