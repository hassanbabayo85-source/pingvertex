use crate::checker::CheckOutcome;
use domain::{Monitor, RegionCode};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct ClaimResponse {
    checks: Vec<ClaimedCheck>,
}

#[derive(Debug, Deserialize)]
pub struct ClaimedCheck {
    pub check_id: Uuid,
    pub monitor: Monitor,
    pub region: RegionCode,
}

#[derive(Debug, Serialize)]
struct SubmitResultRequest {
    check_id: Uuid,
    monitor_id: Uuid,
    region: String,
    success: bool,
    status_code: Option<u16>,
    latency_ms: u128,
    error: Option<String>,
    country_code: Option<String>,
}

pub async fn claim_checks(
    client: &Client,
    api_base: &str,
    agent_id: &str,
    agent_region: &str,
) -> anyhow::Result<Vec<ClaimedCheck>> {
    let url = format!("{api_base}/agents/{agent_id}/checks/claim");

    let resp = client
        .post(url)
        .header("X-Agent-Region", agent_region)
        .send()
        .await?
        .error_for_status()?;

    let body: ClaimResponse = resp.json().await?;
    Ok(body.checks)
}

pub async fn submit_result(
    client: &Client,
    api_base: &str,
    agent_id: &str,
    agent_region: &str,
    check_id: Uuid,
    monitor_id: Uuid,
    result: CheckOutcome,
) -> anyhow::Result<()> {
    let url = format!("{api_base}/agents/{agent_id}/results");

    let payload = SubmitResultRequest {
        check_id,
        monitor_id,
        region: result.region,
        success: result.success,
        status_code: result.status_code,
        latency_ms: result.latency_ms,
        error: result.error,
        country_code: result.country_code,
    };

    client
        .post(url)
        .header("X-Agent-Region", agent_region)
        .json(&payload)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
