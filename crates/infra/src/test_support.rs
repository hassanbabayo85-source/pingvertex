//! Test-support helpers shared by integration tests across crates.
//!
//! ## Design
//!
//! Each test binary gets its own **schema** within the shared test DB.
//! The schema name is derived from the binary path (via FNV-1a hash), so
//! binaries cannot collide. Every pooled connection has its `search_path`
//! set to that schema via an `after_connect` hook, so unqualified table
//! names resolve to the binary's own tables.
//!
//! Within a binary, tests that need a globally-empty schema call
//! `setup_pool_exclusive`, which acquires a session-scoped advisory lock
//! on a dedicated (non-pooled) connection and truncates every table in
//! the binary's schema. The lock is released the moment the guard drops.

use sqlx::{Connection, PgConnection, PgPool, postgres::PgPoolOptions};

fn test_schema_name() -> String {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    let mut hash: u64 = 0xcbf29ce484222325;
    for b in exe.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let safe: String = exe
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(40)
        .collect();
    format!("test_{safe}_{:016x}", hash)
}

/// Builds a pool whose every connection has `search_path` set to this
/// test binary's private schema. Runs migrations once. Idempotent.
pub async fn setup_pool(
    database_url: &str,
) -> Result<PgPool, Box<dyn std::error::Error + Send + Sync>> {
    let schema = test_schema_name();

    // Bootstrap: ensure the schema exists (via a plain connection — the
    // pool cannot be created with a search_path that doesn't exist yet).
    let mut bootstrap = PgConnection::connect(database_url).await?;
    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\""))
        .execute(&mut bootstrap)
        .await?;
    drop(bootstrap);

    // Pool with `after_connect` hook that sets search_path on every
    // connection. More reliable than `?options=` URL tricks.
    let schema_for_hook = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .after_connect(move |conn, _meta| {
            let schema = schema_for_hook.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET search_path TO \"{schema}\", public"))
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await?;

    ensure_test_schema(&pool).await?;
    Ok(pool)
}

/// Ensures migrations have been applied to this binary's schema.
pub async fn ensure_test_schema(
    pool: &PgPool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let schema = test_schema_name();

    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\""))
        .execute(pool)
        .await?;

    // Always run migrations. `sqlx::migrate!().run()` is idempotent:
    // it reads `_sqlx_migrations` and only applies migrations that have
    // not been recorded yet. The previous implementation skipped this
    // call entirely once `_sqlx_migrations` existed, which silently left
    // newly-added migration files un-applied on test schemas that had
    // already been bootstrapped by an earlier test-binary run.
    let mut conn = pool.acquire().await?;
    sqlx::query(&format!("SET search_path TO \"{schema}\", public"))
        .execute(&mut *conn)
        .await?;
    sqlx::migrate!("../../migrations").run(&mut *conn).await?;
    drop(conn);

    Ok(())
}

/// Truncates every table in this binary's schema (except `_sqlx_migrations`).
///
/// NOTE: PL/pgSQL `DO` blocks do NOT accept bind parameters (`$1` is
/// interpreted as a positional parameter placeholder, not a SQL value).
/// The schema name is interpolated via `format!` — safe because
/// `test_schema_name()` produces an identifier-safe ASCII string.
pub async fn truncate_all_test_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
    let schema = test_schema_name();

    sqlx::query(&format!(
        "DO $do$
        DECLARE r RECORD;
        BEGIN
          FOR r IN
            SELECT tablename FROM pg_tables
            WHERE schemaname = '{schema}'
              AND tablename <> '_sqlx_migrations'
          LOOP
            EXECUTE format('TRUNCATE TABLE %I.%I CASCADE',
                           '{schema}', r.tablename);
          END LOOP;
        END $do$;"
    ))
    .execute(pool)
    .await?;

    Ok(())
}

const EXCLUSIVE_TEST_LOCK_KEY: i64 = 0x70696E6776786578;

/// Holds a **dedicated** (non-pooled) connection on which the advisory
/// lock is held. The lock releases when this guard drops.
pub struct ExclusiveTestGuard {
    _conn: PgConnection,
}

/// Like `setup_pool`, but also acquires an exclusive advisory lock on a
/// dedicated connection, then truncates the schema. The lock is held
/// until the guard drops.
pub async fn setup_pool_exclusive(database_url: &str) -> (PgPool, ExclusiveTestGuard) {
    let schema = test_schema_name();

    // Acquire the exclusive lock BEFORE creating the pool or running
    // migrations. This prevents parallel tests in the same binary from
    // racing during schema creation/migration.
    let mut dedicated = PgConnection::connect(database_url)
        .await
        .expect("connect dedicated lock connection");

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(EXCLUSIVE_TEST_LOCK_KEY)
        .execute(&mut dedicated)
        .await
        .expect("acquire exclusive test lock");

    // Set search_path on the dedicated connection too.
    sqlx::query(&format!("SET search_path TO \"{schema}\", public"))
        .execute(&mut dedicated)
        .await
        .expect("set search_path on dedicated conn");

    // The lock is already held, so schema creation and migrations are now
    // serialized across tests in this binary.
    let pool = setup_pool(database_url).await.expect("setup test pool");

    truncate_all_test_tables(&pool)
        .await
        .expect("truncate test tables");

    (pool, ExclusiveTestGuard { _conn: dedicated })
}
