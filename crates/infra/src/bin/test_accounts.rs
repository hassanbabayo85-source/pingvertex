use argon2::{Argon2, PasswordHasher};
use password_hash::SaltString;
use rand_core::OsRng;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::env;
use uuid::Uuid;

const ACCOUNT_COUNT: u8 = 10;

fn require_enabled() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("ALLOW_TEST_ACCOUNTS").ok().as_deref() != Some("true") {
        return Err(
            "refusing test-account mutation: set ALLOW_TEST_ACCOUNTS=true explicitly".into(),
        );
    }
    Ok(())
}

fn email_for(n: u8) -> String {
    let domain = env::var("TEST_ACCOUNT_DOMAIN").unwrap_or_else(|_| "qa.pingvertex.test".into());
    format!("tester{n:02}@{domain}")
}

fn ttl_days() -> i64 {
    env::var("TEST_ACCOUNT_TTL_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30)
}

fn hash_password(password: &str) -> Result<String, Box<dyn std::error::Error>> {
    if password.len() < 12 || password.len() > 128 {
        return Err("TEST_ACCOUNT_PASSWORD must be 12..128 characters".into());
    }
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|err| format!("password hashing failed: {err:?}"))?;

    Ok(password_hash.to_string())
}

async fn ensure_one(
    pool: &PgPool,
    n: u8,
    password_hash: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let email = email_for(n);
    let username = format!("qa_tester_{n:02}");
    let mut tx = pool.begin().await?;
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users(id, username, email, password_hash, role, status, email_verified_at, created_at, updated_at) \
         VALUES ($1,$2,$3,$4,'user','active',now(),now(),now()) \
         ON CONFLICT(email) DO UPDATE SET password_hash=EXCLUDED.password_hash, status='active', email_verified_at=COALESCE(users.email_verified_at, now()), updated_at=now() \
         RETURNING id"
    )
    .bind(Uuid::new_v4()).bind(&username).bind(&email).bind(password_hash)
    .fetch_one(&mut *tx).await?;

    let tenant_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT tenant_id FROM tenant_memberships WHERE user_id=$1 AND role='Owner' ORDER BY joined_at NULLS LAST LIMIT 1"
    ).bind(user_id).fetch_optional(&mut *tx).await?;
    let tenant_id = match tenant_id {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO tenants(id,name,owner_user_id,created_at) VALUES ($1,$2,$3,now())",
            )
            .bind(id)
            .bind(format!("QA Tester {n:02} workspace"))
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("INSERT INTO tenant_memberships(tenant_id,user_id,role,invited_at,joined_at) VALUES ($1,$2,'Owner',now(),now())")
                .bind(id).bind(user_id).execute(&mut *tx).await?;
            id
        }
    };

    sqlx::query(
        "INSERT INTO test_access_grants(tenant_id,plan_tier,enabled,expires_at,created_at,updated_at) \
         VALUES ($1,'enterprise',TRUE,now()+make_interval(days => $2::int),now(),now()) \
         ON CONFLICT(tenant_id) DO UPDATE SET plan_tier='enterprise',enabled=TRUE,expires_at=EXCLUDED.expires_at,updated_at=now()"
    ).bind(tenant_id).bind(ttl_days() as i32).execute(&mut *tx).await?;
    tx.commit().await?;
    println!("enabled tester{n:02}: {email}");
    Ok(())
}

async fn set_enabled(
    pool: &PgPool,
    n: u8,
    enabled: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !(1..=ACCOUNT_COUNT).contains(&n) {
        return Err("tester number must be 1..10".into());
    }
    let email = email_for(n);
    let mut tx = pool.begin().await?;
    let status = if enabled { "active" } else { "suspended" };
    let user_id: Option<Uuid> = sqlx::query_scalar(
        "UPDATE users SET status=$2,updated_at=now() WHERE email=$1 RETURNING id",
    )
    .bind(&email)
    .bind(status)
    .fetch_optional(&mut *tx)
    .await?;
    let user_id = user_id.ok_or("test account not found; run seed first")?;
    sqlx::query(
        "UPDATE test_access_grants g SET enabled=$2, expires_at=CASE WHEN $2 THEN now()+make_interval(days => $3::int) ELSE expires_at END, updated_at=now() \
         FROM tenant_memberships tm WHERE tm.user_id=$1 AND tm.tenant_id=g.tenant_id"
    ).bind(user_id).bind(enabled).bind(ttl_days() as i32).execute(&mut *tx).await?;
    tx.commit().await?;
    println!(
        "{} tester{n:02}: {email}",
        if enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

async fn status(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    for n in 1..=ACCOUNT_COUNT {
        let email = email_for(n);
        let row: Option<(String, Option<bool>, Option<chrono::DateTime<chrono::Utc>>)> =
            sqlx::query_as(
                "SELECT u.status, g.enabled, g.expires_at FROM users u \
             LEFT JOIN tenant_memberships tm ON tm.user_id=u.id AND tm.role='Owner' \
             LEFT JOIN test_access_grants g ON g.tenant_id=tm.tenant_id WHERE u.email=$1 LIMIT 1",
            )
            .bind(&email)
            .fetch_optional(pool)
            .await?;
        match row {
            Some((s, e, x)) => println!(
                "tester{n:02}  {email:<38} user={s:<10} grant={} expires={}",
                e.unwrap_or(false),
                x.map(|v| v.to_rfc3339()).unwrap_or_else(|| "-".into())
            ),
            None => println!("tester{n:02}  {email:<38} not-created"),
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str).unwrap_or("status") {
        "status" => status(&pool).await?,
        "seed" => {
            require_enabled()?;
            let password = env::var("TEST_ACCOUNT_PASSWORD")
                .map_err(|_| "TEST_ACCOUNT_PASSWORD is required for seed")?;
            for n in 1..=ACCOUNT_COUNT {
                ensure_one(&pool, n, &hash_password(&password)?).await?;
            }
            println!(
                "seeded 10 QA accounts; full test access expires in {} day(s)",
                ttl_days()
            );
        }
        "enable" | "disable" => {
            require_enabled()?;
            let enabled = args[1] == "enable";
            let target = args
                .get(2)
                .ok_or("usage: test_accounts enable|disable <1..10|all>")?;
            if target == "all" {
                for n in 1..=ACCOUNT_COUNT {
                    set_enabled(&pool, n, enabled).await?;
                }
            } else {
                set_enabled(&pool, target.parse()?, enabled).await?;
            }
        }
        _ => {
            return Err(
                "usage: test_accounts [status|seed|enable <1..10|all>|disable <1..10|all>]".into(),
            );
        }
    }
    Ok(())
}
