//! Runs an "API Flow" — a sequence of HTTP request steps executed
//! one after another, where each step can use the results of the steps
//! before it (via `{{steps.N....}}` placeholders).
//!
//! SECURITY: every step, ALWAYS, must pass through the SSRF guard
//! (`crate::ssrf::resolve_and_validate_destination`) BEFORE any HTTP
//! request is sent — even if the url_template is static, and especially
//! if part of it comes from a variable extracted by an earlier step, since
//! that is exactly what can carry an internal URL, for example when an
//! external API returns a redirect to `http://169.254.169.254/`.
//! There is no "fast path" that skips this.
//!
//! `execute_flow` (the production entry point) always uses the real
//! SSRF resolver. `execute_flow_with_resolver` is a generalized version
//! that accepts an injected `DestinationResolverFn`, used ONLY by this
//! crate's own tests to pin steps to a local mock HTTP server without
//! weakening the production path — the exact same test-seam pattern
//! `checker.rs` already established with `check_http_with_validator`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::assertion_evaluator::{EvalContext, evaluate_assertions};
use crate::placeholder_resolver::{StepResult, resolve_template};

/// Cap on the TOTAL time a single flow (with up to 10 steps) may take
/// BEFORE it is aborted entirely.
///
/// This is a cap on the WHOLE FLOW, NOT on each individual step — if
/// each step only had its own independent timeout, a flow of 10 steps
/// could run for a very long time (for example if every step came close
/// to its own limit) while holding worker resources for no good
/// reason.
pub const FLOW_TOTAL_TIMEOUT_SECS: u64 = 60;

/// Bound on a single step's response body, mirroring
/// `checker::MAX_HTTP_RESPONSE_BODY_BYTES`. A flow step must never read an
/// unbounded response body into memory. Exceeding this cap fails the step
/// (and therefore, via fail-fast, the whole flow) rather than silently
/// truncating, so assertions/extracted variables never operate on partial
/// data.
const MAX_STEP_RESPONSE_BODY_BYTES: usize = 1024 * 1024;

/// Alias for a boxed future that is `Send` and borrows data with lifetime
/// `'a`. This avoids the malformed multi-line `Pin<Box<dyn Future…>>`
/// return-type syntax that previously broke this file.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Signature for a resolved SSRF-safe destination lookup, generalized so
/// tests can inject a fake resolver. See module docs.
pub type DestinationResolverFn = dyn for<'a> Fn(
        &'a str,
    ) -> BoxFuture<'a, std::result::Result<shared::ssrf::ResolvedDestination, String>>
    + Send
    + Sync;

fn real_ssrf_resolver(
    url: &str,
) -> BoxFuture<'_, std::result::Result<shared::ssrf::ResolvedDestination, String>> {
    Box::pin(crate::ssrf::resolve_and_validate_destination(url))
}

#[derive(Debug)]
pub struct FlowStepOutcome {
    pub step_order: u16,
    pub step_name: String,
    pub passed: bool,
    pub attempted: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    /// Bytes observed/declared in this HTTP step for PAYG egress metering.
    pub egress_bytes: u64,
    pub assertion_results: Vec<domain::value_objects::AssertionEvaluationResult>,
}

#[derive(Debug)]
pub struct FlowExecutionResult {
    pub flow_id: domain::shared::ids::ApiFlowId,
    pub passed: bool,
    pub failed_at_step: Option<u16>,
    pub outcomes: Vec<FlowStepOutcome>,
    pub total_duration_ms: u128,
}

/// Runs ALL steps of `flow` using the REAL SSRF resolver used in
/// production (`crate::ssrf::resolve_and_validate_destination`). This is
/// the ONLY entry point external callers (for example
/// `FlowCheckExecutor`) should use.
pub async fn execute_flow(
    flow: &domain::entities::ApiFlow,
    http_client: &reqwest::Client,
) -> FlowExecutionResult {
    execute_flow_with_resolver(flow, http_client, &real_ssrf_resolver).await
}

