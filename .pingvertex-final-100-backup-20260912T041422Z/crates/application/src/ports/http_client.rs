use domain::HttpMethod;

/// HTTP response returned by the infrastructure.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
}

/// Abstraction over an HTTP client.
#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    async fn execute(
        &self,
        method: HttpMethod,
        url: &str,
        timeout_seconds: u32,
    ) -> Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>>;
}
