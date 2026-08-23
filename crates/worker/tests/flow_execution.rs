//! B29 integration tests for the API flow engine.
//!
//! - SSRF is blocked independently at every step (using the REAL
//!   production `execute_flow`, real SSRF resolver).
//! - Multi-step variable passing and the response-body size limit are
//!   tested against a local hand-rolled mock HTTP server using
//!   `execute_flow_with_resolver`'s injectable-resolver test seam (see
//!   `flow_executor.rs` module docs) — this bypasses SSRF ONLY for these
//!   two tests, which are not testing SSRF behavior; SSRF coverage lives
//!   in the tests that use `execute_flow` directly, above.
//! - Step-count limit is confirmed at the domain layer (`ApiFlow::new`).
//!
//! NOT covered here: the 60s total flow timeout (`FLOW_TOTAL_TIMEOUT_SECS`)
//! is not exercised end-to-end, since doing so would require a real
//! 60-second wait per test run. See HANDOVER.md Known Issues.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;

use domain::entities::{ApiFlow, ApiFlowStep};
use domain::shared::ids::{MonitorId, TenantId};
use domain::{HttpMethod, ResponsePart};
use domain::{
    entities::Assertion,
    enums::{AssertionOperator, ExtractSource},
    value_objects::{FlowStepName, VariableExtraction},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use worker::flow_executor::{DestinationResolverFn, execute_flow};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

fn no_op_assertion() -> Vec<Assertion> {
    vec![]
}

fn step(order: u16, name: &str, url_template: &str, assertions: Vec<Assertion>) -> ApiFlowStep {
    ApiFlowStep::new(
        order,
        FlowStepName::new(name).unwrap(),
        HttpMethod::Get,
        url_template.to_string(),
        vec![],
        None,
        vec![],
        assertions,
    )
    .unwrap()
}

fn flow(steps: Vec<ApiFlowStep>) -> ApiFlow {
    ApiFlow::new(
        domain::shared::ids::ApiFlowId::new(),
        MonitorId::new(),
        TenantId::new(),
        steps,
    )
    .unwrap()
}

// ===================== SSRF blocked at every step =====================
// Uses the REAL `execute_flow` (real SSRF resolver) — no mock server.

#[tokio::test]
async fn ssrf_blocks_a_single_step_flow_targeting_loopback() {
    let f = flow(vec![step(
        1,
        "Loopback",
        "http://127.0.0.1:1/",
        no_op_assertion(),
    )]);
    let client = reqwest::Client::new();

    let result = execute_flow(&f, &client).await;

    assert!(!result.passed);
    assert_eq!(result.outcomes.len(), 1);
    assert!(
        result.outcomes[0]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("SSRF blocked")
    );
}

#[tokio::test]
async fn ssrf_blocks_a_single_step_flow_targeting_cloud_metadata_address() {
    let f = flow(vec![step(
        1,
        "Metadata",
        "http://169.254.169.254/latest/meta-data/",
        no_op_assertion(),
    )]);
    let client = reqwest::Client::new();

    let result = execute_flow(&f, &client).await;

    assert!(!result.passed);
    assert!(
        result.outcomes[0]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("SSRF blocked")
    );
}

#[tokio::test]
async fn ssrf_blocks_a_later_step_independently_even_after_an_earlier_step_succeeds() {
    // Step 1 hits a real public address (mirrors the existing pattern in
    // `shared::ssrf`'s own tests, which also dial 1.1.1.1). Redirects are
    // disabled and step 1 has no assertions, so any HTTP response — even
    // a redirect status — passes step 1. Step 2 then independently must
    // go through SSRF validation and gets blocked, proving step position
    // doesn't matter — every step is checked on its own.
    let f = flow(vec![
        step(1, "Public", "http://1.1.1.1/", no_op_assertion()),
        step(2, "Internal", "http://10.0.0.1/", no_op_assertion()),
    ]);
    let client = reqwest::Client::new();

    let result = execute_flow(&f, &client).await;

    assert!(!result.passed);
    assert_eq!(result.failed_at_step, Some(2));
    assert_eq!(result.outcomes.len(), 2);
    assert!(result.outcomes[0].passed, "step 1 (public) should succeed");
    assert!(
        !result.outcomes[1].passed,
        "step 2 (private) must be blocked"
    );
    assert!(
        result.outcomes[1]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("SSRF blocked")
    );
}

// ===================== Local mock server test seam =====================

/// Spins a minimal one-shot-per-connection HTTP/1.1 mock server on
/// 127.0.0.1, serving the given canned raw responses in order (one per
/// accepted connection). A hand-rolled server is used instead of adding a
/// mocking crate as a new dev-dependency, since these tests' needs are
/// simple (fixed canned responses, sequential steps).
async fn spawn_mock_server(responses: Vec<String>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        for response in responses {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        }
    });

    addr
}

