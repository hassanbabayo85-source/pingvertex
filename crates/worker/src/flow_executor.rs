//! Gudanar da "API Flow" — jerin steps na HTTP request da ake gudanarwa
//! ɗaya-bayan-ɗaya, inda kowanne step zai iya amfani da sakamakon steps
//! da suka gabata (ta hanyar `{{steps.N....}}` placeholders).
//!
//! SECURITY: kowanne step, KODA YAUSHE, dole ya wuce ta cikin SSRF guard
//! (`crate::ssrf::resolve_and_validate_destination`) KAFIN a aika HTTP
//! request — koda url_template ɗin static ce, koda kuma ta samu wani
//! ɓangare daga extracted variable na wani step na baya (domin shi ne
//! ainihin abinda zai iya ɗauke da wani internal URL, misali idan wata
//! external API ta dawo da redirect zuwa `http://169.254.169.254/`).
//! Babu wani "fast path" da ke tsallake wannan.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::assertion_evaluator::{EvalContext, evaluate_assertions};
use crate::placeholder_resolver::{StepResult, resolve_template};

/// Iyaka akan JIMLAR lokacin da flow guda ɗaya (mai steps har zuwa 10) zai
/// iya ɗauka KAFIN a soke shi baki ɗaya.
///
/// Wannan iyaka ce akan FLOW DUKA, BA kowanne step ɗaya-bayan-ɗaya ba —
/// idan kowanne step yana da nasa timeout mai zaman kansa kaɗai, flow mai
/// steps 10 na iya kwana yana gudana (misali idan kowanne step ya kusa kai
/// iyakarsa) tare da toshe worker resources ba tare da wani dalili na
/// gaskiya ba.
pub const FLOW_TOTAL_TIMEOUT_SECS: u64 = 60;

#[derive(Debug)]
pub struct FlowStepOutcome {
    pub step_order: u16,
    pub step_name: String,
    pub passed: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
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

/// Gudanar da DUK steps ɗin `flow`, ɗaya-bayan-ɗaya, FAIL-FAST akan step na
/// farko da ya kasa (ko dai domin placeholder resolution ya kasa, SSRF
/// guard ya toshe destination, HTTP request ɗin ya kasa, ko wani assertion
/// ya gaza) — babu wani step da zai gudana bayan wani step ya kasa, domin
/// steps na gaba na iya dogaro da "stale/empty variables" daga step da ya
/// gagara.
pub async fn execute_flow(
    flow: &domain::entities::ApiFlow,
    http_client: &reqwest::Client,
) -> FlowExecutionResult {
    // `http_client` ba a amfani da shi kai-tsaye a nan: bin salon
    // `checker.rs::check_monitor` A-DAIDAI (wanda shi ma yana da
    // parameter `_client` amma yana gina SABON `reqwest::Client` tare da
    // `.resolve(host, socket_addr)` pinning na kansa daga `destination`
    // ɗin da SSRF guard ya riga ya tabbatar) — kowanne step yana gina
    // nasa pinned client bayan SSRF-resolve domin hana wani sabon DNS
    // lookup bayan an riga an tabbatar da adireshin. An bar wannan
    // parameter domin daidaita sigar aikin da sauran ayyukan wannan crate.
    let _ = http_client;

    let started = Instant::now();

    // An raba wannan da `run_all_steps` domin idan `tokio::time::timeout`
    // ya wuce lokaci ya soke (drop) future ɗin da ke gudanar da steps,
    // mun riga mun samu ci gaban da aka yi zuwa yanzu (outcomes na steps
    // da suka riga sun gama) maimakon ɓatar da su gaba ɗaya.
    let outcomes_so_far: Arc<Mutex<Vec<FlowStepOutcome>>> = Arc::new(Mutex::new(Vec::new()));
    // Order na step ɗin da ke "in-flight" (0 = babu). Ana sabunta wannan
    // KAFIN a fara aiwatar da kowanne step, don haka idan timeout ya kama
    // mu a tsakiyar wani step, mun san wanne ne.
    let in_flight_step: Arc<AtomicU16> = Arc::new(AtomicU16::new(0));

    let run = run_all_steps(
        flow,
        Arc::clone(&outcomes_so_far),
        Arc::clone(&in_flight_step),
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
                flow_id: flow.id.clone(),
                passed,
                failed_at_step,
                outcomes,
                total_duration_ms: started.elapsed().as_millis(),
            }
        }
        Err(_elapsed) => {
            // Flow ya wuce iyakar lokaci gabaɗaya. Mun dogara akan
            // `outcomes_so_far` (wanda aka sabunta bayan KOWANNE step ya
            // gama, ba a ƙarshen flow ba kaɗai) domin mu gina saƙon ƙarshe
            // mai amfani maimakon ɓata dukkan ci gaban da aka riga aka samu.
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
                status_code: None,
                error: Some("flow exceeded 60s total timeout".to_string()),
                assertion_results: Vec::new(),
            });

            FlowExecutionResult {
                flow_id: flow.id.clone(),
                passed: false,
                failed_at_step: None,
                outcomes,
                total_duration_ms: started.elapsed().as_millis(),
            }
        }
    }
}

