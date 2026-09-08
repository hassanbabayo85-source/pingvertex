use std::collections::HashMap;
use std::error::Error;

use application::ports::MonitorRepository;
use application::ports::monitor_repository::QuotaCheckOutcome;
use async_trait::async_trait;
use domain::shared::ids::{AssertionId, MonitorId, TenantId, UserId};
use domain::{
    Assertion, AssertionOperator, HttpMethod, Monitor, MonitorKind, MonitorName, RegionCode,
    ResponsePart, Url,
};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const MONITOR_COLUMNS: &str = "id, user_id, tenant_id, regions, name, url, method, expected_status, \
    interval_seconds, timeout_seconds, enabled, is_public, ssl_check_enabled, \
    ssl_expiry_warning_days, kind, tcp_port, dns_record_type, dns_expected_value, \
    heartbeat_token, heartbeat_interval_seconds, heartbeat_grace_period_seconds, \
    last_heartbeat_at, created_at, updated_at";

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
    tenant_id: Uuid,
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
    kind: String,
    tcp_port: Option<i16>,
    dns_record_type: Option<String>,
    dns_expected_value: Option<String>,
    heartbeat_token: Option<String>,
    heartbeat_interval_seconds: Option<i32>,
    heartbeat_grace_period_seconds: Option<i32>,
    last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

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
    fn from_domain(monitor: &Monitor) -> RepoResult<Self> {
        Ok(Self {
            id: monitor.id.0,
            user_id: monitor.user_id.0,
            tenant_id: monitor.tenant_id.0,
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
            kind: kind_to_db(monitor.kind),
            tcp_port: monitor
                .tcp_port
                .map(|p| i16::try_from(p).expect("tcp_port exceeds i16 capacity")),
            dns_record_type: monitor.dns_record_type.clone(),
            dns_expected_value: monitor.dns_expected_value.clone(),
            heartbeat_token: monitor
                .heartbeat_token
                .as_ref()
                .map(|t| t.as_str().to_owned()),
            heartbeat_interval_seconds: monitor.heartbeat_interval_seconds.map(|v| {
                i32::try_from(v).expect("heartbeat_interval_seconds exceeds i32 capacity")
            }),
            heartbeat_grace_period_seconds: monitor.heartbeat_grace_period_seconds.map(|v| {
                i32::try_from(v).expect("heartbeat_grace_period_seconds exceeds i32 capacity")
            }),
            last_heartbeat_at: monitor.last_heartbeat_at,
            created_at: monitor.created_at,
            updated_at: monitor.updated_at,
        })
    }

    fn into_domain(self, assertions: Vec<Assertion>) -> RepoResult<Monitor> {
        let method = method_from_db(&self.method)?;
        let kind = kind_from_db(&self.kind)?;

        let regions = self
            .regions
            .iter()
            .map(|r| RegionCode::new(r.clone()))
            .collect::<Result<Vec<_>, _>>()?;

        let heartbeat_token = self
            .heartbeat_token
            .map(domain::HeartbeatToken::new)
            .transpose()?;

        Ok(Monitor {
            id: MonitorId(self.id),
            user_id: UserId(self.user_id),
            tenant_id: TenantId(self.tenant_id),
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
            kind,
            tcp_port: self.tcp_port.map(|p| p as u16),
            dns_record_type: self.dns_record_type,
            dns_expected_value: self.dns_expected_value,
            heartbeat_token,
            heartbeat_interval_seconds: self.heartbeat_interval_seconds.map(|v| v as u32),
            heartbeat_grace_period_seconds: self.heartbeat_grace_period_seconds.map(|v| v as u32),
            last_heartbeat_at: self.last_heartbeat_at,
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

fn kind_to_db(kind: MonitorKind) -> String {
    match kind {
        MonitorKind::Http => "http".to_string(),
        MonitorKind::Tcp => "tcp".to_string(),
        MonitorKind::Dns => "dns".to_string(),
        MonitorKind::Heartbeat => "heartbeat".to_string(),
        MonitorKind::Flow => "flow".to_string(),
    }
}

fn kind_from_db(value: &str) -> RepoResult<MonitorKind> {
    match value {
        "http" => Ok(MonitorKind::Http),
        "tcp" => Ok(MonitorKind::Tcp),
        "dns" => Ok(MonitorKind::Dns),
        "heartbeat" => Ok(MonitorKind::Heartbeat),
        "flow" => Ok(MonitorKind::Flow),
        other => Err(format!("unsupported monitor kind in database: {other}").into()),
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

impl PostgresMonitorRepository {
    /// Inserts (or, on a primary-key conflict, fully replaces) `monitor`
    /// and its assertions, within an already-open transaction.
    ///
    /// Factored out of `save` so that both the plain `save` path and the
    /// new `save_if_within_quota` path (see below) share this exact SQL
    /// instead of it being duplicated — `save_if_within_quota` needs to
    /// run the same insert logic but only after an in-transaction quota
    /// check, inside the same transaction so the check and the write are
    /// atomic together.
    ///
    /// NOTE (observed, not touched — out of B13's scope): this INSERT's
    /// column list does not include `is_public`, even though `MonitorRow`
    /// carries it and `MONITOR_COLUMNS`/`SELECT` reads it back. That
    /// predates this worker and is unrelated to quota atomicity; flagging
    /// only so it isn't mistaken for something this change introduced.
    async fn insert_or_replace(
        tx: &mut Transaction<'_, Postgres>,
        monitor: &Monitor,
    ) -> RepoResult<()> {
        let row = MonitorRow::from_domain(monitor)?;

        sqlx::query(
            r#"
            INSERT INTO monitors (
                id,
                user_id,
                tenant_id,
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
                kind,
                tcp_port,
                dns_record_type,
                dns_expected_value,
                heartbeat_token,
                heartbeat_interval_seconds,
                heartbeat_grace_period_seconds,
                last_heartbeat_at,
                created_at,
                updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)
            ON CONFLICT (id) DO UPDATE SET
                user_id = EXCLUDED.user_id,
                tenant_id = EXCLUDED.tenant_id,
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
                kind = EXCLUDED.kind,
                tcp_port = EXCLUDED.tcp_port,
                dns_record_type = EXCLUDED.dns_record_type,
                dns_expected_value = EXCLUDED.dns_expected_value,
                heartbeat_token = EXCLUDED.heartbeat_token,
                heartbeat_interval_seconds = EXCLUDED.heartbeat_interval_seconds,
                heartbeat_grace_period_seconds = EXCLUDED.heartbeat_grace_period_seconds,
                last_heartbeat_at = EXCLUDED.last_heartbeat_at,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.tenant_id)
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
        .bind(row.kind)
        .bind(row.tcp_port)
        .bind(row.dns_record_type)
        .bind(row.dns_expected_value)
        .bind(row.heartbeat_token)
        .bind(row.heartbeat_interval_seconds)
        .bind(row.heartbeat_grace_period_seconds)
        .bind(row.last_heartbeat_at)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            r#"
            DELETE FROM monitor_assertions
            WHERE monitor_id = $1
            "#,
        )
        .bind(monitor.id.0)
        .execute(&mut **tx)
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
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }
}

#[async_trait]
impl MonitorRepository for PostgresMonitorRepository {
    async fn save(&self, monitor: &Monitor) -> RepoResult<()> {
        let mut tx = self.pool.begin().await?;
        Self::insert_or_replace(&mut tx, monitor).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn update(&self, monitor: &Monitor) -> RepoResult<()> {
        self.save(monitor).await
    }

    async fn find_by_id(&self, id: MonitorId) -> RepoResult<Option<Monitor>> {
        let query = format!("SELECT {MONITOR_COLUMNS} FROM monitors WHERE id = $1");
        let row = sqlx::query_as::<_, MonitorRow>(&query)
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
        let query = format!("SELECT {MONITOR_COLUMNS} FROM monitors ORDER BY created_at DESC");
        let monitor_rows = sqlx::query_as::<_, MonitorRow>(&query)
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

    async fn find_all_for_tenant(&self, tenant_id: TenantId) -> RepoResult<Vec<Monitor>> {
        let query = format!(
            "SELECT {MONITOR_COLUMNS} FROM monitors WHERE tenant_id = $1 ORDER BY created_at DESC"
        );
        let monitor_rows = sqlx::query_as::<_, MonitorRow>(&query)
            .bind(tenant_id.0)
            .fetch_all(&self.pool)
            .await?;

        if monitor_rows.is_empty() {
            return Ok(Vec::new());
        }

        let assertion_rows = sqlx::query_as::<_, AssertionRow>(
            r#"
            SELECT
                monitor_assertions.id,
                monitor_assertions.monitor_id,
                monitor_assertions.position,
                monitor_assertions.response_part,
                monitor_assertions.operator,
                monitor_assertions.target,
                monitor_assertions.expected_value,
                monitor_assertions.created_at,
                monitor_assertions.updated_at
            FROM monitor_assertions
            INNER JOIN monitors ON monitors.id = monitor_assertions.monitor_id
            WHERE monitors.tenant_id = $1
            ORDER BY monitor_assertions.monitor_id, monitor_assertions.position
            "#,
        )
        .bind(tenant_id.0)
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
        let query =
            format!("SELECT {MONITOR_COLUMNS} FROM monitors WHERE id = $1 AND user_id = $2");
        let row = sqlx::query_as::<_, MonitorRow>(&query)
            .bind(id.0)
            .bind(user_id.0)
            .fetch_optional(&self.pool)
            .await?;
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
        let query = format!(
            "SELECT {MONITOR_COLUMNS} FROM monitors WHERE user_id = $1 ORDER BY created_at DESC"
        );
        let monitor_rows = sqlx::query_as::<_, MonitorRow>(&query)
            .bind(user_id.0)
            .fetch_all(&self.pool)
            .await?;
        if monitor_rows.is_empty() {
            return Ok(Vec::new());
        }
        // SECURITY/PERF: scoped to this user's own monitors via the JOIN
        // below (mirrors `find_all_for_tenant`'s pattern), rather than
        // pulling every OTHER user's assertions into memory too and
        // relying purely on the in-memory `HashMap::remove` filtering
        // afterward to keep them out of the response.
        let assertion_rows = sqlx::query_as::<_, AssertionRow>(
            r#"
            SELECT
                monitor_assertions.id,
                monitor_assertions.monitor_id,
                monitor_assertions.position,
                monitor_assertions.response_part,
                monitor_assertions.operator,
                monitor_assertions.target,
                monitor_assertions.expected_value,
                monitor_assertions.created_at,
                monitor_assertions.updated_at
            FROM monitor_assertions
            INNER JOIN monitors ON monitors.id = monitor_assertions.monitor_id
            WHERE monitors.user_id = $1
            ORDER BY monitor_assertions.monitor_id, monitor_assertions.position
            "#,
        )
        .bind(user_id.0)
        .fetch_all(&self.pool)
        .await?;
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
        let query = format!(
            "SELECT {MONITOR_COLUMNS} FROM monitors WHERE is_public = true ORDER BY created_at DESC"
        );
        let monitor_rows = sqlx::query_as::<_, MonitorRow>(&query)
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

    async fn delete_for_tenant(&self, id: MonitorId, tenant_id: TenantId) -> RepoResult<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM monitors
            WHERE id = $1
              AND tenant_id = $2
            "#,
        )
        .bind(id.0)
        .bind(tenant_id.0)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_by_heartbeat_token(&self, token: &str) -> RepoResult<Option<Monitor>> {
        let query = format!("SELECT {MONITOR_COLUMNS} FROM monitors WHERE heartbeat_token = $1");
        let row = sqlx::query_as::<_, MonitorRow>(&query)
            .bind(token)
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
        .bind(row.id)
        .fetch_all(&self.pool)
        .await?;

        let assertions = assertion_rows
            .into_iter()
            .map(AssertionRow::into_domain)
            .collect::<RepoResult<Vec<_>>>()?;

        Ok(Some(row.into_domain(assertions)?))
    }

    async fn update_last_heartbeat_at(
        &self,
        id: MonitorId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> RepoResult<()> {
        sqlx::query("UPDATE monitors SET last_heartbeat_at = $2, updated_at = $2 WHERE id = $1")
            .bind(id.0)
            .bind(now)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Atomically increments/resets `consecutive_failures` and
    /// `consecutive_successes` for `id` based on `success`, in a single
    /// `UPDATE ... RETURNING` statement.
    ///
    /// A single UPDATE is inherently row-locked by Postgres for its
    /// duration, so two concurrent calls for the same `id` (e.g. two
    /// worker replicas racing to process results for the same monitor)
    /// serialize at the database level: each sees the correctly
    /// up-to-date prior value, never a stale read. No advisory lock or
    /// explicit transaction is needed here — unlike
    /// `save_if_within_quota` above, there is no separate check-then-act
    /// step; the read and write are the same statement.
    async fn record_check_outcome(&self, id: MonitorId, success: bool) -> RepoResult<(u32, u32)> {
        let row: (i32, i32) = sqlx::query_as(
            r#"
            UPDATE monitors
            SET
                consecutive_failures = CASE WHEN $2 THEN 0 ELSE consecutive_failures + 1 END,
                consecutive_successes = CASE WHEN $2 THEN consecutive_successes + 1 ELSE 0 END,
                updated_at = now()
            WHERE id = $1
            RETURNING consecutive_failures, consecutive_successes
            "#,
        )
        .bind(id.0)
        .bind(success)
        .fetch_one(&self.pool)
        .await?;

        Ok((row.0 as u32, row.1 as u32))
    }

    /// Atomic quota-checked create — see the trait doc comment and this
    /// crate's `MonitorRepository` port for the contract.
    ///
    /// # Why an advisory lock, not `SELECT ... FOR UPDATE`
    ///
    /// `SELECT COUNT(*) ... FOR UPDATE` only takes row locks on rows that
    /// currently match the `WHERE` clause. It does **not** block a
    /// concurrent transaction from `INSERT`ing a brand-new row for the
    /// same tenant — there is no existing row to lock against a row that
    /// doesn't exist yet ("phantom read" territory). Two transactions can
    /// both run the `FOR UPDATE` count, both see e.g. 9 out of a limit of
    /// 10, and both proceed to insert — ending at 11.
    ///
    /// A `pg_advisory_xact_lock` avoids this: it's not a row lock at all,
    /// it's a lock on an arbitrary application-chosen key, held for the
    /// duration of the transaction and automatically released on
    /// `COMMIT` or `ROLLBACK` (including an implicit rollback from the
    /// `Transaction` being dropped without a `commit()`, e.g. on an early
    /// `return` or a `?`-propagated error). Taking this lock first
    /// serializes every `save_if_within_quota` call for the same tenant,
    /// so the `COUNT` that follows is guaranteed not to be invalidated by
    /// a concurrent insert before this transaction commits.
    ///
    /// # Reusable pattern for future resource types
    ///
    /// The lock key is namespaced as `"{resource_table}:{tenant_id}"`
    /// (here `"monitors:{tenant_id}"`) rather than just `tenant_id`, so
    /// that quota checks for different resource types (status pages, API
    /// keys, ...) don't serialize against each other unnecessarily — only
    /// concurrent creates of the *same* resource type for the *same*
    /// tenant contend for the same lock. Future workers adding quota
    /// enforcement for another resource should follow this exact
    /// four-step shape inside one transaction:
    ///   1. `SELECT pg_advisory_xact_lock(hashtext($1)::bigint)` with key
    ///      `"{table}:{tenant_id}"`
    ///   2. `SELECT COUNT(*) FROM {table} WHERE tenant_id = $1`
    ///   3. compare against the entitlement-resolved limit; if at/over,
    ///      return early (drop the transaction — automatic rollback) and
    ///      report `QuotaCheckOutcome::QuotaExceeded` — no partial writes
    ///   4. otherwise perform the insert inside the same transaction,
    ///      then `commit()`
    async fn save_if_within_quota(
        &self,
        monitor: &Monitor,
        tenant_id: TenantId,
        limit: u32,
    ) -> RepoResult<QuotaCheckOutcome> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(format!("monitors:{}", tenant_id.0))
            .execute(&mut *tx)
            .await?;

        let current_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM monitors WHERE tenant_id = $1")
                .bind(tenant_id.0)
                .fetch_one(&mut *tx)
                .await?;

        if current_count as u32 >= limit {
            // `tx` is dropped here without `commit()`, which sqlx turns
            // into an automatic ROLLBACK — releasing the advisory lock
            // and guaranteeing no partial write happened.
            return Ok(QuotaCheckOutcome::QuotaExceeded);
        }

        Self::insert_or_replace(&mut tx, monitor).await?;
        tx.commit().await?;

        Ok(QuotaCheckOutcome::Saved)
    }

    async fn set_enabled_for_tenant(
        &self,
        tenant_id: TenantId,
        monitor_ids: Option<&[MonitorId]>,
        enabled: bool,
    ) -> RepoResult<u64> {
        let mut tx = self.pool.begin().await?;
        let count = if let Some(ids) = monitor_ids {
            if ids.is_empty() {
                return Ok(0);
            }
            let ids: Vec<Uuid> = ids.iter().map(|id| id.0).collect();
            sqlx::query_scalar::<_, i64>(
                r#"
                WITH changed AS (
                    UPDATE monitors
                    SET enabled = $3,
                        updated_at = now(),
                        next_due_at = CASE
                            WHEN $3 AND NOT enabled
                            THEN now() + ((abs(hashtextextended(id::text, 0)) % 60) * interval '1 second')
                            ELSE next_due_at
                        END
                    WHERE tenant_id = $1 AND id = ANY($2)
                    RETURNING id
                )
                SELECT COUNT(*) FROM changed
                "#,
            )
            .bind(tenant_id.0)
            .bind(&ids)
            .bind(enabled)
            .fetch_one(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar::<_, i64>(
                r#"
                WITH changed AS (
                    UPDATE monitors
                    SET enabled = $2,
                        updated_at = now(),
                        next_due_at = CASE
                            WHEN $2 AND NOT enabled
                            THEN now() + ((abs(hashtextext(id::text, 0)) % 60) * interval '1 second')
                            ELSE next_due_at
                        END
                    WHERE tenant_id = $1
                    RETURNING id
                )
                SELECT COUNT(*) FROM changed
                "#,
            )
            .bind(tenant_id.0)
            .bind(enabled)
            .fetch_one(&mut *tx)
            .await?
        };
        tx.commit().await?;
        Ok(count as u64)
    }

    async fn set_billing_block(
        &self,
        monitor_id: MonitorId,
        tenant_id: TenantId,
        reason: &str,
    ) -> RepoResult<bool> {
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > 500 {
            return Err("invalid billing block reason".into());
        }
        let result = sqlx::query(
            r#"
            INSERT INTO monitor_billing_blocks(monitor_id, tenant_id, reason)
            SELECT id, tenant_id, $3
            FROM monitors
            WHERE id = $1 AND tenant_id = $2
            ON CONFLICT (monitor_id) DO UPDATE
            SET reason = EXCLUDED.reason, updated_at = now()
            "#,
        )
        .bind(monitor_id.0)
        .bind(tenant_id.0)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn clear_billing_blocks_for_tenant(&self, tenant_id: TenantId) -> RepoResult<u64> {
        let result = sqlx::query(
            "DELETE FROM monitor_billing_blocks WHERE tenant_id = $1",
        )
        .bind(tenant_id.0)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn billing_blocks_for_tenant(
        &self,
        tenant_id: TenantId,
    ) -> RepoResult<std::collections::HashMap<MonitorId, String>> {
        let rows = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT monitor_id, reason FROM monitor_billing_blocks WHERE tenant_id = $1",
        )
        .bind(tenant_id.0)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(id, reason)| (MonitorId(id), reason))
            .collect())
    }
}

