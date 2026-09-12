#!/usr/bin/env bash
set -euo pipefail

REPO="${1:-$HOME/uptime-monitor}"
cd "$REPO"

if [[ ! -f Cargo.toml ]]; then
  echo "ERROR: $REPO does not look like the PingVertex/uptime-monitor repository (Cargo.toml missing)." >&2
  exit 1
fi

STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_DIR=".payg-gap-closure-backup-${STAMP}"
mkdir -p "$BACKUP_DIR"

# Product decisions encoded by this addendum:
# A) Recurring proxy monitoring is retired; one-shot proxy checks remain.
# B) Network egress is metered per completed PAYG check from measured/declared
#    remote response bytes, rounded up in binary MB (1 MiB), with a separate
#    idempotent network_egress usage event.
# C) Storage is a one-time storage-write fee, not recurring monthly rent.
#
# The patch below is source-only. It does NOT run the test suite.

TARGETS=(
  "crates/application/src/use_cases/charge_for_usage.rs"
  "crates/application/src/use_cases/upload_status_page_logo.rs"
  "crates/application/tests/payg_scheduler_context.rs"
  "crates/api/src/handlers/payg_pricing.rs"
  "crates/worker/src/retention_worker.rs"
  "crates/worker/src/flow_executor.rs"
  "crates/worker/src/main.rs"
  "crates/worker/src/checker.rs"
  "crates/worker/src/proxy_checker.rs"
  "crates/worker/src/http_check_executor.rs"
  "crates/worker/src/tcp_check_executor.rs"
  "crates/worker/src/flow_check_executor.rs"
  "crates/worker/src/dns_check_executor.rs"
  "crates/worker/src/durable_scheduler.rs"
  "crates/worker/tests/retention_worker.rs"
  "crates/worker/tests/http_check.rs"
  "crates/domain/src/payg.rs"
  "crates/infra/src/repositories/postgres_payg_pricing_repository.rs"
  "crates/infra/src/repositories/in_memory_payg_pricing_repository.rs"
  "apps/leptos-dashboard/src/pages/platform_admin.rs"
  "apps/leptos-dashboard/src/pages/pricing.rs"
  "apps/leptos-dashboard/src/api/billing.rs"
)

for path in "${TARGETS[@]}"; do
  if [[ -f "$path" ]]; then
    mkdir -p "$BACKUP_DIR/$(dirname "$path")"
    cp -p "$path" "$BACKUP_DIR/$path"
  fi
done

