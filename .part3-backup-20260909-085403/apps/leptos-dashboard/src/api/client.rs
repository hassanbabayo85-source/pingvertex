use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use web_sys::{FormData, RequestCredentials};

const API_BASE: &str = "/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    Validation(String),
    NotFound,
    Conflict(String),
    RateLimited {
        message: String,
        retry_after: Option<u64>,
    },
    Network(String),
    Server(u16, String),
    Unknown(u16, String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => write!(f, "Unauthorized (401)"),
            Self::Forbidden => write!(f, "Forbidden (403)"),
            Self::Validation(msg) => write!(f, "Validation error: {msg}"),
            Self::NotFound => write!(f, "Not found (404)"),
            Self::Conflict(msg) => write!(f, "Conflict (409): {msg}"),
            Self::RateLimited {
                message,
                retry_after,
            } => match retry_after {
                Some(seconds) => {
                    write!(f, "Rate limited (429): {message} (retry after {seconds}s)")
                }
                None => write!(f, "Rate limited (429): {message}"),
            },
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::Server(status, msg) => write!(f, "Server error ({status}): {msg}"),
            Self::Unknown(status, msg) => write!(f, "Unexpected HTTP error ({status}): {msg}"),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(_: serde_json::Error) -> Self {
        ApiError::Unknown(0, "Invalid JSON response.".to_string())
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self {
        Self::Network(err.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

fn safe_message(status: u16, body: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<ErrorBody>(body) {
        if let Some(message) = parsed.message.filter(|m| !m.trim().is_empty()) {
            return if (500..=599).contains(&status) {
                "The server could not complete the request.".to_string()
            } else {
                message
            };
        }
    }
    if (500..=599).contains(&status) {
        "The server could not complete the request.".to_string()
    } else if body.trim().is_empty() {
        "The request could not be completed.".to_string()
    } else {
        body.trim().chars().take(500).collect()
    }
}

fn parse_retry_after(value: Option<&str>) -> Option<u64> {
    value.and_then(|v| v.trim().parse::<u64>().ok())
}

fn map_status(status: u16, body: String, retry_after: Option<u64>) -> ApiError {
    let message = safe_message(status, &body);
    match status {
        401 => ApiError::Unauthorized,
        403 => ApiError::Forbidden,
        400 | 422 => ApiError::Validation(message),
        404 => ApiError::NotFound,
        409 => ApiError::Conflict(message),
        429 => ApiError::RateLimited {
            message,
            retry_after,
        },
        500..=599 => ApiError::Server(status, message),
        _ => ApiError::Unknown(status, message),
    }
}

async fn handle_response<T: DeserializeOwned>(
    resp: gloo_net::http::Response,
) -> Result<T, ApiError> {
    let status = resp.status();
    if (200..300).contains(&status) {
        if status == 204 {
            return serde_json::from_str("null").map_err(ApiError::from);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        if text.trim().is_empty() {
            return Err(ApiError::Unknown(
                status,
                "Expected a JSON response but received an empty body.".into(),
            ));
        }
        serde_json::from_str(&text).map_err(ApiError::from)
    } else {
        let retry_after = parse_retry_after(resp.headers().get("retry-after").as_deref());
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text, retry_after))
    }
}

pub async fn get_json<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::get(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn post_json<T: DeserializeOwned, B: Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn put_json<T: DeserializeOwned, B: Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::put(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    handle_response(resp).await
}

async fn handle_no_content_response(resp: gloo_net::http::Response) -> Result<(), ApiError> {
    let status = resp.status();
    if (200..300).contains(&status) {
        return Ok(());
    }
    let retry_after = parse_retry_after(resp.headers().get("retry-after").as_deref());
    let text = resp.text().await.unwrap_or_default();
    Err(map_status(status, text, retry_after))
}

pub async fn post_json_no_content(path: &str) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    handle_no_content_response(resp).await
}

pub async fn delete_json_no_content(path: &str) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::delete(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    handle_no_content_response(resp).await
}

pub async fn get_json_query<T: DeserializeOwned>(
    path: &str,
    query: &[(&str, &str)],
) -> Result<T, ApiError> {
    let mut url = format!("{API_BASE}{path}");
    if !query.is_empty() {
        let encoded = query
            .iter()
            .map(|(k, v)| {
                let key = js_sys::encode_uri_component(k)
                    .as_string()
                    .unwrap_or_default();
                let value = js_sys::encode_uri_component(v)
                    .as_string()
                    .unwrap_or_default();
                format!("{key}={value}")
            })
            .collect::<Vec<_>>()
            .join("&");
        url.push('?');
        url.push_str(&encoded);
    }
    let resp = Request::get(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn delete_json<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::delete(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    handle_response(resp).await
}

pub async fn put_json_no_content<B: Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::put(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    handle_no_content_response(resp).await
}

pub async fn patch_json_no_content<B: Serialize>(path: &str, body: &B) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::patch(&url)
        .header("Content-Type", "application/json")
        .credentials(RequestCredentials::Include)
        .json(body)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    handle_no_content_response(resp).await
}

pub async fn post_multipart_file<T: DeserializeOwned>(
    path: &str,
    field_name: &str,
    file_name: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> Result<T, ApiError> {
    let form =
        FormData::new().map_err(|_| ApiError::Validation("could not create upload form".into()))?;
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let blob_parts = js_sys::Array::new();
    blob_parts.push(&array);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&blob_parts, &options)
        .map_err(|_| ApiError::Validation("could not create upload payload".into()))?;
    form.append_with_blob_and_filename(field_name, &blob, file_name)
        .map_err(|_| ApiError::Validation("could not attach upload".into()))?;
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url)
        .credentials(RequestCredentials::Include)
        .body(form)
        .map_err(|e| ApiError::Validation(e.to_string()))?
        .send()
        .await?;
    handle_response(resp).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_conflict_and_rate_limit() {
        assert!(matches!(
            map_status(409, r#"{"message":"already exists"}"#.into(), None),
            ApiError::Conflict(_)
        ));
        assert!(matches!(
            map_status(429, r#"{"message":"slow down"}"#.into(), Some(12)),
            ApiError::RateLimited {
                retry_after: Some(12),
                ..
            }
        ));
    }
    #[test]
    fn sanitizes_server_error_body() {
        let err = map_status(
            500,
            r#"{"message":"relation monitors does not exist"}"#.into(),
            None,
        );
        assert_eq!(
            err.to_string(),
            "Server error (500): The server could not complete the request."
        );
    }
}