/// Aiwatar da DUK steps na `flow` ɗaya-bayan-ɗaya (ORDER ASC, kamar yadda
/// `flow.steps()` ya dawo da su), FAIL-FAST akan step na farko mai gaza.
///
/// Ana kiran wannan a CIKIN `tokio::time::timeout` daga `execute_flow` —
/// don haka wannan aikin BAI KAMATA ya yi nasa timeout na jimlar lokaci
/// ba, domin `execute_flow` ke da alhakin hakan. Kowanne outcome ana
/// turashi zuwa `outcomes_so_far` nan take bayan an gama shi, domin idan
/// aka soke wannan future daga waje (timeout), ci gaban da aka riga aka
/// samu bai ɓace ba.
async fn run_all_steps(
    flow: &domain::entities::ApiFlow,
    outcomes_so_far: Arc<Mutex<Vec<FlowStepOutcome>>>,
    in_flight_step: Arc<AtomicU16>,
) {
    let mut results: HashMap<u16, StepResult> = HashMap::new();

    for step in flow.steps() {
        in_flight_step.store(step.order, Ordering::SeqCst);

        let (outcome, step_result) = execute_step(step, &results).await;

        if let Some(step_result) = step_result {
            results.insert(step.order, step_result);
        }

        let failed = !outcome.passed;

        {
            let mut guard = outcomes_so_far.lock().await;
            guard.push(outcome);
        }

        if failed {
            // FAIL-FAST: TSAYA nan take, KADA a ci gaba da steps na gaba.
            break;
        }
    }

    in_flight_step.store(0, Ordering::SeqCst);
}

/// Aiwatar da step guda ɗaya kaɗai: resolve placeholders -> SSRF guard ->
/// aika HTTP request -> tantance assertions. Mayar da `(outcome, StepResult
/// idan an sami nasarar HTTP response)` — `StepResult` ɗin ana amfani da
/// shi domin steps na gaba su iya amfani da sakamakon wannan step (koda
/// idan wannan step ɗin ya kasa a ƙarshe saboda assertion, ba ruwan mu,
/// domin loop ɗin zai TSAYA nan take saboda FAIL-FAST kafin wani step na
/// gaba ya sami dama ya amfana da shi).
async fn execute_step(
    step: &domain::entities::ApiFlowStep,
    results: &HashMap<u16, StepResult>,
) -> (FlowStepOutcome, Option<StepResult>) {
    let step_name = step.name.as_str().to_string();

    let failed_outcome = |error: String| FlowStepOutcome {
        step_order: step.order,
        step_name: step_name.clone(),
        passed: false,
        status_code: None,
        error: Some(error),
        assertion_results: Vec::new(),
    };

    // a. Resolve url_template, kowanne header value, da body — KAWAI ta
    // hanyar `placeholder_resolver::resolve_template`, babu wata sabuwar
    // hanyar interpolation a nan.
    let resolved_url = match resolve_template(&step.url_template, results, step.order) {
        Ok(u) => u,
        Err(err) => return (failed_outcome(err), None),
    };

    let mut resolved_headers: Vec<(String, String)> = Vec::with_capacity(step.headers.len());
    for (key, value_template) in &step.headers {
        match resolve_template(value_template, results, step.order) {
            Ok(v) => resolved_headers.push((key.clone(), v)),
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

    // b. SSRF GUARD — DOLE, BA TARE DA TOGIYA BA. Ana gudanar da wannan
    // KOYAUSHE, koda url_template ɗin static ce mai yiwuwa ta zama
    // internal address idan mai amfani ya rubuta ta da gangan, KUMA
    // musamman koda url_template ta samu wani ɓangare daga extracted
    // variable na wani step na baya.
    let destination = match crate::ssrf::resolve_and_validate_destination(&resolved_url).await {
        Ok(d) => d,
        Err(err) => return (failed_outcome(err), None),
    };

    let method = match step.method {
        domain::HttpMethod::Get => reqwest::Method::GET,
        domain::HttpMethod::Head => reqwest::Method::HEAD,
    };

    // c. Bin salon `checker.rs::check_monitor` A-DAIDAI: gina sabon client
    // mai `.resolve(host, socket_addr)` pinning daga `destination` da SSRF
    // guard ya riga ya tabbatar — babu sabon DNS lookup bayan an tabbatar
    // da adireshin, kuma redirect an kashe shi domin ba za mu bar wani
    // redirect ya zama sabuwar hanyar SSRF ba.
    let pinned_client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&destination.host, destination.socket_addr)
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
    let response = request_builder.send().await;
    let latency = started.elapsed().as_millis();

    // d. Kasawar HTTP request (timeout, connection error, da sauransu).
    let response = match response {
        Ok(r) => r,
        Err(err) => return (failed_outcome(err.to_string()), None),
    };

    let status = response.status().as_u16();

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

    let body_bytes = response.bytes().await.unwrap_or_default().to_vec();
    let body_string = String::from_utf8_lossy(&body_bytes).into_owned();

    // e. Ajiye StepResult domin steps na gaba su iya amfani dashi.
    let step_result = StepResult {
        status_code: status,
        headers: headers.clone(),
        body: body_string,
    };

    // f. Tantance assertions na wannan step. Koda step ɗin ya dawo da
    // HTTP 200 amma wani assertion ya kasa, wannan step din baki ɗayansa
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
            passed: all_passed,
            status_code: Some(status),
            error,
            assertion_results,
        },
        Some(step_result),
    )
}