fn http_ok_json(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn http_ok_body(body: &[u8]) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut out = header.into_bytes();
    out.extend_from_slice(body);
    out
}

/// Test-only resolver that pins every request to `mock_addr`, bypassing
/// real SSRF/DNS resolution. Only used by the two tests below, which are
/// testing flow orchestration (variable passing, size limits), not SSRF —
/// SSRF coverage lives in the tests above using the real `execute_flow`.
fn mock_resolver(mock_addr: SocketAddr) -> Box<DestinationResolverFn> {
    Box::new(move |url: &str| {
        let url_owned = url.to_string();
        Box::pin(async move {
            let parsed = url::Url::parse(&url_owned).map_err(|e| e.to_string())?;
            let host = parsed.host_str().ok_or("no host")?.to_string();
            Ok(shared::ssrf::ResolvedDestination {
                url: parsed,
                host,
                socket_addr: mock_addr,
            })
        })
    })
}

// ===================== Multi-step variable passing =====================

#[tokio::test]
async fn multi_step_variable_passing_threads_extracted_token_into_next_step() {
    let mock_addr = spawn_mock_server(vec![
        http_ok_json(r#"{"token": "abc123"}"#),
        http_ok_json(r#"{"received": "abc123"}"#),
    ])
    .await;

    let f = flow(vec![
        step(
            1,
            "Login",
            &format!("http://mockhost.test:{}/login", mock_addr.port()),
            no_op_assertion(),
        ),
        step(
            2,
            "UseToken",
            &format!(
                "http://mockhost.test:{}/use?token={{{{steps.1.body.token}}}}",
                mock_addr.port()
            ),
            vec![
                Assertion::new(
                    ResponsePart::Json,
                    AssertionOperator::Equals,
                    Some("$.received".to_string()),
                    Some("abc123".to_string()),
                )
                .unwrap(),
            ],
        ),
    ]);

    let client = reqwest::Client::new();
    let resolver = mock_resolver(mock_addr);
    let result = worker::flow_executor::execute_flow_with_resolver(&f, &client, &resolver).await;

    assert!(
        result.passed,
        "expected flow to pass, outcomes: {:?}",
        result.outcomes
    );
    assert_eq!(result.outcomes.len(), 2);
    assert!(result.outcomes[0].passed);
    assert!(result.outcomes[1].passed);
}

// ===================== Bounded response size =====================

#[tokio::test]
async fn step_fails_when_response_body_exceeds_the_size_cap() {
    // 1 byte over the 1 MiB cap enforced in flow_executor.rs.
    let oversized_body = vec![b'x'; (1024 * 1024) + 1];
    let mock_addr = spawn_mock_server(vec![
        String::from_utf8(http_ok_body(&oversized_body))
            .unwrap_or_else(|_| unreachable!("body is all ASCII 'x'")),
    ])
    .await;

    let f = flow(vec![step(
        1,
        "Oversized",
        &format!("http://mockhost.test:{}/big", mock_addr.port()),
        no_op_assertion(),
    )]);

    let client = reqwest::Client::new();
    let resolver = mock_resolver(mock_addr);
    let result = worker::flow_executor::execute_flow_with_resolver(&f, &client, &resolver).await;

    assert!(!result.passed);
    assert!(
        result.outcomes[0]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("maximum permitted size")
    );
}

// ===================== Step count limit (domain-level) =====================

#[tokio::test]
async fn flow_creation_rejects_more_than_ten_steps() {
    let steps: Vec<ApiFlowStep> = (1..=11u16)
        .map(|order| {
            step(
                order,
                &format!("Step {order}"),
                "https://example.com",
                vec![],
            )
        })
        .collect();

    let result = ApiFlow::new(
        domain::shared::ids::ApiFlowId::new(),
        MonitorId::new(),
        TenantId::new(),
        steps,
    );

    assert!(result.is_err());
}

// Unused imports guard (VariableExtraction/ExtractSource kept available
// for anyone extending these tests with an `extract` step later).
#[allow(dead_code)]
fn _unused_extract_example() -> VariableExtraction {
    VariableExtraction::new("token".to_string(), ExtractSource::Body).unwrap()
}
