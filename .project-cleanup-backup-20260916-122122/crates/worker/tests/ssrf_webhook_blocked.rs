use shared::ssrf::resolve_and_validate_destination;

/// B26 task requirement: webhook sending must be SSRF-safe. The shared
/// SSRF validator rejects private/internal IPs before any outbound request
/// is built. This test verifies the exact function that
/// `WebhookNotificationService` calls on every send attempt.
#[tokio::test]
async fn webhook_url_with_private_ip_is_blocked() {
    let blocked_urls = [
        "http://127.0.0.1:8080/status",
        "http://10.0.0.5/hook",
        "http://[::1]:8080/hook",
        "http://192.168.1.1/hook",
        "http://169.254.169.254/latest/meta-data",
    ];

    for url in blocked_urls {
        let result = resolve_and_validate_destination(url).await;
        assert!(
            result.is_err(),
            "expected {url} to be blocked by SSRF guard"
        );
    }
}
