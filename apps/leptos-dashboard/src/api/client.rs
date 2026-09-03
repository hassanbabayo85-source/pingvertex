use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use serde::Serialize;
use web_sys::{FormData, RequestCredentials};

const API_BASE: &str = "/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    Validation(String),
    NotFound,
    Network(String),
    Server(u16, String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "Unauthorized (401)"),
            ApiError::Forbidden => write!(f, "Forbidden (403)"),
            ApiError::Validation(msg) => write!(f, "Validation error: {msg}"),
            ApiError::NotFound => write!(f, "Not found (404)"),
            ApiError::Network(msg) => write!(f, "Network error: {msg}"),
            ApiError::Server(status, msg) => write!(f, "Server error ({status}): {msg}"),
        }
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self {
        ApiError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::Validation(format!("Invalid JSON: {err}"))
    }
}

fn map_status(status: u16, body: String) -> ApiError {
    match status {
        401 => ApiError::Unauthorized,
        403 => ApiError::Forbidden,
        400 | 422 => ApiError::Validation(body),
        404 => ApiError::NotFound,
        500..=599 => ApiError::Server(status, body),
        _ => ApiError::Server(status, body),
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
        serde_json::from_str(&text).map_err(ApiError::from)
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
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

pub async fn post_json_no_content(path: &str) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::post(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    let status = resp.status();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
}

pub async fn delete_json_no_content(path: &str) -> Result<(), ApiError> {
    let url = format!("{API_BASE}{path}");
    let resp = Request::delete(&url)
        .credentials(RequestCredentials::Include)
        .send()
        .await?;
    let status = resp.status();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
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
    let status = resp.status();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
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
    let status = resp.status();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(map_status(status, text))
    }
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
