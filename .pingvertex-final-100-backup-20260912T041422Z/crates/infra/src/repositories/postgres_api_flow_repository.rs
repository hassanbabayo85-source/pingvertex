use std::collections::HashMap;
use std::error::Error;

use application::ports::ApiFlowRepository;
use async_trait::async_trait;
use domain::shared::ids::{ApiFlowId, ApiFlowStepId, AssertionId, MonitorId, TenantId};
use domain::{
    ApiFlow, ApiFlowStep, Assertion, AssertionOperator, FlowStepName, HttpMethod, ResponsePart,
    VariableExtraction,
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

type RepoResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// PostgreSQL-backed API flow repository.
#[derive(Debug, Clone)]
pub struct PostgresApiFlowRepository {
    pool: PgPool,
}

impl PostgresApiFlowRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Loads the steps (and their assertions) for an already-fetched
    /// `api_flows` row and assembles the full `ApiFlow` aggregate.
    ///
    /// B35: loads all step assertions in one `ANY($1)` query instead of
    /// one query per step, removing the N+1 pattern this method had.
    async fn build_flow(&self, row: ApiFlowRow) -> RepoResult<ApiFlow> {
        let step_rows = sqlx::query_as::<_, ApiFlowStepRow>(
            r#"
            SELECT
                id,
                api_flow_id,
                "order",
                name,
                method,
                url_template,
                headers,
                body,
                extract,
                created_at,
                updated_at
            FROM api_flow_steps
            WHERE api_flow_id = $1
            ORDER BY "order" ASC
            "#,
        )
        .bind(row.id)
        .fetch_all(&self.pool)
        .await?;

        if step_rows.is_empty() {
            return row.into_domain(Vec::new());
        }

        let step_ids: Vec<Uuid> = step_rows.iter().map(|step| step.id).collect();

        let assertion_rows = sqlx::query_as::<_, FlowStepAssertionRow>(
            r#"
            SELECT
                id,
                api_flow_step_id,
                position,
                response_part,
                operator,
                target,
                expected_value,
                created_at,
                updated_at
            FROM monitor_assertions
            WHERE api_flow_step_id = ANY($1)
            ORDER BY api_flow_step_id, position ASC
            "#,
        )
        .bind(&step_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut assertions_by_step: HashMap<Uuid, Vec<Assertion>> = HashMap::new();
        for assertion_row in assertion_rows {
            let step_id = assertion_row.api_flow_step_id;
            let domain_assertion = assertion_row.into_domain()?;
            assertions_by_step
                .entry(step_id)
                .or_default()
                .push(domain_assertion);
        }

        let mut steps = Vec::with_capacity(step_rows.len());
        for step_row in step_rows {
            let assertions = assertions_by_step.remove(&step_row.id).unwrap_or_default();
            steps.push(step_row.into_domain(assertions)?);
        }

        row.into_domain(steps)
    }
}

//
// api_flows table row
//
#[derive(Debug, Clone, FromRow)]
struct ApiFlowRow {
    id: Uuid,
    monitor_id: Uuid,
    tenant_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl ApiFlowRow {
    fn into_domain(self, steps: Vec<ApiFlowStep>) -> RepoResult<ApiFlow> {
        Ok(ApiFlow {
            id: ApiFlowId(self.id),
            monitor_id: MonitorId(self.monitor_id),
            tenant_id: TenantId(self.tenant_id),
            steps,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

//
// api_flow_steps table row
//
#[derive(Debug, Clone, FromRow)]
struct ApiFlowStepRow {
    id: Uuid,
    /// Loaded from the database so sqlx can map the query result.
    /// `ApiFlowStep` does not carry its parent flow id, so this field is not read.
    #[expect(dead_code)]
    api_flow_id: Uuid,
    order: i16,
    name: String,
    method: String,
    url_template: String,
    headers: serde_json::Value,
    body: Option<String>,
    extract: serde_json::Value,
    /// Loaded from the database so sqlx can map the query result.
    /// `ApiFlowStep` does not carry its own timestamps, so these fields are not read.
    #[expect(dead_code)]
    created_at: chrono::DateTime<chrono::Utc>,
    #[expect(dead_code)]
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl ApiFlowStepRow {
    fn into_domain(self, assertions: Vec<Assertion>) -> RepoResult<ApiFlowStep> {
        let method = method_from_db(&self.method)?;
        let headers: Vec<(String, String)> = serde_json::from_value(self.headers)?;
        let extract: Vec<VariableExtraction> = serde_json::from_value(self.extract)?;

        Ok(ApiFlowStep {
            id: ApiFlowStepId(self.id),
            order: self.order.try_into()?,
            name: FlowStepName::new(self.name)?,
            method,
            url_template: self.url_template,
            headers,
            body: self.body,
            extract,
            assertions,
        })
    }
}

//
// monitor_assertions rows scoped to a single api_flow_step
//
#[derive(Debug, Clone, FromRow)]
struct FlowStepAssertionRow {
    id: Uuid,
    /// Loaded from the database so sqlx can map the query result.
    /// Used for grouping assertions by step in `build_flow`.
    api_flow_step_id: Uuid,
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

impl FlowStepAssertionRow {
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
impl ApiFlowRepository for PostgresApiFlowRepository {
    async fn save(&self, flow: &ApiFlow) -> RepoResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO api_flows (id, monitor_id, tenant_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(flow.id.0)
        .bind(flow.monitor_id.0)
        .bind(flow.tenant_id.0)
        .bind(flow.created_at)
        .bind(flow.updated_at)
        .execute(&mut *tx)
        .await?;

        for step in flow.steps() {
            let headers = serde_json::to_value(&step.headers)?;
            let extract = serde_json::to_value(&step.extract)?;
            let order: i16 = step.order.try_into()?;

            sqlx::query(
                r#"
                INSERT INTO api_flow_steps (
                    id,
                    api_flow_id,
                    "order",
                    name,
                    method,
                    url_template,
                    headers,
                    body,
                    extract
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
            )
            .bind(step.id.0)
            .bind(flow.id.0)
            .bind(order)
            .bind(step.name.as_str())
            .bind(method_to_db(step.method))
            .bind(&step.url_template)
            .bind(headers)
            .bind(&step.body)
            .bind(extract)
            .execute(&mut *tx)
            .await?;

            for (position, assertion) in step.assertions.iter().enumerate() {
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
                        api_flow_step_id,
                        created_at,
                        updated_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                    "#,
                )
                .bind(assertion.id.0)
                .bind(flow.monitor_id.0)
                .bind(position as i32)
                .bind(response_part_to_db(&assertion.response_part))
                .bind(operator_to_db(&assertion.operator))
                .bind(&assertion.target)
                .bind(&assertion.expected_value)
                .bind(step.id.0)
                .bind(assertion.created_at)
                .bind(assertion.updated_at)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;

        Ok(())
    }

    async fn update(&self, flow: &ApiFlow) -> RepoResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE api_flows
            SET updated_at = $1
            WHERE id = $2
            "#,
        )
        .bind(flow.updated_at)
        .bind(flow.id.0)
        .execute(&mut *tx)
        .await?;

        // Cascades: deleting a step also deletes its monitor_assertions rows
        // via `monitor_assertions.api_flow_step_id ... ON DELETE CASCADE`
        // (migration 0014), so old assertions are cleaned up for free.
        sqlx::query(
            r#"
            DELETE FROM api_flow_steps
            WHERE api_flow_id = $1
            "#,
        )
        .bind(flow.id.0)
        .execute(&mut *tx)
        .await?;

        for step in flow.steps() {
            let headers = serde_json::to_value(&step.headers)?;
            let extract = serde_json::to_value(&step.extract)?;
            let order: i16 = step.order.try_into()?;

            sqlx::query(
                r#"
                INSERT INTO api_flow_steps (
                    id,
                    api_flow_id,
                    "order",
                    name,
                    method,
                    url_template,
                    headers,
                    body,
                    extract
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
            )
            .bind(step.id.0)
            .bind(flow.id.0)
            .bind(order)
            .bind(step.name.as_str())
            .bind(method_to_db(step.method))
            .bind(&step.url_template)
            .bind(headers)
            .bind(&step.body)
            .bind(extract)
            .execute(&mut *tx)
            .await?;

            for (position, assertion) in step.assertions.iter().enumerate() {
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
                        api_flow_step_id,
                        created_at,
                        updated_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                    "#,
                )
                .bind(assertion.id.0)
                .bind(flow.monitor_id.0)
                .bind(position as i32)
                .bind(response_part_to_db(&assertion.response_part))
                .bind(operator_to_db(&assertion.operator))
                .bind(&assertion.target)
                .bind(&assertion.expected_value)
                .bind(step.id.0)
                .bind(assertion.created_at)
                .bind(assertion.updated_at)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;

        Ok(())
    }

    async fn delete(&self, id: ApiFlowId) -> RepoResult<bool> {
        // ON DELETE CASCADE (migration 0014) removes the flow's api_flow_steps,
        // which in turn cascades the deletion of their monitor_assertions rows.
        let result = sqlx::query(
            r#"
            DELETE FROM api_flows
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_for_tenant(&self, id: ApiFlowId, tenant_id: TenantId) -> RepoResult<bool> {
        // ON DELETE CASCADE (migration 0014) removes the flow's api_flow_steps,
        // which in turn cascades the deletion of their monitor_assertions rows.
        let result = sqlx::query(
            r#"
            DELETE FROM api_flows
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

    async fn find_by_id(&self, id: ApiFlowId) -> RepoResult<Option<ApiFlow>> {
        let row = sqlx::query_as::<_, ApiFlowRow>(
            r#"
            SELECT id, monitor_id, tenant_id, created_at, updated_at
            FROM api_flows
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(self.build_flow(row).await?))
    }

    async fn find_by_monitor_id(&self, monitor_id: MonitorId) -> RepoResult<Option<ApiFlow>> {
        let row = sqlx::query_as::<_, ApiFlowRow>(
            r#"
            SELECT id, monitor_id, tenant_id, created_at, updated_at
            FROM api_flows
            WHERE monitor_id = $1
            "#,
        )
        .bind(monitor_id.0)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(self.build_flow(row).await?))
    }
}
