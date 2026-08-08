use application::ports::UserRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Email, PasswordHash, User, UserBuilder, UserId, UserRole, UserStatus, Username};
use sqlx::{FromRow, PgPool};
use std::error::Error;
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// PostgreSQL-backed user repository.
#[derive(Debug, Clone)]
pub struct PostgresUserRepository {
    pool: PgPool,
}

impl PostgresUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone, FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: String,
    password_hash: String,
    role: String,
    status: String,
    email_verified_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl UserRow {
    fn from_domain(user: &User) -> Self {
        Self {
            id: user.id().into_uuid(),
            username: user.username().as_str().to_owned(),
            email: user.email().as_str().to_owned(),
            password_hash: user.password_hash().as_str().to_owned(),
            role: user.role().to_string(),
            status: user.status().to_string(),
            email_verified_at: user.email_verified_at(),
            created_at: user.created_at(),
            updated_at: user.updated_at(),
        }
    }

    fn into_domain(self) -> RepoResult<User> {
        let role = match self.role.as_str() {
            "admin" => UserRole::Admin,
            _ => UserRole::User,
        };
        let status = match self.status.as_str() {
            "active" => UserStatus::Active,
            "suspended" => UserStatus::Suspended,
            "deleted" => UserStatus::Deleted,
            _ => UserStatus::PendingVerification,
        };
        Ok(UserBuilder::new()
            .id(UserId::from_uuid(self.id))
            .username(Username::new(self.username)?)
            .email(Email::new(self.email)?)
            .password_hash(PasswordHash::new(self.password_hash)?)
            .role(role)
            .status(status)
            .email_verified_at(self.email_verified_at)
            .created_at(self.created_at)
            .updated_at(self.updated_at)
            .build()?)
    }
}

#[async_trait]
impl UserRepository for PostgresUserRepository {
    async fn create(&self, user: &User) -> RepoResult<()> {
        let row = UserRow::from_domain(user);
        sqlx::query(
            r#"
            INSERT INTO users (
                id,
                username,
                email,
                password_hash,
                role,
                status,
                email_verified_at,
                created_at,
                updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            "#,
        )
        .bind(row.id)
        .bind(row.username)
        .bind(row.email)
        .bind(row.password_hash)
        .bind(row.role)
        .bind(row.status)
        .bind(row.email_verified_at)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, user: &User) -> RepoResult<()> {
        let row = UserRow::from_domain(user);

        sqlx::query(
            r#"
            UPDATE users
            SET
                username = $2,
                email = $3,
                password_hash = $4,
                role = $5,
                status = $6,
                email_verified_at = $7,
                updated_at = $8
            WHERE id = $1
            "#,
        )
        .bind(row.id)
        .bind(row.username)
        .bind(row.email)
        .bind(row.password_hash)
        .bind(row.role)
        .bind(row.status)
        .bind(row.email_verified_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> RepoResult<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT
                id,
                username,
                email,
                password_hash,
                role,
                status,
                email_verified_at,
                created_at,
                updated_at
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(row.into_domain()?)),
            None => Ok(None),
        }
    }

    async fn find_by_email(&self, email: &Email) -> RepoResult<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT
                id,
                username,
                email,
                password_hash,
                role,
                status,
                email_verified_at,
                created_at,
                updated_at
            FROM users
            WHERE email = $1
            "#,
        )
        .bind(email.as_str())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(row.into_domain()?)),
            None => Ok(None),
        }
    }

    async fn find_by_username(&self, username: &Username) -> RepoResult<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            SELECT
                id,
                username,
                email,
                password_hash,
                role,
                status,
                email_verified_at,
                created_at,
                updated_at
            FROM users
            WHERE username = $1
            "#,
        )
        .bind(username.as_str())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(row.into_domain()?)),
            None => Ok(None),
        }
    }

    async fn delete(&self, id: &UserId) -> RepoResult<()> {
        sqlx::query(
            r#"
            DELETE FROM users
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn email_exists(&self, email: &Email) -> RepoResult<bool> {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM users
                WHERE email = $1
            )
            "#,
        )
        .bind(email.as_str())
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    async fn username_exists(&self, username: &Username) -> RepoResult<bool> {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM users
                WHERE username = $1
            )
            "#,
        )
        .bind(username.as_str())
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }
}