/// Runs ALL steps of `flow`, one after another, FAIL-FAST on the first
/// step that fails (whether because placeholder resolution failed, the SSRF
/// guard blocks the destination, the HTTP request fails, or an assertion
/// fails) — no step runs after a failure, because later steps may depend on
/// "stale/empty variables" from the step that did not complete.
///
/// `resolver` is injectable ONLY so tests can pin steps to a local mock
/// server (see module docs). `execute_flow` above is the production entry
/// point and always passes the real SSRF resolver.
pub async fn execute_flow_with_resolver(
    flow: &domain::entities::ApiFlow,
    http_client: &reqwest::Client,
    resolver: &DestinationResolverFn,
) -> FlowExecutionResult {
    // `http_client` is deliberately not used directly here: this mirrors
    // `checker.rs::check_monitor` EXACTLY (which also takes a `_client`
    // parameter but builds a NEW `reqwest::Client` with its own
    // `.resolve(host, socket_addr)` pinning from the `destination`
    // that the SSRF guard has already validated) — each step builds
    // its own pinned client after SSRF resolution to prevent a second DNS
    // lookup once the address has been verified. The parameter is kept so
    // this function's signature matches the rest of the crate.
    let _ = http_client;

    let started = Instant::now();

    // This is kept separate from `run_all_steps` so that when
    // `tokio::time::timeout` fires and drops the future running the steps,
    // we still hold the progress made so far (the outcomes of steps
    // that already completed) instead of discarding them entirely.
    let outcomes_so_far: Arc<Mutex<Vec<FlowStepOutcome>>> = Arc::new(Mutex::new(Vec::new()));
    // Order of the step currently "in flight" (0 = none). Updated
    // BEFORE each step starts executing, so that if the timeout catches us
    // mid-step we know exactly which one it was.
    let in_flight_step: Arc<AtomicU16> = Arc::new(AtomicU16::new(0));
    let in_flight_attempted: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    let run = run_all_steps(
        flow,
        resolver,
        Arc::clone(&outcomes_so_far),
        Arc::clone(&in_flight_step),
        Arc::clone(&in_flight_attempted),
    );

    match tokio::time::timeout(Duration::from_secs(FLOW_TOTAL_TIMEOUT_SECS), run).await {
        Ok(()) => {
            let outcomes: Vec<FlowStepOutcome> = {
                let mut guard = outcomes_so_far.lock().await;
                guard.drain(..).collect()
            };

            let failed_at_step = outcomes.iter().find(|o| !o.passed).map(|o| o.step_order);
            let passed = failed_at_step.is_none();

            FlowExecutionResult {
                flow_id: flow.id,
                passed,
                failed_at_step,
                outcomes,
                total_duration_ms: started.elapsed().as_millis(),
            }
        }
        Err(_elapsed) => {
            // The flow exceeded its overall time limit. We rely on
            // `outcomes_so_far` (which is updated after EVERY step
            // finishes, not only at the end of the flow) so we can build a
            // useful final message instead of losing all progress made so far.
            let mut outcomes: Vec<FlowStepOutcome> = {
                let mut guard = outcomes_so_far.lock().await;
                guard.drain(..).collect()
            };

            let stalled_step = in_flight_step.load(Ordering::SeqCst);
            let stalled_name = flow
                .steps()
                .iter()
                .find(|s| s.order == stalled_step)
                .map(|s| s.name.as_str().to_string())
                .unwrap_or_default();

            outcomes.push(FlowStepOutcome {
                step_order: stalled_step,
                step_name: stalled_name,
                passed: false,
                attempted: in_flight_attempted.load(Ordering::Acquire),
                status_code: None,
                error: Some("flow exceeded 60s total timeout".to_string()),
                egress_bytes: 0,
                assertion_results: Vec::new(),
            });

            FlowExecutionResult {
                flow_id: flow.id,
                passed: false,
                failed_at_step: None,
                outcomes,
                total_duration_ms: started.elapsed().as_millis(),
            }
        }
    }
}

