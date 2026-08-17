use std::collections::HashMap;
use std::error::Error;

use application::ports::MonitorRepository;
use async_trait::async_trait;
use domain::shared::ids::{AssertionId, MonitorId, UserId};
use domain::{
    Assertion, AssertionOperator, HttpMethod, Monitor, MonitorName, RegionCode, ResponsePart, Url,
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// PostgreSQL-backed monitor repository.
#[derive(Debug, Clone)]
pub struct PostgresMonitorRepository {
    pool: PgPool,
}

impl PostgresMonitorRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

//
// Monitor table row
//
#[derive(Debug, Clone, FromRow)]
struct MonitorRow {
    id: Uuid,
    user_id: Uuid,
    regions: Vec<String>,
    name: String,
    url: String,
    method: String,
    expected_status: i16,
    interval_seconds: i32,
    timeout_seconds: i32,
    enabled: bool,
    is_public: bool,
    ssl_check_enabled: bool,
    ssl_expiry_warning_days: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

//
// monitor_assertions table row
//
//
// monitor_assertions table row
//
#[derive(Debug, Clone, FromRow)]
struct AssertionRow {
    id: Uuid,
    monitor_id: Uuid,
    /// Loaded from the database so sqlx can map the query result.
    /// Ordering is handled by SQL (ORDER BY position), so this field is not read.
    #[expect(dead_code)]
    position: i32,
    response_part: String,
    operator: String,
    target: Option<String>,
    expected_value: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl MonitorRow {
    fn from_domain(monitor: &Monitor) -> Self {
        Self {
            id: monitor.id.0,
            user_id: monitor.user_id.0,
            regions: monitor.regions.iter().map(|r| r.to_string()).collect(),
            name: monitor.name.as_str().to_owned(),
            url: monitor.url.clone(),
            method: method_to_db(monitor.method),
            expected_status: i16::try_from(i64::from(monitor.expected_status))
                .expect("expected_status exceeds i16 capacity"),
            interval_seconds: i32::try_from(i64::from(monitor.interval_seconds))
                .expect("interval_seconds exceeds i32 capacity"),
            timeout_seconds: i32::try_from(i64::from(monitor.timeout_seconds))
                .expect("timeout_seconds exceeds i32 capacity"),
            enabled: monitor.enabled,
            is_public: monitor.is_public,
            ssl_check_enabled: monitor.ssl_check_enabled,
            ssl_expiry_warning_days: i32::try_from(i64::from(monitor.ssl_expiry_warning_days))
                .expect("ssl_expiry_warning_days exceeds i32 capacity"),
            created_at: monitor.created_at,
            updated_at: monitor.updated_at,
        }
    }

    fn into_domain(self, assertions: Vec<Assertion>) -> RepoResult<Monitor> {
        let method = method_from_db(&self.method)?;

        let regions = self
            .regions
            .iter()
            .map(|r| RegionCode::new(r.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Monitor {
            id: MonitorId(self.id),
            user_id: UserId(self.user_id),
            regions,
            name: MonitorName::new(self.name)?,
            url: Url::new(self.url)?.as_str().to_owned(),
            method,
            expected_status: self.expected_status.try_into()?,
            interval_seconds: self.interval_seconds.try_into()?,
            timeout_seconds: self.timeout_seconds.try_into()?,
            enabled: self.enabled,
            is_public: self.is_public,
            ssl_check_enabled: self.ssl_check_enabled,
            ssl_expiry_warning_days: self.ssl_expiry_warning_days.try_into()?,
            created_at: self.created_at,
            updated_at: self.updated_at,
            assertions,
        })
    }
}

impl AssertionRow {
    fn into_domain(self) -> RepoResult<Assertion> {
        Ok(Assertion {
            id: AssertionId(self.id),
            response_part: response_part_from_db(&self.response_part)?,
            operator: operator_from_db(&self.operator)?,
            target: self.target,
            expected_value: self.expected_value,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn method_to_db(method: HttpMethod) -> String {
    match method {
        HttpMethod::Get => "GET".to_string(),
        HttpMethod::Head => "HEAD".to_string(),
    }
}

fn method_from_db(value: &str) -> RepoResult<HttpMethod> {
    match value {
        "GET" => Ok(HttpMethod::Get),
        "HEAD" => Ok(HttpMethod::Head),
        other => Err(format!("unsupported http method in database: {other}").into()),
    }
}

fn response_part_to_db(part: &ResponsePart) -> String {
    match part {
        ResponsePart::StatusCode => "status_code".to_string(),
        ResponsePart::Headers => "headers".to_string(),
        ResponsePart::Body => "body".to_string(),
        ResponsePart::Json => "json".to_string(),
        ResponsePart::Latency => "latency".to_string(),
        ResponsePart::ContentType => "content_type".to_string(),
        ResponsePart::ContentLength => "content_length".to_string(),
        ResponsePart::Xml => "xml".to_string(),
        ResponsePart::XPath => "xpath".to_string(),
        ResponsePart::CssSelector => "css_selector".to_string(),
        ResponsePart::GraphQL => "graphql".to_string(),
        ResponsePart::GrpcStatusCode => "grpc_status_code".to_string(),
        ResponsePart::SslCertificate => "ssl_certificate".to_string(),
        ResponsePart::DnsRecord => "dns_record".to_string(),
        ResponsePart::TcpPort => "tcp_port".to_string(),
        ResponsePart::ProxyHealth => "proxy_health".to_string(),
    }
}

fn response_part_from_db(value: &str) -> RepoResult<ResponsePart> {
    match value {
        "status_code" => Ok(ResponsePart::StatusCode),
        "headers" => Ok(ResponsePart::Headers),
        "body" => Ok(ResponsePart::Body),
        "json" => Ok(ResponsePart::Json),
        "latency" => Ok(ResponsePart::Latency),
        "content_type" => Ok(ResponsePart::ContentType),
        "content_length" => Ok(ResponsePart::ContentLength),
        "xml" => Ok(ResponsePart::Xml),
        "xpath" => Ok(ResponsePart::XPath),
        "css_selector" => Ok(ResponsePart::CssSelector),
        "graphql" => Ok(ResponsePart::GraphQL),
        "grpc_status_code" => Ok(ResponsePart::GrpcStatusCode),
        "ssl_certificate" => Ok(ResponsePart::SslCertificate),
        "dns_record" => Ok(ResponsePart::DnsRecord),
        "tcp_port" => Ok(ResponsePart::TcpPort),
        "proxy_health" => Ok(ResponsePart::ProxyHealth),
        other => Err(format!("unsupported response part in database: {other}").into()),
    }
}

fn operator_to_db(operator: &AssertionOperator) -> String {
    match operator {
        AssertionOperator::Equals => "equals".to_string(),
        AssertionOperator::NotEquals => "not_equals".to_string(),
        AssertionOperator::GreaterThan => "greater_than".to_string(),
        AssertionOperator::GreaterThanOrEqual => "greater_than_or_equal".to_string(),
        AssertionOperator::LessThan => "less_than".to_string(),
        AssertionOperator::LessThanOrEqual => "less_than_or_equal".to_string(),
        AssertionOperator::Contains => "contains".to_string(),
        AssertionOperator::NotContains => "not_contains".to_string(),
        AssertionOperator::Exists => "exists".to_string(),
        AssertionOperator::RegexMatch => "regex_match".to_string(),
    }
}

fn operator_from_db(value: &str) -> RepoResult<AssertionOperator> {
    match value {
        "equals" => Ok(AssertionOperator::Equals),
        "not_equals" => Ok(AssertionOperator::NotEquals),
        "greater_than" => Ok(AssertionOperator::GreaterThan),
        "greater_than_or_equal" => Ok(AssertionOperator::GreaterThanOrEqual),
        "less_than" => Ok(AssertionOperator::LessThan),
        "less_than_or_equal" => Ok(AssertionOperator::LessThanOrEqual),
        "contains" => Ok(AssertionOperator::Contains),
        "not_contains" => Ok(AssertionOperator::NotContains),
        "exists" => Ok(AssertionOperator::Exists),
        "regex_match" => Ok(AssertionOperator::RegexMatch),
        other => Err(format!("unsupported assertion operator in database: {other}").into()),
    }
}

#[async_trait]
impl MonitorRepository for PostgresMonitorRepository {
    async fn save(&self, monitor: &Monitor) -> RepoResult<()> {
        let row = MonitorRow::from_domain(monitor);

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO monitors (
                id,
                user_id,
                regions,
                name,
                url,
                method,
                expected_status,
                interval_seconds,
                timeout_seconds,
                enabled,
                ssl_check_enabled,
                ssl_expiry_warning_days,
                created_at,
                updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                user_id = EXCLUDED.user_id,
                regions = EXCLUDED.regions,
                name = EXCLUDED.name,
                url = EXCLUDED.url,
                method = EXCLUDED.method,
                expected_status = EXCLUDED.expected_status,
                interval_seconds = EXCLUDED.interval_seconds,
                timeout_seconds = EXCLUDED.timeout_seconds,
                enabled = EXCLUDED.enabled,
                ssl_check_enabled = EXCLUDED.ssl_check_enabled,
                ssl_expiry_warning_days = EXCLUDED.ssl_expiry_warning_days,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.regions)
        .bind(row.name)
        .bind(row.url)
        .bind(row.method)
        .bind(row.expected_status)
        .bind(row.interval_seconds)
        .bind(row.timeout_seconds)
        .bind(row.enabled)
        .bind(row.ssl_check_enabled)
        .bind(row.ssl_expiry_warning_days)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM monitor_assertions
            WHERE monitor_id = $1
            "#,
        )
        .bind(monitor.id.0)
        .execute(&mut *tx)
        .await?;

        for (position, assertion) in monitor.assertions.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO monitor_assertions (
                    id,
                    monitor_id,
                    position,
                    response_part,
                    operator,
                    target,
                    expected_value,
                    created_at,
                    updated_at
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                "#,
            )
            .bind(assertion.id.0)
            .bind(monitor.id.0)
            .bind(position as i32)
            .bind(response_part_to_db(&assertion.response_part))
            .bind(operator_to_db(&assertion.operator))
            .bind(&assertion.target)
            .bind(&assertion.expected_value)
            .bind(assertion.created_at)
            .bind(assertion.updated_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    async fn update(&self, monitor: &Monitor) -> RepoResult<()> {
        self.save(monitor).await
    }

    async fn find_by_id(&self, id: MonitorId) -> RepoResult<Option<Monitor>> {
        let row = sqlx::query_as::<_, MonitorRow>(
            r#"
            SELECT
                id,
                user_id,
                regions,
                name,
                url,
                method,
                expected_status,
                interval_seconds,
                timeout_seconds,
                enabled,
                is_public,
                ssl_check_enabled,
                ssl_expiry_warning_days,
                created_at,
                updated_at
            FROM monitors
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let assertion_rows = sqlx::query_as::<_, AssertionRow>(
            r#"
            SELECT
                id,
                monitor_id,
                position,
                response_part,
                operator,
                target,
                expected_value,
                created_at,
                updated_at
            FROM monitor_assertions
            WHERE monitor_id = $1
            ORDER BY position
            "#,
        )
        .bind(id.0)
        .fetch_all(&self.pool)
        .await?;

        let assertions = assertion_rows
            .into_iter()
            .map(AssertionRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Some(row.into_domain(assertions)?))
    }

    async fn find_all(&self) -> RepoResult<Vec<Monitor>> {
        let monitor_rows = sqlx::query_as::<_, MonitorRow>(
            r#"
            SELECT
                id,
                user_id,
                regions,
                name,
                url,
                method,
                expected_status,
                interval_seconds,
                timeout_seconds,
                enabled,
                is_public,
                ssl_check_enabled,
                ssl_expiry_warning_days,
                created_at,
                updated_at
            FROM monitors
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        if monitor_rows.is_empty() {
            return Ok(Vec::new());
        }

        let assertion_rows = sqlx::query_as::<_, AssertionRow>(
            r#"
            SELECT
                id,
                monitor_id,
                position,
                response_part,
                operator,
                target,
                expected_value,
                created_at,
                updated_at
            FROM monitor_assertions
            ORDER BY monitor_id, position
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut assertions_by_monitor: HashMap<Uuid, Vec<Assertion>> = HashMap::new();
        for row in assertion_rows {
            let monitor_id = row.monitor_id;
            let domain_assertion = row.into_domain()?;
            assertions_by_monitor
                .entry(monitor_id)
                .or_default()
                .push(domain_assertion);
        }

        let mut monitors = Vec::with_capacity(monitor_rows.len());
        for row in monitor_rows {
            let monitor_id = row.id;
            let assertions = assertions_by_monitor
                .remove(&monitor_id)
                .unwrap_or_default();
            monitors.push(row.into_domain(assertions)?);
        }

        Ok(monitors)
    }

    async fn find_by_id_for_user(
        &self,
        id: MonitorId,
        user_id: UserId,
    ) -> RepoResult<Option<Monitor>> {
        let row = sqlx::query_as::<_, MonitorRow>("SELECT id, user_id, regions, name, url, method, expected_status, interval_seconds, timeout_seconds, enabled, is_public, ssl_check_enabled, ssl_expiry_warning_days, created_at, updated_at FROM monitors WHERE id = $1 AND user_id = $2")
            .bind(id.0).bind(user_id.0).fetch_optional(&self.pool).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let assertion_rows = sqlx::query_as::<_, AssertionRow>("SELECT id, monitor_id, position, response_part, operator, target, expected_value, created_at, updated_at FROM monitor_assertions WHERE monitor_id = $1 ORDER BY position")
            .bind(id.0).fetch_all(&self.pool).await?;
        let assertions = assertion_rows
            .into_iter()
            .map(AssertionRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;
        Ok(Some(row.into_domain(assertions)?))
    }

    async fn find_all_for_user(&self, user_id: UserId) -> RepoResult<Vec<Monitor>> {
        let monitor_rows = sqlx::query_as::<_, MonitorRow>("SELECT id, user_id, regions, name, url, method, expected_status, interval_seconds, timeout_seconds, enabled, is_public, ssl_check_enabled, ssl_expiry_warning_days, created_at, updated_at FROM monitors WHERE user_id = $1 ORDER BY created_at DESC")
            .bind(user_id.0).fetch_all(&self.pool).await?;
        if monitor_rows.is_empty() {
            return Ok(Vec::new());
        }
        let assertion_rows = sqlx::query_as::<_, AssertionRow>("SELECT id, monitor_id, position, response_part, operator, target, expected_value, created_at, updated_at FROM monitor_assertions ORDER BY monitor_id, position").fetch_all(&self.pool).await?;
        let mut assertions_by_monitor: HashMap<Uuid, Vec<Assertion>> = HashMap::new();
        for row in assertion_rows {
            assertions_by_monitor
                .entry(row.monitor_id)
                .or_default()
                .push(row.into_domain()?);
        }
        let mut monitors = Vec::with_capacity(monitor_rows.len());
        for row in monitor_rows {
            let assertions = assertions_by_monitor.remove(&row.id).unwrap_or_default();
            monitors.push(row.into_domain(assertions)?);
        }
        Ok(monitors)
    }

    async fn delete_for_user(&self, id: MonitorId, user_id: UserId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM monitors WHERE id = $1 AND user_id = $2")
            .bind(id.0)
            .bind(user_id.0)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_public(&self) -> RepoResult<Vec<Monitor>> {
        // Yana dawo da monitors da `is_public = true` KAWAI. Ba mu bukatar
        // assertions a nan domin sabis din public baya bukatar bayanan cikin
        // gida na assertion rules.
        let monitor_rows = sqlx::query_as::<_, MonitorRow>(
            "SELECT id, user_id, regions, name, url, method, expected_status, interval_seconds, \
             timeout_seconds, enabled, is_public, ssl_check_enabled, ssl_expiry_warning_days, created_at, updated_at \
             FROM monitors WHERE is_public = true ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut monitors = Vec::with_capacity(monitor_rows.len());
        for row in monitor_rows {
            monitors.push(row.into_domain(Vec::new())?);
        }

        Ok(monitors)
    }

    async fn delete(&self, id: MonitorId) -> RepoResult<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM monitors
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}
