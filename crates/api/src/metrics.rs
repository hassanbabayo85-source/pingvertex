use std::sync::OnceLock;

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing::warn;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Prefix na daidaitaccen `Authorization: Bearer <token>` header, wanda
/// Prometheus scrape_config ke iya turowa kai tsaye ta hanyar
/// `authorization: { type: Bearer, credentials_file: ... }` — babu bukatar
/// wani custom header.
const BEARER_PREFIX: &str = "Bearer ";

pub fn init_metrics() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder");

    let _ = PROMETHEUS_HANDLE.set(handle);
}

pub fn render_metrics() -> String {
    PROMETHEUS_HANDLE
        .get()
        .map(|handle| handle.render())
        .unwrap_or_default()
}

/// Kwatanta biyu &str a cikin lokaci mai daidaito (constant-time),
/// domin toshe timing side-channel wanda zai iya bawa mai kai hari
/// damar tantance sirrin METRICS_TOKEN haruffa-haruffa.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());

    if a.len() != b.len() {
        return false;
    }

    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }

    diff == 0
}

/// Handler na `/metrics` mai kariya.
///
/// FAIL-CLOSED: idan `METRICS_TOKEN` bai saitu a environment ba, endpoint
/// ɗin zai koma 404 (baya bayyana ko wanzuwarsa) maimakon ya buɗe wa kowa
/// — domin an gano a security audit na 2026-08-11 cewa metrics din baya
/// da wata kariya, wanda zai iya bayyana adadin monitors, latency, da
/// sauran bayanan aiki na cikin gida ga duk wanda ya isa endpoint ɗin.
///
/// Idan `METRICS_TOKEN` an saita shi, dole mai kira ya turo daidai
/// darajarsa a header `Authorization: Bearer <token>`, in ba haka ba sai
/// 401.
pub async fn metrics_handler(headers: HeaderMap) -> Response {
    let expected_token = match std::env::var("METRICS_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        _ => {
            warn!(
                "METRICS_TOKEN ba a saita shi ba a environment — /metrics endpoint yana \
                 rufe (fail-closed). Saita METRICS_TOKEN domin bada damar Prometheus \
                 scraping."
            );
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let provided_token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(BEARER_PREFIX))
        .unwrap_or("");

    if !constant_time_eq(provided_token, &expected_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    render_metrics().into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_equal_strings() {
        assert!(constant_time_eq("my-secret-token", "my-secret-token"));
    }

    #[test]
    fn constant_time_eq_rejects_different_strings() {
        assert!(!constant_time_eq("my-secret-token", "wrong-token"));
    }

    #[test]
    fn constant_time_eq_rejects_different_length_strings() {
        assert!(!constant_time_eq("short", "much-longer-string"));
    }

    #[test]
    fn constant_time_eq_rejects_empty_provided_token() {
        assert!(!constant_time_eq("", "my-secret-token"));
    }

    #[test]
    fn constant_time_eq_treats_empty_equal_to_empty() {
        // Wannan hali ne kawai na aikin comparison da kansa; handler ɗin
        // baya taɓa kiran wannan aikin idan METRICS_TOKEN bai saitu ba
        // (fail-closed 404 kafin a isa nan), don haka babu hadari.
        assert!(constant_time_eq("", ""));
    }

    // NOTE: waɗannan gwaje-gwajen suna canza `METRICS_TOKEN` env var, don
    // haka an rufe su duka a cikin `#[serial]`-style pattern na hannu
    // (guda ɗaya, jere) domin toshe race condition tsakanin tests masu
    // gudana lokaci guda. Idan aka ƙara wasu env-var-mutating tests a
    // nan gaba, a haɗa su cikin wannan function guda ɗaya ko a yi amfani
    // da `serial_test` crate.
    #[tokio::test]
    async fn metrics_handler_env_var_scenarios() {
        use axum::body::to_bytes;
        use axum::http::{header::AUTHORIZATION, HeaderValue};

        // 1) Babu METRICS_TOKEN a environment => fail-closed 404.
        unsafe {
            std::env::remove_var("METRICS_TOKEN");
        }
        let response = metrics_handler(HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // 2) An saita METRICS_TOKEN amma babu Authorization header => 401.
        unsafe {
            std::env::set_var("METRICS_TOKEN", "test-token-abc123");
        }
        let response = metrics_handler(HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 3) An turo token da bai dace ba => 401.
        let mut wrong_headers = HeaderMap::new();
        wrong_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong-token"),
        );
        let response = metrics_handler(wrong_headers).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // 4) An turo daidai token din => 200, tare da metrics body.
        let mut correct_headers = HeaderMap::new();
        correct_headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer test-token-abc123"),
        );
        let response = metrics_handler(correct_headers).await;
        assert_eq!(response.status(), StatusCode::OK);
        // Idan an isa nan, an wuce auth check ɗin daidai. Babu recorder
        // da aka install a wannan test binary, don haka kawai muna
        // tabbatar cewa karanta body ɗin baya haifar da panic/error.
        let _body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

        unsafe {
            std::env::remove_var("METRICS_TOKEN");
        }
    }
}
