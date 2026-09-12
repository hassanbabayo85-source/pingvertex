use sqlx::PgPool;

#[derive(Clone)]
pub struct PostgresMonitorRepository {
    pool: PgPool,
}

impl PostgresMonitorRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