cat > /tmp/pingvertex-payg-gap-closure.patch <<'PATCH'
diff --git a/apps/leptos-dashboard/src/api/billing.rs b/apps/leptos-dashboard/src/api/billing.rs
index 2091619..33660aa 100644
--- a/apps/leptos-dashboard/src/api/billing.rs
+++ b/apps/leptos-dashboard/src/api/billing.rs
@@ -18,7 +18,6 @@ pub struct AddOn {
 pub struct Subscription {
     pub id: String,
     pub tenant_id: String,
-    pub plan_tier: String,
     pub status: String,
     pub trial_ends_at: Option<String>,
     pub grace_period_ends_at: Option<String>,
@@ -100,23 +99,6 @@ pub struct QuoteRequest {
     pub retention_days: u32,
 }
 #[derive(Debug, Clone, Serialize)]
-pub struct CheckoutRequest {
-    pub plan_tier: String,
-}
-#[derive(Debug, Clone, Deserialize, PartialEq)]
-pub struct CheckoutResponse {
-    pub checkout_session_id: String,
-    pub checkout_url: String,
-}
-#[derive(Debug, Clone, Serialize)]
-pub struct VerifyCheckoutRequest {
-    pub checkout_session_id: String,
-}
-#[derive(Debug, Clone, Deserialize, PartialEq)]
-pub struct VerifyCheckoutResponse {
-    pub status: String,
-}
-#[derive(Debug, Clone, Serialize)]
 pub struct AddOnRequest {
     pub addon_type: String,
     pub quantity: u64,
@@ -230,24 +212,6 @@ pub async fn quote(req: &QuoteRequest) -> Result<Quote, ApiError> {
     );
     client::get_json(&path).await
 }
-pub async fn checkout(plan_tier: &str) -> Result<CheckoutResponse, ApiError> {
-    client::post_json(
-        "/billing/checkout",
-        &CheckoutRequest {
-            plan_tier: plan_tier.to_string(),
-        },
-    )
-    .await
-}
-pub async fn verify_checkout(id: &str) -> Result<VerifyCheckoutResponse, ApiError> {
-    client::post_json(
-        "/billing/checkout/verify",
-        &VerifyCheckoutRequest {
-            checkout_session_id: id.to_string(),
-        },
-    )
-    .await
-}
 pub async fn add_on(req: &AddOnRequest) -> Result<AddOn, ApiError> {
     client::post_json("/billing/addons", req).await
 }
diff --git a/apps/leptos-dashboard/src/pages/platform_admin.rs b/apps/leptos-dashboard/src/pages/platform_admin.rs
index ce788c6..31cd60c 100644
--- a/apps/leptos-dashboard/src/pages/platform_admin.rs
+++ b/apps/leptos-dashboard/src/pages/platform_admin.rs
@@ -214,7 +214,7 @@ pub fn PlatformAdminPage() -> impl IntoView {
                     <section class="card">
                         <div class="section-heading"><div><h2>"PAYG pricing"</h2><p class="muted">"Create a new immutable price version; historical versions cannot be edited."</p></div></div>
                         <div class="form-grid">
-                            <label>"Operation"<select prop:value=pricing_operation on:change=move |e| pricing_operation.set(event_target_value(&e))><option value="http_check.monitoring">"HTTP check"</option><option value="tcp_check.monitoring">"TCP check"</option><option value="dns_check.monitoring">"DNS check"</option><option value="api_flow_check.monitoring">"API flow"</option><option value="proxy_check.monitoring">"Proxy check"</option></select></label>
+                            <label>"Operation"<select prop:value=pricing_operation on:change=move |e| pricing_operation.set(event_target_value(&e))><option value="http_check.monitoring">"HTTP check"</option><option value="tcp_check.monitoring">"TCP check"</option><option value="dns_check.monitoring">"DNS check"</option><option value="api_flow_check.monitoring">"API flow"</option><option value="storage">"Storage write"</option><option value="network_egress">"Network egress"</option></select></label>
                             <label>"Unit price (micro-USD)"<input type="number" min="1" prop:value=pricing_units on:input=move |e| pricing_units.set(event_target_value(&e)) /></label>
                             <label>"Reason"<input placeholder="required change reason" prop:value=pricing_reason on:input=move |e| pricing_reason.set(event_target_value(&e)) /></label>
                             <button class="button button-secondary" on:click=move |_| {
diff --git a/apps/leptos-dashboard/src/pages/pricing.rs b/apps/leptos-dashboard/src/pages/pricing.rs
index 6a94611..2b50384 100644
--- a/apps/leptos-dashboard/src/pages/pricing.rs
+++ b/apps/leptos-dashboard/src/pages/pricing.rs
@@ -17,11 +17,10 @@ fn operation_label(operation: &str) -> String {
         "tcp_check.monitoring" => "TCP — monitoring".into(),
         "dns_check.monitoring" => "DNS — monitoring".into(),
         "api_flow_check.monitoring" => "API flow — monitoring".into(),
-        "proxy_check.monitoring" => "Proxy — monitoring".into(),
         "heartbeat_processing" => "Heartbeat processing".into(),
         "rum_event" => "RUM event".into(),
         "sms" => "SMS".into(),
-        "storage" => "Storage".into(),
+        "storage" => "Storage write".into(),
         "network_egress" => "Network egress".into(),
         _ => operation.to_owned(),
     }
diff --git a/crates/api/src/handlers/payg_pricing.rs b/crates/api/src/handlers/payg_pricing.rs
index c05a931..b8d8cee 100644
--- a/crates/api/src/handlers/payg_pricing.rs
+++ b/crates/api/src/handlers/payg_pricing.rs
@@ -105,8 +105,15 @@ pub async fn payg_pricing(
         prices: rows
             .into_iter()
             .map(|p| PAYGPricingResponse {
-                operation: p.operation,
-                unit_name: p.unit_name,
+                operation: p.operation.clone(),
+                // Storage is intentionally a one-time write charge. The
+                // historical v3 row says MB/month, but immutable pricing rows
+                // cannot be edited; normalize the customer-facing unit here.
+                unit_name: if p.operation == "storage" {
+                    "MB".to_owned()
+                } else {
+                    p.unit_name
+                },
                 unit_price: p.unit_price,
                 currency: p.currency,
                 version: p.version,
diff --git a/crates/application/src/use_cases/charge_for_usage.rs b/crates/application/src/use_cases/charge_for_usage.rs
index 09d02d9..010da3b 100644
--- a/crates/application/src/use_cases/charge_for_usage.rs
+++ b/crates/application/src/use_cases/charge_for_usage.rs
@@ -32,6 +32,9 @@ pub struct ChargeForUsageResult {
 pub struct MeteredAttempt {
     pub attempted: bool,
     pub error: Option<String>,
+    /// Bytes observed leaving the remote HTTP/API target during this attempt.
+    /// The worker converts this to billable `network_egress` MB units.
+    pub egress_bytes: u64,
 }
 
 #[derive(Debug)]
@@ -523,6 +526,7 @@ impl ChargeForUsageUseCase {
                     // execute the external operation again.
                     attempted: true,
                     error: None,
+                    egress_bytes: 0,
                 }),
             )
         })
diff --git a/crates/application/src/use_cases/upload_status_page_logo.rs b/crates/application/src/use_cases/upload_status_page_logo.rs
index c744ab4..f139cc0 100644
--- a/crates/application/src/use_cases/upload_status_page_logo.rs
+++ b/crates/application/src/use_cases/upload_status_page_logo.rs
@@ -122,6 +122,9 @@ where
         }
 
         let logo = LogoImage::new(command.data).map_err(UploadStatusPageLogoError::InvalidLogo)?;
+        // Product decision: `storage` is a one-time storage-write fee, not
+        // recurring monthly rent. The price-book API normalizes the historical
+        // unit label to `MB` for customer-facing copy.
         let storage_units = ((logo.data().len() as u64).div_ceil(1024 * 1024)) as i64;
         let status_page_id = status_page.id;
 
@@ -186,7 +189,7 @@ where
             target_id: status_page.id.0.to_string(),
             metadata: json!({
                 "has_logo": true,
-                "storage_billable_mb": storage_units.max(1),
+                "storage_write_billable_mb": storage_units.max(1),
             }),
             created_at: Utc::now(),
         };
diff --git a/crates/application/tests/payg_scheduler_context.rs b/crates/application/tests/payg_scheduler_context.rs
index c01e6f5..4ba460d 100644
--- a/crates/application/tests/payg_scheduler_context.rs
+++ b/crates/application/tests/payg_scheduler_context.rs
@@ -114,6 +114,7 @@ async fn scheduler_context_resolves_contextual_price_not_generic() {
             Ok::<_, std::convert::Infallible>(MeteredAttempt {
                 attempted: true,
                 error: None,
+                egress_bytes: 0,
             })
         })
         .await
diff --git a/crates/domain/src/payg.rs b/crates/domain/src/payg.rs
index 200baa4..e674a54 100644
--- a/crates/domain/src/payg.rs
+++ b/crates/domain/src/payg.rs
@@ -5,6 +5,11 @@ use std::str::FromStr;
 /// Minimum product-defined wallet top-up: $5 in micro-USD.
 pub const MIN_PAYG_TOP_UP_UNITS: i64 = 5_000_000;
 
+/// PAYG network egress is billed in binary megabytes to match the existing
+/// bounded response-body safety limits and storage metering semantics.
+pub const PAYG_NETWORK_EGRESS_UNIT_BYTES: u64 = 1024 * 1024;
+pub const RETIRED_PAYG_OPERATIONS: &[&str] = &["proxy_check.monitoring"];
+
 /// Canonical server-owned PAYG operation identifiers. Runtime code derives
 /// these values; clients never submit prices or operation identifiers.
 #[derive(Debug, Clone, Copy, PartialEq, Eq)]
@@ -19,7 +24,6 @@ pub enum PaygOperation {
     DnsMonitoring,
     HeartbeatProcessing,
     ApiFlowMonitoring,
-    ProxyMonitoring,
     RumEvent,
     Sms,
     Storage,
@@ -38,7 +42,6 @@ impl PaygOperation {
             Self::DnsMonitoring => "dns_check.monitoring",
             Self::HeartbeatProcessing => "heartbeat_processing",
             Self::ApiFlowMonitoring => "api_flow_check.monitoring",
-            Self::ProxyMonitoring => "proxy_check.monitoring",
             Self::RumEvent => "rum_event",
             Self::Sms => "sms",
             Self::Storage => "storage",
@@ -59,7 +62,6 @@ impl PaygOperation {
         Self::DnsMonitoring,
         Self::HeartbeatProcessing,
         Self::ApiFlowMonitoring,
-        Self::ProxyMonitoring,
         Self::RumEvent,
         Self::Sms,
         Self::Storage,
@@ -110,7 +112,7 @@ mod tests {
     use super::*;
     #[test]
     fn canonical_operation_set_matches_part1_price_book() {
-        assert_eq!(PaygOperation::ALL.len(), 15);
+        assert_eq!(PaygOperation::ALL.len(), 14);
         assert!(PaygOperation::is_canonical("http_check.one_time"));
         assert!(PaygOperation::is_canonical("http_check.monitoring"));
         assert!(PaygOperation::is_canonical("heartbeat_processing"));
diff --git a/crates/infra/src/repositories/in_memory_payg_pricing_repository.rs b/crates/infra/src/repositories/in_memory_payg_pricing_repository.rs
index 640b9c6..df85c35 100644
--- a/crates/infra/src/repositories/in_memory_payg_pricing_repository.rs
+++ b/crates/infra/src/repositories/in_memory_payg_pricing_repository.rs
@@ -139,7 +139,11 @@ impl PAYGPricingRepository for InMemoryPAYGPricingRepository {
         let guard = self.rows.read().await;
         Ok(guard
             .iter()
-            .filter(|r| r.pricing.active && r.pricing.effective_until.is_none())
+            .filter(|r| {
+                r.pricing.active
+                    && r.pricing.effective_until.is_none()
+                    && !domain::RETIRED_PAYG_OPERATIONS.contains(&r.pricing.operation.as_str())
+            })
             .map(|r| r.pricing.clone())
             .collect())
     }
diff --git a/crates/infra/src/repositories/postgres_payg_pricing_repository.rs b/crates/infra/src/repositories/postgres_payg_pricing_repository.rs
index 3bd7ec1..d876b0f 100644
--- a/crates/infra/src/repositories/postgres_payg_pricing_repository.rs
+++ b/crates/infra/src/repositories/postgres_payg_pricing_repository.rs
@@ -155,6 +155,11 @@ impl PAYGPricingRepository for PostgresPAYGPricingRepository {
             r#"
             SELECT * FROM payg_pricing_configs
             WHERE active = true
+              AND NOT EXISTS (
+                  SELECT 1
+                  FROM payg_retired_operations retired
+                  WHERE retired.operation = payg_pricing_configs.operation
+              )
               AND effective_from <= now()
               AND (effective_until IS NULL OR effective_until > now())
             "#,
diff --git a/crates/worker/src/checker.rs b/crates/worker/src/checker.rs
index 97e757d..3518ea8 100644
--- a/crates/worker/src/checker.rs
+++ b/crates/worker/src/checker.rs
@@ -33,6 +33,9 @@ pub struct HttpCheckOutcome {
     pub result: CheckResult,
     pub error_kind: CheckErrorKind,
     pub attempted: bool,
+    /// Bytes observed/declared in the remote HTTP response. This is used only
+    /// for PAYG network-egress metering; the response body remains bounded.
+    pub egress_bytes: u64,
 }
 
 impl HttpCheckOutcome {
@@ -41,6 +44,7 @@ impl HttpCheckOutcome {
             result,
             error_kind,
             attempted: true,
+            egress_bytes: 0,
         }
     }
 
@@ -54,6 +58,7 @@ impl HttpCheckOutcome {
             result: CheckResult::failure(region, latency_ms, error),
             error_kind,
             attempted: false,
+            egress_bytes: 0,
         }
     }
 
@@ -68,6 +73,7 @@ impl HttpCheckOutcome {
             result: CheckResult::failure_with_status(region, status, latency_ms, error),
             error_kind,
             attempted: true,
+            egress_bytes: 0,
         }
     }
 }
@@ -364,6 +370,11 @@ async fn handle_http_response(
         Ok(response) => {
             let status = response.status().as_u16();
 
+            let declared_egress_bytes = if monitor.method == HttpMethod::Head {
+                0
+            } else {
+                response.content_length().unwrap_or(0)
+            };
             if status != monitor.expected_status {
                 let error_msg = format!("expected {}, got {}", monitor.expected_status, status);
                 let result = if include_status_on_mismatch {
@@ -376,24 +387,30 @@ async fn handle_http_response(
                 } else {
                     CheckResult::failure(RegionCode::default_region(), latency_ms, error_msg)
                 };
-
-                return HttpCheckOutcome::new(result, CheckErrorKind::HttpError);
+                let mut outcome = HttpCheckOutcome::new(result, CheckErrorKind::HttpError);
+                outcome.egress_bytes = declared_egress_bytes;
+                return outcome;
             }
 
-            let (headers, body_bytes) = if monitor.assertions.is_empty() {
-                (HashMap::new(), Vec::new())
+            let (headers, body_bytes, egress_bytes) = if monitor.assertions.is_empty() {
+                (HashMap::new(), Vec::new(), declared_egress_bytes)
             } else {
                 let headers = collect_headers(&response);
                 match read_body_bounded(response).await {
-                    Ok(body) => (headers, body),
+                    Ok(body) => {
+                        let observed = body.len() as u64;
+                        (headers, body, observed.max(declared_egress_bytes))
+                    }
                     Err(error_kind) => {
-                        return HttpCheckOutcome::failure_with_status(
+                        let mut outcome = HttpCheckOutcome::failure_with_status(
                             RegionCode::default_region(),
                             status,
                             latency_ms,
                             error_kind,
                             "HTTP response body exceeded the maximum permitted size",
                         );
+                        outcome.egress_bytes = declared_egress_bytes.min(MAX_HTTP_RESPONSE_BODY_BYTES as u64);
+                        return outcome;
                     }
                 }
             };
@@ -413,13 +430,15 @@ async fn handle_http_response(
                         results.iter().filter_map(|r| r.message.clone()).collect();
                     let error_msg = format!("assertions sun gaza: {}", errors.join("; "));
 
-                    return HttpCheckOutcome::failure_with_status(
+                    let mut outcome = HttpCheckOutcome::failure_with_status(
                         RegionCode::default_region(),
                         status,
                         latency_ms,
                         CheckErrorKind::AssertionFailed,
                         error_msg,
                     );
+                    outcome.egress_bytes = egress_bytes;
+                    return outcome;
                 }
             }
 
@@ -430,11 +449,13 @@ async fn handle_http_response(
                 None => (None, None),
             };
 
-            HttpCheckOutcome::new(
+            let mut outcome = HttpCheckOutcome::new(
                 CheckResult::success(RegionCode::default_region(), status, latency_ms)
                     .with_cert_info(cert_expires_at, cert_days_remaining),
                 CheckErrorKind::Unknown,
-            )
+            );
+            outcome.egress_bytes = egress_bytes;
+            outcome
         }
 
         Err(err) => {
diff --git a/crates/worker/src/dns_check_executor.rs b/crates/worker/src/dns_check_executor.rs
index 3743e97..3cb3b5f 100644
--- a/crates/worker/src/dns_check_executor.rs
+++ b/crates/worker/src/dns_check_executor.rs
@@ -110,6 +110,7 @@ impl crate::durable_scheduler::CheckExecutor for DnsCheckExecutor {
         Ok(CheckExecution {
             attempted: outcome.attempted,
             error: None,
+            egress_bytes: 0,
         })
     }
 }
diff --git a/crates/worker/src/durable_scheduler.rs b/crates/worker/src/durable_scheduler.rs
index f49aa35..5a33c59 100644
--- a/crates/worker/src/durable_scheduler.rs
+++ b/crates/worker/src/durable_scheduler.rs
@@ -397,7 +397,7 @@ where
                                 move || {
                                     let executor = executor.clone();
                                     let monitor = monitor.clone();
-                                    async move { execute_with_controls(executor.clone(), monitor.clone(), execution_limiter.clone(), execution_min_interval, execution_retry_limit, execution_retry_base, jitter).await.map(|o| MeteredAttempt { attempted: o.attempted, error: o.error }) }
+                                    async move { execute_with_controls(executor.clone(), monitor.clone(), execution_limiter.clone(), execution_min_interval, execution_retry_limit, execution_retry_base, jitter).await.map(|o| MeteredAttempt { attempted: o.attempted, error: o.error, egress_bytes: o.egress_bytes }) }
                                 },
                             ).await {
                                 Ok((charge_result, attempt)) => {
@@ -592,7 +592,7 @@ where
                                             .as_str()
                                             .to_owned(),
                                         units: 1,
-                                        request_id,
+                                        request_id: request_id.clone(),
                                         monitor_id: Some(monitor.id.0),
                                         execution_id: Some(claim.execution_id),
                                         pricing_context: Some(application::ports::PricingContext {
@@ -727,3 +727,17 @@ where
     .run()
     .await
 }
+
+
+#[cfg(test)]
+mod network_egress_tests {
+    use super::network_egress_units;
+
+    #[test]
+    fn network_egress_uses_binary_mb_and_rounds_up() {
+        assert_eq!(network_egress_units(0), 0);
+        assert_eq!(network_egress_units(1), 1);
+        assert_eq!(network_egress_units(1024 * 1024), 1);
+        assert_eq!(network_egress_units((1024 * 1024) + 1), 2);
+    }
+}
diff --git a/crates/worker/src/flow_check_executor.rs b/crates/worker/src/flow_check_executor.rs
index 7c71ba5..88b612e 100644
--- a/crates/worker/src/flow_check_executor.rs
+++ b/crates/worker/src/flow_check_executor.rs
@@ -86,6 +86,11 @@ impl crate::durable_scheduler::CheckExecutor for FlowCheckExecutor {
 
         let result = execute_flow(&flow, &self.http_client).await;
         let attempted = result.outcomes.iter().any(|o| o.attempted);
+        let egress_bytes = result
+            .outcomes
+            .iter()
+            .map(|outcome| outcome.egress_bytes)
+            .sum();
 
         let check_id = self.id_generator.next_check_id().await;
         let check = build_check(check_id, monitor, &result);
@@ -118,6 +123,7 @@ impl crate::durable_scheduler::CheckExecutor for FlowCheckExecutor {
         Ok(CheckExecution {
             attempted,
             error: None,
+            egress_bytes,
         })
     }
 }
diff --git a/crates/worker/src/flow_executor.rs b/crates/worker/src/flow_executor.rs
index ed5b2b0..e9a03e2 100644
--- a/crates/worker/src/flow_executor.rs
+++ b/crates/worker/src/flow_executor.rs
@@ -74,6 +74,8 @@ pub struct FlowStepOutcome {
     pub attempted: bool,
     pub status_code: Option<u16>,
     pub error: Option<String>,
+    /// Bytes observed/declared in this HTTP step for PAYG egress metering.
+    pub egress_bytes: u64,
     pub assertion_results: Vec<domain::value_objects::AssertionEvaluationResult>,
 }
 
@@ -186,6 +188,7 @@ pub async fn execute_flow_with_resolver(
                 attempted: in_flight_attempted.load(Ordering::Acquire),
                 status_code: None,
                 error: Some("flow exceeded 60s total timeout".to_string()),
+                egress_bytes: 0,
                 assertion_results: Vec::new(),
             });
 
@@ -268,6 +271,7 @@ async fn execute_step(
         attempted: false,
         status_code: None,
         error: Some(error),
+        egress_bytes: 0,
         assertion_results: Vec::new(),
     };
 
@@ -350,6 +354,11 @@ async fn execute_step(
     };
 
     let status = response.status().as_u16();
+    let declared_egress_bytes = if step.method == domain::HttpMethod::Head {
+        0
+    } else {
+        response.content_length().unwrap_or(0)
+    };
 
     let headers: HashMap<String, String> = response
         .headers()
@@ -369,14 +378,14 @@ async fn execute_step(
     let body_bytes = match read_body_bounded(response).await {
         Ok(bytes) => bytes,
         Err(()) => {
-            return (
-                failed_outcome(
-                    "HTTP response body exceeded the maximum permitted size".to_string(),
-                ),
-                None,
+            let mut outcome = failed_outcome(
+                "HTTP response body exceeded the maximum permitted size".to_string(),
             );
+            outcome.egress_bytes = declared_egress_bytes.min(MAX_STEP_RESPONSE_BODY_BYTES as u64);
+            return (outcome, None);
         }
     };
+    let egress_bytes = (body_bytes.len() as u64).max(declared_egress_bytes);
     let body_string = String::from_utf8_lossy(&body_bytes).into_owned();
 
     // e. Ajiye StepResult domin steps na gaba su iya amfani dashi.
@@ -417,6 +426,7 @@ async fn execute_step(
             passed: all_passed,
             status_code: Some(status),
             error,
+            egress_bytes,
             assertion_results,
         },
         Some(step_result),
diff --git a/crates/worker/src/http_check_executor.rs b/crates/worker/src/http_check_executor.rs
index 49f3ef1..dbc9090 100644
--- a/crates/worker/src/http_check_executor.rs
+++ b/crates/worker/src/http_check_executor.rs
@@ -120,6 +120,7 @@ impl crate::durable_scheduler::CheckExecutor for HttpCheckExecutor {
         Ok(CheckExecution {
             attempted: outcome.attempted,
             error: None,
+            egress_bytes: outcome.egress_bytes,
         })
     }
 }
diff --git a/crates/worker/src/main.rs b/crates/worker/src/main.rs
index 7ccf3f3..5d6a259 100644
--- a/crates/worker/src/main.rs
+++ b/crates/worker/src/main.rs
@@ -15,7 +15,7 @@ use infra::{
     PostgresMonitorRepository, PostgresNotificationChannelRepository,
     PostgresNotificationOutboxRepository, PostgresPAYGPricingRepository,
     PostgresPricingConfigRepository, PostgresProxyCheckJobRepository, PostgresSchedulingRepository,
-    PostgresStatusPageRepository, PostgresSubscriptionRepository,
+    PostgresStatusPageRepository,
     PostgresTenantMembershipRepository, PostgresTenantRepository, PostgresUsageEventRepository,
     RedisStatusPageCache, UuidIdGenerator, create_pool,
 };
@@ -110,7 +110,6 @@ async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
     let id_generator = Arc::new(UuidIdGenerator::new());
 
     let tenant_repository = Arc::new(PostgresTenantRepository::new(pool.clone()));
-    let subscription_repository = Arc::new(PostgresSubscriptionRepository::new(pool.clone()));
     let wallet_repository = Arc::new(infra::PostgresWalletRepository::new(pool.clone()));
     let financial_audit_repository = Arc::new(infra::PostgresFinancialAuditLogRepository::new(
         pool.clone(),
@@ -297,14 +296,13 @@ async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
 
     let retention_worker = RetentionWorker::new(
         tenant_repository.clone(),
-        subscription_repository.clone(),
         history_retention_repository,
         pricing_config_repository.clone(),
         worker::retention_worker::DEFAULT_BATCH_SIZE,
         worker::retention_worker::DEFAULT_RETENTION_INTERVAL,
         worker_config.retention_max_concurrent_tenants,
-    )
-    .with_wallet_repository(wallet_repository.clone());
+        wallet_repository.clone(),
+    );
     let retention_cancel = cancel_token.clone();
     tokio::spawn(async move {
         if let Err(err) = retention_worker.run_forever(retention_cancel).await {
diff --git a/crates/worker/src/proxy_checker.rs b/crates/worker/src/proxy_checker.rs
index 3db3d57..7a9f4ae 100644
--- a/crates/worker/src/proxy_checker.rs
+++ b/crates/worker/src/proxy_checker.rs
@@ -743,10 +743,10 @@ where
                     || async {
                         if had_existing_result {
                             let result = entry.result().cloned().unwrap_or_else(|| proxy_result(&entry, Instant::now(), false, false, Some("persisted proxy result missing".to_owned())));
-                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted: true, error: None }, result))
+                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted: true, error: None, egress_bytes: 0 }, result))
                         } else {
                             let (result, attempted) = check_single_entry(&entry).await;
-                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted, error: None }, result))
+                            Ok::<_, std::convert::Infallible>((MeteredAttempt { attempted, error: None, egress_bytes: 0 }, result))
                         }
                     },
                 ).await {
diff --git a/crates/worker/src/retention_worker.rs b/crates/worker/src/retention_worker.rs
index 99c2de5..501735e 100644
--- a/crates/worker/src/retention_worker.rs
+++ b/crates/worker/src/retention_worker.rs
@@ -5,11 +5,9 @@ use std::time::Duration;
 
 use application::{
     ports::{
-        HistoryRetentionRepository, PricingConfigRepository, SubscriptionRepository,
-        TenantRepository, WalletRepository,
+        HistoryRetentionRepository, PricingConfigRepository, TenantRepository,
+        WalletRepository,
     },
-    queries::ResolveEntitlements,
-    use_cases::resolve_entitlements::ResolveEntitlementsUseCase,
 };
 use chrono::{Duration as ChronoDuration, Utc};
 use tokio::{sync::Semaphore, task::JoinSet, time};
@@ -26,59 +24,47 @@ pub struct SweepSummary {
     pub checks_deleted: u64,
 }
 
-pub struct RetentionWorker<T, S, H, P>
+pub struct RetentionWorker<T, H, P>
 where
     T: TenantRepository,
-    S: SubscriptionRepository + 'static,
     H: HistoryRetentionRepository + 'static,
     P: PricingConfigRepository + 'static,
 {
     tenant_repository: Arc<T>,
-    subscription_repository: Arc<S>,
     history_repository: Arc<H>,
     pricing_repository: Arc<P>,
     batch_size: u32,
     interval: Duration,
     max_concurrent_tenants: usize,
-    wallet_repository: Option<Arc<dyn WalletRepository>>,
+    wallet_repository: Arc<dyn WalletRepository>,
 }
 
-impl<T, S, H, P> RetentionWorker<T, S, H, P>
+impl<T, H, P> RetentionWorker<T, H, P>
 where
     T: TenantRepository,
-    S: SubscriptionRepository + 'static,
     H: HistoryRetentionRepository + 'static,
     P: PricingConfigRepository + 'static,
 {
     pub fn new(
         tenant_repository: Arc<T>,
-        subscription_repository: Arc<S>,
         history_repository: Arc<H>,
         pricing_repository: Arc<P>,
         batch_size: u32,
         interval: Duration,
         max_concurrent_tenants: usize,
+        wallet_repository: Arc<dyn WalletRepository>,
     ) -> Self {
         Self {
             tenant_repository,
-            subscription_repository,
             history_repository,
             pricing_repository,
             batch_size,
             interval,
             max_concurrent_tenants,
-            wallet_repository: None,
+            wallet_repository,
         }
     }
 
-    /// PAYG runtime retention source. When configured, subscription plans are
-    /// ignored and retention is derived from the wallet activation state plus
-    /// the active PAYG pricing configuration.
-    pub fn with_wallet_repository(mut self, repository: Arc<dyn WalletRepository>) -> Self {
-        self.wallet_repository = Some(repository);
-        self
-    }
-
     /// Runs one full retention sweep over all tenants with bounded
     /// concurrency. Each tenant is processed independently; failures for
     /// an individual tenant are logged and skipped.
@@ -91,7 +77,6 @@ where
 
         for tenant in tenants {
             let permit = semaphore.clone().acquire_owned().await?;
-            let subscription_repository = self.subscription_repository.clone();
             let history_repository = self.history_repository.clone();
             let pricing_repository = self.pricing_repository.clone();
             let wallet_repository = self.wallet_repository.clone();
@@ -132,27 +117,6 @@ where
                     } else {
                         config.free_tier.retention_days
                     }
-                } else {
-                    // Legacy/test compatibility only. Production wires the wallet
-                    // repository, so subscription tiers cannot affect runtime.
-                    let plan = match ResolveEntitlementsUseCase::new(subscription_repository)
-                        .with_pricing_config_repository(pricing_repository)
-                        .execute(ResolveEntitlements {
-                            tenant_id: tenant.id,
-                        })
-                        .await
-                    {
-                        Ok(plan) => plan,
-                        Err(err) => {
-                            error!(
-                                tenant_id = ?tenant.id,
-                                error = %err,
-                                "failed to resolve legacy retention entitlement; skipping retention"
-                            );
-                            return;
-                        }
-                    };
-                    plan.history_retention_days
                 };
                 let cutoff = Utc::now() - ChronoDuration::days(retention_days as i64);
 
diff --git a/crates/worker/src/tcp_check_executor.rs b/crates/worker/src/tcp_check_executor.rs
index 6b10e52..cad5f95 100644
--- a/crates/worker/src/tcp_check_executor.rs
+++ b/crates/worker/src/tcp_check_executor.rs
@@ -110,6 +110,7 @@ impl crate::durable_scheduler::CheckExecutor for TcpCheckExecutor {
         Ok(CheckExecution {
             attempted: outcome.attempted,
             error: None,
+            egress_bytes: 0,
         })
     }
 }
diff --git a/crates/worker/tests/http_check.rs b/crates/worker/tests/http_check.rs
index 61581d6..ba931d3 100644
--- a/crates/worker/tests/http_check.rs
+++ b/crates/worker/tests/http_check.rs
@@ -85,6 +85,7 @@ async fn success_returns_200_and_no_error() {
     assert!(outcome.result.success);
     assert_eq!(outcome.result.status_code, Some(200));
     assert!(outcome.result.error.is_none());
+    assert_eq!(outcome.egress_bytes, 2);
 }
 
 #[tokio::test]
diff --git a/crates/worker/tests/retention_worker.rs b/crates/worker/tests/retention_worker.rs
index 79d774e..df9d6a7 100644
--- a/crates/worker/tests/retention_worker.rs
+++ b/crates/worker/tests/retention_worker.rs
@@ -2,16 +2,16 @@ use std::sync::Arc;
 use std::sync::atomic::{AtomicUsize, Ordering};
 use std::time::Duration;
 
-use application::ports::{HistoryRetentionRepository, SubscriptionRepository, TenantRepository};
+use application::ports::{HistoryRetentionRepository, TenantRepository, WalletRepository};
 use async_trait::async_trait;
 use chrono::{DateTime, Duration as ChronoDuration, Utc};
 use domain::{
-    Check, CheckStatus, ErrorType, PlanTier, RegionCode, Subscription, Tenant,
-    shared::ids::{CheckId, MonitorId, SubscriptionId, TenantId, UserId},
+    Check, CheckStatus, ErrorType, RegionCode, Tenant,
+    shared::ids::{CheckId, MonitorId, TenantId, UserId},
 };
 use infra::{
     InMemoryHistoryRetentionRepository, InMemoryPricingConfigRepository,
-    InMemorySubscriptionRepository, InMemoryTenantRepository,
+    InMemoryTenantRepository, InMemoryWalletRepository,
 };
 use uuid::Uuid;
 use worker::{RetentionWorker, retention_worker::DEFAULT_BATCH_SIZE};
@@ -36,50 +36,55 @@ fn tenant(name: &str) -> Tenant {
 }
 
 #[tokio::test]
-async fn retention_deletes_older_than_cutoff_per_plan() {
+async fn retention_deletes_older_than_cutoff_per_payg_entitlement() {
     let tenant_repo = Arc::new(InMemoryTenantRepository::new());
-    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());
+    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
     let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());
 
     let free_tenant = tenant("Free Tenant");
-    let starter_tenant = tenant("Starter Tenant");
+    let paid_tenant = tenant("Paid Tenant");
     tenant_repo.create(&free_tenant).await.unwrap();
-    tenant_repo.create(&starter_tenant).await.unwrap();
-
-    subscription_repo
-        .create(&Subscription::new(
-            SubscriptionId::new(),
-            free_tenant.id,
-            PlanTier::Free,
-        ))
+    tenant_repo.create(&paid_tenant).await.unwrap();
+
+    // Unfunded tenants use the PAYG Free retention. A funded wallet activates
+    // PAYG and uses the configured maximum retention.
+    wallet_repo
+        .get_or_create(paid_tenant.id.0, "USD")
         .await
         .unwrap();
-    subscription_repo
-        .create(&Subscription::new(
-            SubscriptionId::new(),
-            starter_tenant.id,
-            PlanTier::Starter,
-        ))
+    wallet_repo
+        .credit(
+            paid_tenant.id.0,
+            1_000_000,
+            domain::WalletTransactionType::TopUp,
+            "retention-test-topup",
+            Some("retention test"),
+        )
         .await
         .unwrap();
 
-    // Free retains 2 days; Starter retains 30 days.
+    let pricing = Arc::new(
+        InMemoryPricingConfigRepository::new()
+            .with_config(domain::pricing::PricingConfig::default_config()),
+    );
+
+    // Free retains 2 days; funded PAYG retains the configured 3650 days.
     let old_free = sample_check_older_than_days(3);
     let recent_free = sample_check_older_than_days(0);
-    let old_starter = sample_check_older_than_days(10);
+    let old_paid = sample_check_older_than_days(10);
 
     history_repo.seed_check(old_free.clone(), free_tenant.id);
     history_repo.seed_check(recent_free.clone(), free_tenant.id);
-    history_repo.seed_check(old_starter.clone(), starter_tenant.id);
+    history_repo.seed_check(old_paid.clone(), paid_tenant.id);
 
     let worker = RetentionWorker::new(
         tenant_repo,
-        subscription_repo,
         history_repo.clone(),
-        Arc::new(InMemoryPricingConfigRepository::default()),
+        pricing,
         100,
         Duration::from_secs(3600),
         2,
+        wallet_repo,
     );
 
     let summary = worker.run_sweep().await.unwrap();
@@ -89,7 +94,7 @@ async fn retention_deletes_older_than_cutoff_per_plan() {
 
     let remaining_ids = history_repo.list_check_ids();
     assert!(remaining_ids.contains(&recent_free.id));
-    assert!(remaining_ids.contains(&old_starter.id));
+    assert!(remaining_ids.contains(&old_paid.id));
     assert!(!remaining_ids.contains(&old_free.id));
 }
 
@@ -127,19 +132,15 @@ async fn batching_does_not_delete_more_or_less_than_expected() {
 #[tokio::test]
 async fn running_twice_is_idempotent() {
     let tenant_repo = Arc::new(InMemoryTenantRepository::new());
-    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());
+    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
     let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());
 
     let tenant = tenant("Idempotent Tenant");
     tenant_repo.create(&tenant).await.unwrap();
-    subscription_repo
-        .create(&Subscription::new(
-            SubscriptionId::new(),
-            tenant.id,
-            PlanTier::Free,
-        ))
-        .await
-        .unwrap();
+    let pricing = Arc::new(
+        InMemoryPricingConfigRepository::new()
+            .with_config(domain::pricing::PricingConfig::default_config()),
+    );
 
     // Free retention is 2 days. Keep one fresh check and two checks beyond the cutoff.
     let old1 = sample_check_older_than_days(3);
@@ -152,12 +153,15 @@ async fn running_twice_is_idempotent() {
 
     let worker = RetentionWorker::new(
         tenant_repo,
-        subscription_repo,
         history_repo.clone(),
-        Arc::new(InMemoryPricingConfigRepository::default()),
+        Arc::new(
+            InMemoryPricingConfigRepository::new()
+                .with_config(domain::pricing::PricingConfig::default_config()),
+        ),
         DEFAULT_BATCH_SIZE,
         Duration::from_secs(3600),
         2,
+        wallet_repo,
     );
 
     let first = worker.run_sweep().await.unwrap();
@@ -217,20 +221,12 @@ async fn bounded_concurrency_never_exceeds_configured_limit() {
     const MAX_CONCURRENT_TENANTS: usize = 3;
 
     let tenant_repo = Arc::new(InMemoryTenantRepository::new());
-    let subscription_repo = Arc::new(InMemorySubscriptionRepository::new());
+    let wallet_repo = Arc::new(InMemoryWalletRepository::default());
     let history_repo = Arc::new(InMemoryHistoryRetentionRepository::new());
 
     for i in 0..TENANT_COUNT {
         let t = tenant(&format!("Concurrency Tenant {i}"));
         tenant_repo.create(&t).await.unwrap();
-        subscription_repo
-            .create(&Subscription::new(
-                SubscriptionId::new(),
-                t.id,
-                PlanTier::Free,
-            ))
-            .await
-            .unwrap();
         history_repo.seed_check(sample_check_older_than_days(10), t.id);
     }
 
@@ -243,12 +239,15 @@ async fn bounded_concurrency_never_exceeds_configured_limit() {
 
     let worker = RetentionWorker::new(
         tenant_repo,
-        subscription_repo,
         tracking_repo.clone(),
-        Arc::new(InMemoryPricingConfigRepository::default()),
+        Arc::new(
+            InMemoryPricingConfigRepository::new()
+                .with_config(domain::pricing::PricingConfig::default_config()),
+        ),
         100,
         Duration::from_secs(3600),
         MAX_CONCURRENT_TENANTS,
+        wallet_repo,
     );
 
     let summary = worker.run_sweep().await.unwrap();
PATCH

echo "Checking PAYG gap-closure patch..."
git apply --check --whitespace=fix /tmp/pingvertex-payg-gap-closure.patch

echo "Applying PAYG gap-closure patch..."
git apply --whitespace=fix /tmp/pingvertex-payg-gap-closure.patch

rm -f /tmp/pingvertex-payg-gap-closure.patch

echo
echo "PAYG gap-closure source changes applied."
echo "Backup: $REPO/$BACKUP_DIR"
echo
echo "Static verification:"
if grep -RIn --exclude-dir=.git 'ProxyMonitoring' crates/domain crates/worker crates/application apps/leptos-dashboard/src 2>/dev/null; then
  echo "ERROR: retired ProxyMonitoring symbol still exists." >&2
  exit 1
fi
if grep -RIn --exclude-dir=.git 'pub async fn checkout(plan_tier' apps/leptos-dashboard/src 2>/dev/null; then
  echo "ERROR: retired frontend checkout API still exists." >&2
  exit 1
fi
if grep -n 'SubscriptionRepository' crates/worker/src/retention_worker.rs >/dev/null 2>&1; then
  echo "ERROR: RetentionWorker still depends on SubscriptionRepository." >&2
  exit 1
fi
echo "OK: retired proxy symbol, checkout API, and retention subscription dependency checks passed."
echo
echo "Next: run the project's required formatting, clippy, and tests locally."
