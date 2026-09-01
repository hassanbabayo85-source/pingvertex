use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde::Serialize;
use web_sys::RequestCredentials;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ApiError {
    Network(String),
    Http(u16, String),
    Json(String),
}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self {
        ApiError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::Json(err.to_string())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

pub async fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, ApiError> {
    let resp = Request::get(url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    let status = resp.status();
    if status == 200 {
        resp.json::<T>().await.map_err(ApiError::from)
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(ApiError::Http(status, text))
    }
}

#[allow(dead_code)]
pub async fn post_json<T: DeserializeOwned, B: Serialize>(
    url: &str,
    body: &B,
) -> Result<T, ApiError> {
    let resp = Request::post(url)
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Json(e.to_string()))?
        .send()
        .await?;
    let status = resp.status();
    if status == 200 {
        resp.json::<T>().await.map_err(ApiError::from)
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(ApiError::Http(status, text))
    }
}

pub async fn health_check() -> Result<HealthResponse, ApiError> {
    get_json::<HealthResponse>("/api/health").await
}