/// Executes ALL steps of `flow` one after another (ORDER ASC, exactly as
/// `flow.steps()` returns them), FAIL-FAST on the first failing step.
///
/// This is called INSIDE `tokio::time::timeout` by `execute_flow_with_resolver` —
/// so this function MUST NOT apply its own overall timeout; that is
/// `execute_flow_with_resolver`'s responsibility. Every outcome is pushed to
/// `outcomes_so_far` as soon as it completes, so that if this future is
/// cancelled from the outside (timeout), the progress already
/// obtained is not lost.
async fn run_all_steps(
    flow: &domain::entities::ApiFlow,
    resolver: &DestinationResolverFn,
    outcomes_so_far: Arc<Mutex<Vec<FlowStepOutcome>>>,
    in_flight_step: Arc<AtomicU16>,
    in_flight_attempted: Arc<AtomicBool>,
) {
    let mut results: HashMap<u16, StepResult> = HashMap::new();

    for step in flow.steps() {
        in_flight_step.store(step.order, Ordering::SeqCst);
        in_flight_attempted.store(false, Ordering::Release);

        let (outcome, step_result) =
            execute_step(step, &results, resolver, Arc::clone(&in_flight_attempted)).await;

        if let Some(step_result) = step_result {
            results.insert(step.order, step_result);
        }

        let failed = !outcome.passed;

        {
            let mut guard = outcomes_so_far.lock().await;
            guard.push(outcome);
        }

        if failed {
            // FAIL-FAST: stop immediately, do NOT continue to later steps.
            break;
        }
    }

    in_flight_step.store(0, Ordering::SeqCst);
    in_flight_attempted.store(false, Ordering::Release);
}

