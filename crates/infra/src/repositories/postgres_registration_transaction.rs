use std::error::Error;

use application::ports::RegistrationTransaction;
use async_trait::async_trait;
use domain::{Tenant, TenantMembership, User, shared::types::Role};
use sqlx::PgPool;

type TxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn role_to_db(role: &Role) -> &'static str {
    match role {
        Role::Operator => "Operator",
        Role::Admin => "Admin",
        Role::Owner => "Owner",
        Role::Auditor => "Auditor",
        Role::Viewer => "Viewer",
    }
}

/// Postgres implementation na `RegistrationTransaction`.
///
/// Yana ajiye User + Tenant + TenantMembership a cikin `sqlx::Transaction`
/// GUDA ƊAYA: idan ko ɗaya daga cikin INSERT ɗin nan uku ya gaza (misali,
/// database connection ya faɗi tsakiyar aiki), `?` operator zai dawo da
/// Error kafin a kai ga `tx.commit()`. Lokacin da `tx` ya fita daga scope
/// ba tare da an yi `commit()` ba, sqlx yana yin ROLLBACK ta atomatik —
/// wato ba za a taɓa samun "orphaned user" mara Tenant ba.
#[derive(Debug, Clone)]
pub struct PostgresRegistrationTransaction {
    pool: PgPool,
}

impl PostgresRegistrationTransaction {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RegistrationTransaction for PostgresRegistrationTransaction {
    async fn register_user_with_tenant(
        &self,
        user: &User,
        tenant: &Tenant,
        membership: &TenantMembership,
    ) -> TxResult<()> {
        let mut tx = self.pool.begin().await?;

        // 1. users — SQL ɗin daidai yake da PostgresUserRepository::create
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
                totp_secret,
                totp_enabled,
                created_at,
                updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
            "#,
        )
        .bind(user.id().into_uuid())
        .bind(user.username().as_str())
        .bind(user.email().as_str())
        .bind(user.password_hash().as_str())
        .bind(user.role().to_string())
        .bind(user.status().to_string())
        .bind(user.email_verified_at())
        .bind(user.totp_secret().cloned())
        .bind(user.totp_enabled())
        .bind(user.created_at())
        .bind(user.updated_at())
        .execute(&mut *tx)
        .await?;

        // 2. tenants — SQL ɗin daidai yake da PostgresTenantRepository::create
        sqlx::query(
            "INSERT INTO tenants (id, name, owner_user_id, created_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(tenant.id.0)
        .bind(tenant.name.clone())
        .bind(tenant.owner_user_id.0)
        .bind(tenant.created_at)
        .execute(&mut *tx)
        .await?;

        // 3. tenant_memberships — SQL ɗin daidai yake da
        //    PostgresTenantMembershipRepository::create
        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role, invited_at, joined_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(membership.tenant_id.0)
        .bind(membership.user_id.0)
        .bind(role_to_db(&membership.role))
        .bind(membership.invited_at)
        .bind(membership.joined_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }
}