/// Executes exactly one step: resolve placeholders -> SSRF guard ->
/// send the HTTP request -> evaluate assertions. Returns `(outcome, StepResult
/// if an HTTP response was obtained)` — the `StepResult` is used by
/// later steps so they can consume this step's result (and even
/// if this step ultimately fails because of an assertion, that is fine,
/// because the loop STOPS immediately under FAIL-FAST before any later
/// step gets the chance to use it).
async fn execute_step(
    step: &domain::entities::ApiFlowStep,
    results: &HashMap<u16, StepResult>,
    resolver: &DestinationResolverFn,
    in_flight_attempted: Arc<AtomicBool>,
) -> (FlowStepOutcome, Option<StepResult>) {
    let step_name = step.name.as_str().to_string();

    let failed_outcome = |error: String| FlowStepOutcome {
        step_order: step.order,
        step_name: step_name.clone(),
        passed: false,
        attempted: false,
        status_code: None,
        error: Some(error),
        egress_bytes: 0,
        assertion_results: Vec::new(),
    };

    // a. Resolve url_template, every header value, and the body — ONLY
    // through `placeholder_resolver::resolve_template`; there is no other
    // interpolation path here.
    let resolved_url = match resolve_template(&step.url_template, results, step.order) {
        Ok(u) => u,
        Err(err) => return (failed_outcome(err), None),
    };

    let mut resolved_headers: Vec<(String, String)> = Vec::with_capacity(step.headers.len());
    for header in &step.headers {
        match resolve_template(header.value(), results, step.order) {
            Ok(v) => resolved_headers.push((header.name().to_string(), v)),
            Err(err) => return (failed_outcome(err), None),
        }
    }

    let resolved_body: Option<String> = match &step.body {
        Some(body_template) => match resolve_template(body_template, results, step.order) {
            Ok(b) => Some(b),
            Err(err) => return (failed_outcome(err), None),
        },
        None => None,
    };

    // b. SSRF GUARD — MANDATORY, WITHOUT EXCEPTION. This runs
    // ALWAYS, even when the url_template is static and could plausibly be
    // an internal address if the user wrote one deliberately, AND
    // especially when the url_template takes part of its value from a
    // variable extracted by an earlier step.
    let destination = match resolver(&resolved_url).await {
        Ok(d) => d,
        Err(err) => return (failed_outcome(err), None),
    };

    let method = match step.method {
        domain::HttpMethod::Get => reqwest::Method::GET,
        domain::HttpMethod::Head => reqwest::Method::HEAD,
    };

    // c. Mirroring `checker.rs::check_monitor` EXACTLY: build a new client
    // with `.resolve(host, socket_addr)` pinning from the `destination` the
    // SSRF guard already validated — no second DNS lookup after the address
    // has been verified, and redirects are disabled so that a redirect
    // cannot become a fresh SSRF vector.
    let pinned_client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
        // SECURITY: see the matching comment in `checker.rs` —
        // without `.no_proxy()`, a configured HTTP_PROXY/HTTPS_PROXY/
        // ALL_PROXY env var bypasses the `.resolve()` DNS pinning above,
        // reopening the SSRF/DNS-rebinding hole the destination
        // validation exists to close.
        .no_proxy()
        .build()
    {
        Ok(c) => c,
        Err(err) => return (failed_outcome(err.to_string()), None),
    };

    let mut request_builder = pinned_client.request(method, destination.url);
    for (key, value) in &resolved_headers {
        request_builder = request_builder.header(key, value);
    }
    if let Some(body) = &resolved_body {
        request_builder = request_builder.body(body.clone());
    }

    let started = Instant::now();
    in_flight_attempted.store(true, Ordering::Release);
    let response = request_builder.send().await;
    let latency = started.elapsed().as_millis();

    // d. Kasawar HTTP request (timeout, connection error, da sauransu).
    let response = match response {
        Ok(r) => r,
        Err(err) => return (failed_outcome(err.to_string()), None),
    };

    let status = response.status().as_u16();
    let declared_egress_bytes = if step.method == domain::HttpMethod::Head {
        0
    } else {
        response.content_length().unwrap_or(0)
    };

    let headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_lowercase(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    // Bounded body read — never reads more than
    // MAX_STEP_RESPONSE_BODY_BYTES into memory. Exceeding the cap fails
    // this step (and, via fail-fast, the whole flow) instead of silently
    // truncating.
    let body_bytes = match read_body_bounded(response).await {
        Ok(bytes) => bytes,
        Err(()) => {
            let mut outcome = failed_outcome(
                "HTTP response body exceeded the maximum permitted size".to_string(),
            );
            outcome.egress_bytes = declared_egress_bytes.min(MAX_STEP_RESPONSE_BODY_BYTES as u64);
            return (outcome, None);
        }
    };
    let egress_bytes = (body_bytes.len() as u64).max(declared_egress_bytes);
    let body_string = String::from_utf8_lossy(&body_bytes).into_owned();

    // e. Store the StepResult so later steps can use it.
    let step_result = StepResult {
        status_code: status,
        headers: headers.clone(),
        body: body_string,
    };

    // f. Evaluate this step's assertions. Even if the step returned
    // HTTP 200, if an assertion fails then the step as a whole
    // YA KASA.
    let ctx = EvalContext {
        status_code: status,
        headers,
        body_bytes,
        latency_ms: latency,
    };

    let (all_passed, assertion_results) = evaluate_assertions(&step.assertions, &ctx);

    let error = if all_passed {
        None
    } else {
        let errors: Vec<String> = assertion_results
            .iter()
            .filter_map(|r| r.message.clone())
            .collect();
        Some(format!("assertions sun gaza: {}", errors.join("; ")))
    };

    // g.
    (
        FlowStepOutcome {
            step_order: step.order,
            step_name,
            attempted: true,
            passed: all_passed,
            status_code: Some(status),
            error,
            egress_bytes,
            assertion_results,
        },
        Some(step_result),
    )
}

/// Reads a response body up to `MAX_STEP_RESPONSE_BODY_BYTES`. Never reads
/// unbounded and never accepts more than the configured cap. Mirrors
/// `checker::read_body_bounded`.
async fn read_body_bounded(mut response: reqwest::Response) -> std::result::Result<Vec<u8>, ()> {
    let mut body = Vec::new();

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > MAX_STEP_RESPONSE_BODY_BYTES {
                    return Err(());
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(_) => return Err(()),
        }
    }

    Ok(body)
}
