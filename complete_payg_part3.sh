#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

if [[ ! -f Cargo.toml ]]; then
  echo "ERROR: Cargo.toml not found. Run this script from the Uptime Monitor project." >&2
  exit 1
fi

command -v patch >/dev/null 2>&1 || {
  echo "ERROR: 'patch' command is required." >&2
  exit 1
}

PATCH_FILE="$(mktemp "${TMPDIR:-/tmp}/uptime-monitor-part3-XXXXXX.patch")"
BACKUP_DIR=".part3-completion-backup-$(date +%Y%m%d-%H%M%S)"
trap 'rm -f "$PATCH_FILE"' EXIT

cat > "$PATCH_FILE" <<'PATCH_EOF'
--- a/apps/leptos-dashboard/src/api/wallet.rs
+++ b/apps/leptos-dashboard/src/api/wallet.rs
@@ -65,6 +65,10 @@
     pub payment_method: Option<String>,
     pub daily_top_up_limit_units: i64,
     pub monthly_top_up_limit_units: i64,
+    pub daily_spend_limit_units: Option<i64>,
+    pub monthly_spend_limit_units: Option<i64>,
+    pub daily_usage_alert_threshold_units: Option<i64>,
+    pub monthly_usage_alert_threshold_units: Option<i64>,
     pub retry_limit: i32,
 }
 #[derive(Debug, Clone, Serialize)]
@@ -77,6 +81,10 @@
     pub payment_method: Option<String>,
     pub daily_top_up_limit_units: i64,
     pub monthly_top_up_limit_units: i64,
+    pub daily_spend_limit_units: Option<i64>,
+    pub monthly_spend_limit_units: Option<i64>,
+    pub daily_usage_alert_threshold_units: Option<i64>,
+    pub monthly_usage_alert_threshold_units: Option<i64>,
     pub retry_limit: i32,
 }
 #[derive(Debug, Clone, Deserialize, PartialEq)]
--- a/apps/leptos-dashboard/src/pages/bulk_import.rs
+++ b/apps/leptos-dashboard/src/pages/bulk_import.rs
@@ -14,6 +14,20 @@
     )
 }
 
+fn error_csv(value: &monitoring::BulkImportPreview) -> String {
+    let mut csv = String::from("row,error\n");
+    for item in &value.errors {
+        for message in &item.errors {
+            let escaped = message.replace('"', "\"\"");
+            csv.push_str(&format!("{},\"{}\"\n", item.row, escaped));
+        }
+    }
+    format!(
+        "data:text/csv;charset=utf-8,{}",
+        js_sys::encode_uri_component(&csv).as_string().unwrap_or_default()
+    )
+}
+
 #[component]
 pub fn BulkImportPage() -> impl IntoView {
     let preview = RwSignal::new(None::<monitoring::BulkImportPreview>);
@@ -159,7 +173,15 @@
                         <div><strong>{value.total_rows}</strong><span>"Rows"</span></div>
                         <div><strong>{value.valid_rows}</strong><span>"Valid"</span></div>
                         <div><strong>{value.invalid_rows}</strong><span>"Invalid"</span></div>
+                        <div><strong>{value.estimated_execution_quantity}</strong><span>"Estimated checks"</span></div>
                         <div><strong>{format_units(value.estimated_payg_cost_units, &value.currency)}</strong><span>"Estimated usage"</span></div>
+                        <div><strong>{value.pricing_version.map(|v| format!("v{v}")).unwrap_or_else(|| "mixed".to_owned())}</strong><span>"Pricing version"</span></div>
+                    </div>
+                    <div class="stack-form">
+                        <p class="muted">"Pricing is server-resolved and pinned in the import job. Unit prices:"</p>
+                        <ul>
+                            {value.pricing_versions.iter().map(|p| view! { <li>{format!("{} — {} per check (v{})", p.operation, format_units(p.unit_price, &p.currency), p.version)}</li> }).collect_view()}
+                        </ul>
                     </div>
                     {if value.errors.is_empty() {
                         view! { <p class="muted">"No validation errors."</p> }.into_any()
@@ -173,6 +195,15 @@
                             </details>
                         }.into_any()
                     }}
+                    {if value.errors.is_empty() {
+                        ().into_any()
+                    } else {
+                        view! {
+                            <a class="btn" download="bulk-import-errors.csv" href=move || error_csv(&value)>
+                                "Download errors"
+                            </a>
+                        }.into_any()
+                    }}
                     <button class="button" disabled=move || busy.get() || value.valid_rows == 0 on:click=confirm_import>
                         "Confirm and import"
                     </button>
--- a/apps/leptos-dashboard/src/pages/wallet.rs
+++ b/apps/leptos-dashboard/src/pages/wallet.rs
@@ -24,6 +24,10 @@
     let payment_method = RwSignal::new(String::new());
     let daily_limit = RwSignal::new(100_i64);
     let monthly_limit = RwSignal::new(500_i64);
+    let daily_spend_limit = RwSignal::new(0_i64);
+    let monthly_spend_limit = RwSignal::new(0_i64);
+    let daily_usage_alert = RwSignal::new(0_i64);
+    let monthly_usage_alert = RwSignal::new(0_i64);
     let retry_limit = RwSignal::new(3_i32);
     let low_threshold = RwSignal::new(5_i64);
     let critical_threshold = RwSignal::new(1_i64);
@@ -52,6 +56,10 @@
                     payment_method.set(v.payment_method.clone().unwrap_or_default());
                     daily_limit.set(v.daily_top_up_limit_units / 1_000_000);
                     monthly_limit.set(v.monthly_top_up_limit_units / 1_000_000);
+                    daily_spend_limit.set(v.daily_spend_limit_units.unwrap_or(0) / 1_000_000);
+                    monthly_spend_limit.set(v.monthly_spend_limit_units.unwrap_or(0) / 1_000_000);
+                    daily_usage_alert.set(v.daily_usage_alert_threshold_units.unwrap_or(0) / 1_000_000);
+                    monthly_usage_alert.set(v.monthly_usage_alert_threshold_units.unwrap_or(0) / 1_000_000);
                     retry_limit.set(v.retry_limit);
                     low_threshold.set(v.low_balance_threshold_units / 1_000_000);
                     critical_threshold.set(v.critical_balance_threshold_units / 1_000_000);
@@ -113,6 +121,13 @@
             return;
         }
 
+        let daily_spend = daily_spend_limit.get();
+        let monthly_spend = monthly_spend_limit.get();
+        if daily_spend > 0 && monthly_spend > 0 && monthly_spend < daily_spend {
+            error.set(Some("Monthly spend cap must be at least the daily spend cap.".to_owned()));
+            return;
+        }
+
         loading.set(true);
 
         let req = wallet::WalletSettingsRequest {
@@ -124,6 +139,10 @@
             payment_method: (!payment.is_empty()).then_some(payment),
             daily_top_up_limit_units: daily * 1_000_000,
             monthly_top_up_limit_units: monthly * 1_000_000,
+            daily_spend_limit_units: (daily_spend_limit.get() > 0).then(|| daily_spend_limit.get() * 1_000_000),
+            monthly_spend_limit_units: (monthly_spend_limit.get() > 0).then(|| monthly_spend_limit.get() * 1_000_000),
+            daily_usage_alert_threshold_units: (daily_usage_alert.get() > 0).then(|| daily_usage_alert.get() * 1_000_000),
+            monthly_usage_alert_threshold_units: (monthly_usage_alert.get() > 0).then(|| monthly_usage_alert.get() * 1_000_000),
             retry_limit: retries,
         };
 
@@ -136,6 +155,10 @@
                     payment_method.set(v.payment_method.clone().unwrap_or_default());
                     daily_limit.set(v.daily_top_up_limit_units / 1_000_000);
                     monthly_limit.set(v.monthly_top_up_limit_units / 1_000_000);
+                    daily_spend_limit.set(v.daily_spend_limit_units.unwrap_or(0) / 1_000_000);
+                    monthly_spend_limit.set(v.monthly_spend_limit_units.unwrap_or(0) / 1_000_000);
+                    daily_usage_alert.set(v.daily_usage_alert_threshold_units.unwrap_or(0) / 1_000_000);
+                    monthly_usage_alert.set(v.monthly_usage_alert_threshold_units.unwrap_or(0) / 1_000_000);
                     retry_limit.set(v.retry_limit);
                     low_threshold.set(v.low_balance_threshold_units / 1_000_000);
                     critical_threshold.set(v.critical_balance_threshold_units / 1_000_000);
@@ -403,6 +426,26 @@
                         </label>
 
                         <label>
+                            "Daily spend cap ($, 0 = unlimited)"
+                            <input type="number" min="0" prop:value=move || daily_spend_limit.get().to_string() disabled=move || loading.get() on:input=move |event| { if let Ok(v)=event_target_value(&event).parse::<i64>() { daily_spend_limit.set(v.max(0)); } } />
+                        </label>
+
+                        <label>
+                            "Monthly spend cap ($, 0 = unlimited)"
+                            <input type="number" min="0" prop:value=move || monthly_spend_limit.get().to_string() disabled=move || loading.get() on:input=move |event| { if let Ok(v)=event_target_value(&event).parse::<i64>() { monthly_spend_limit.set(v.max(0)); } } />
+                        </label>
+
+                        <label>
+                            "Daily usage alert ($, 0 = off)"
+                            <input type="number" min="0" prop:value=move || daily_usage_alert.get().to_string() disabled=move || loading.get() on:input=move |event| { if let Ok(v)=event_target_value(&event).parse::<i64>() { daily_usage_alert.set(v.max(0)); } } />
+                        </label>
+
+                        <label>
+                            "Monthly usage alert ($, 0 = off)"
+                            <input type="number" min="0" prop:value=move || monthly_usage_alert.get().to_string() disabled=move || loading.get() on:input=move |event| { if let Ok(v)=event_target_value(&event).parse::<i64>() { monthly_usage_alert.set(v.max(0)); } } />
+                        </label>
+
+                        <label>
                             "Retry limit"
                             <input
                                 type="number"
@@ -435,7 +478,7 @@
                     </label>
 
                     <p class="muted">
-                        "A saved payment method is required to enable automatic charging. Daily, monthly and retry limits are enforced server-side."
+                        "A saved payment method is required to enable automatic charging. Spend caps and usage alerts are enforced server-side; 0 disables an optional cap/alert."
                     </p>
 
                     <div class="inline-form">
@@ -494,7 +537,7 @@
                 <div class="section-heading">
                     <div>
                         <h2>"Wallet notifications"</h2>
-                        <p class="muted">"Low-balance, critical-balance and payment events for this wallet."</p>
+                        <p class="muted">"Low-balance, usage-threshold and payment events for this wallet."</p>
                     </div>
                 </div>
                 {move || if notifications.get().is_empty() {
--- a/crates/api/src/auth_state.rs
+++ b/crates/api/src/auth_state.rs
@@ -10,7 +10,7 @@
 };
 
 use application::ports::{
-    BackupCodeRepository, EmailVerificationMailer, EmailVerificationTokenRepository,
+    AuditLogEntryRepository, BackupCodeRepository, EmailVerificationMailer, EmailVerificationTokenRepository,
     IdTokenVerifier, OAuthIdentityRepository, PasswordHasher, PasswordResetMailer,
     PasswordResetMailerResult, PendingAuthCache, PendingAuthCacheResult, RateLimiter,
     RateLimiterResult, RegistrationTransaction, ResetPasswordTokenRepository, SecretCipher,
@@ -315,6 +315,16 @@
     }
 }
 
+#[derive(Clone, Default)]
+struct NoopAuditLogEntryRepository;
+
+#[async_trait]
+impl AuditLogEntryRepository for NoopAuditLogEntryRepository {
+    async fn create(&self, _entry: &domain::AuditLogEntry) -> RepoResult<()> { Ok(()) }
+    async fn list_by_tenant(&self, _tenant_id: domain::TenantId) -> RepoResult<Vec<domain::AuditLogEntry>> { Ok(Vec::new()) }
+    async fn list_by_actor(&self, _actor_user_id: UserId) -> RepoResult<Vec<domain::AuditLogEntry>> { Ok(Vec::new()) }
+}
+
 #[derive(Clone)]
 pub struct AuthState {
     pub user_repository: Arc<dyn UserRepository>,
@@ -339,6 +349,7 @@
     pub google_id_token_verifier: Arc<dyn IdTokenVerifier>,
     pub apple_id_token_verifier: Arc<dyn IdTokenVerifier>,
     pub oauth_identity_repository: Arc<dyn OAuthIdentityRepository>,
+    pub audit_log_entry_repository: Arc<dyn application::ports::AuditLogEntryRepository>,
 }
 
 impl AuthState {
@@ -379,6 +390,7 @@
             google_id_token_verifier: Arc::new(NoopIdTokenVerifier),
             apple_id_token_verifier: Arc::new(NoopIdTokenVerifier),
             oauth_identity_repository: Arc::new(NoopOAuthIdentityRepository),
+            audit_log_entry_repository: Arc::new(NoopAuditLogEntryRepository),
         }
     }
 
@@ -442,6 +454,14 @@
     /// omitting the call — that keeps the failure mode explicit
     /// ("Google sign-in is not configured") instead of falling back to
     /// the default `NoopIdTokenVerifier`'s generic message.
+    pub fn with_audit_log_repository<R>(mut self, repository: R) -> Self
+    where
+        R: AuditLogEntryRepository + 'static,
+    {
+        self.audit_log_entry_repository = Arc::new(repository);
+        self
+    }
+
     pub fn with_oauth<G, A, O>(
         mut self,
         google_verifier: G,
--- a/crates/api/src/handlers/add_member.rs
+++ b/crates/api/src/handlers/add_member.rs
@@ -33,6 +33,7 @@
     );
 
     let created = use_case
+        .with_audit_log_repository(auth_state.audit_log_entry_repository.clone())
         .execute(
             AddMemberCommand {
                 user_id: UserId(payload.user_id),
--- a/crates/api/src/handlers/admin_pricing.rs
+++ b/crates/api/src/handlers/admin_pricing.rs
@@ -3,7 +3,8 @@
     extract::{Extension, Path, State},
     http::StatusCode,
 };
-use domain::PaygOperation;
+use domain::{AccountType, MonitorKind, PaygOperation};
+use domain::value_objects::RegionCode;
 use domain::pricing::{FreeTierDefaults, PricingConfig};
 use domain::shared::types::AuthenticatedUser;
 use std::str::FromStr;
@@ -24,6 +25,11 @@
     pub unit_price: i64,
     pub currency: String,
     pub effective_from: Option<chrono::DateTime<chrono::Utc>>,
+    pub account_type: Option<String>,
+    pub region: Option<String>,
+    pub monitor_kind: Option<String>,
+    pub min_quantity: Option<u64>,
+    pub max_quantity: Option<u64>,
     pub reason: String,
 }
 
@@ -71,14 +77,39 @@
 ) -> Result<(StatusCode, Json<PaygPriceResponse>), ApiError> {
     require_platform_admin(&user)?;
     let operation = validate_payg_price_request(&payload)?;
+    let account_type = payload.account_type.as_deref().map(str::trim).filter(|v| !v.is_empty())
+        .map(AccountType::from_str).transpose()
+        .map_err(|_| ApiError::bad_request("account_type must be individual or company"))?;
+    let region = payload.region.as_deref().map(str::trim).filter(|v| !v.is_empty())
+        .map(RegionCode::new).transpose()
+        .map_err(|_| ApiError::bad_request("unsupported pricing region"))?;
+    let monitor_kind = payload.monitor_kind.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|v| match v.to_ascii_lowercase().as_str() {
+        "http" => Ok(MonitorKind::Http),
+        "tcp" => Ok(MonitorKind::Tcp),
+        "dns" => Ok(MonitorKind::Dns),
+        "heartbeat" => Ok(MonitorKind::Heartbeat),
+        "flow" => Ok(MonitorKind::Flow),
+        _ => Err(ApiError::bad_request("unsupported pricing monitor kind")),
+    }).transpose()?;
+    if payload.min_quantity.zip(payload.max_quantity).is_some_and(|(min, max)| min > max) {
+        return Err(ApiError::bad_request("min_quantity must be less than or equal to max_quantity"));
+    }
+
     let pricing = state
         .payg_pricing_repository
-        .create_version(
+        .create_version_with_context(
             operation.as_str(),
             payload.unit_name.trim(),
             payload.unit_price,
             "USD",
             payload.effective_from.unwrap_or_else(chrono::Utc::now),
+            &application::ports::PricingRuleContext {
+                account_type,
+                region,
+                monitor_kind,
+                min_quantity: payload.min_quantity,
+                max_quantity: payload.max_quantity,
+            },
         )
         .await
         .map_err(|e| ApiError::internal_with_source("failed to create PAYG price version", e))?;
--- a/crates/api/src/handlers/bulk_monitor_import_confirm.rs
+++ b/crates/api/src/handlers/bulk_monitor_import_confirm.rs
@@ -25,6 +25,16 @@
 const MAX_CSV_BYTES: usize = 1_000_000;
 const DEFAULT_FORECAST_DAYS: u32 = 30;
 const DEFAULT_BATCH_SIZE: i32 = 500;
+const MIN_BATCH_SIZE: i32 = 50;
+const MAX_BATCH_SIZE: i32 = 1000;
+
+fn configured_batch_size() -> i32 {
+    std::env::var("BULK_IMPORT_BATCH_SIZE")
+        .ok()
+        .and_then(|v| v.trim().parse::<i32>().ok())
+        .map(|v| v.clamp(MIN_BATCH_SIZE, MAX_BATCH_SIZE))
+        .unwrap_or(DEFAULT_BATCH_SIZE)
+}
 
 #[derive(Debug, Deserialize)]
 pub struct BulkMonitorImportConfirmQuery {
@@ -218,7 +228,7 @@
             .map_err(|_| ApiError::bad_request("bulk import estimate is too large"))?,
         estimated_charge_units: preview.estimated_payg_cost_units,
         currency: preview.currency.clone(),
-        batch_size: DEFAULT_BATCH_SIZE,
+        batch_size: configured_batch_size(),
         pricing_snapshot: serde_json::to_value(&preview.pricing_versions)
             .map_err(|_| ApiError::internal("failed to serialize pricing snapshot"))?,
         rows: durable_rows,
--- a/crates/api/src/handlers/change_member_role.rs
+++ b/crates/api/src/handlers/change_member_role.rs
@@ -27,6 +27,7 @@
     let use_case = ChangeMemberRoleUseCase::new(auth_state.tenant_membership_repository.clone());
 
     use_case
+        .with_audit_log_repository(auth_state.audit_log_entry_repository.clone())
         .execute(
             ChangeMemberRoleCommand {
                 user_id: UserId(user_id),
--- a/crates/api/src/handlers/create_invitation.rs
+++ b/crates/api/src/handlers/create_invitation.rs
@@ -35,6 +35,7 @@
     );
 
     let created = use_case
+        .with_audit_log_repository(auth_state.audit_log_entry_repository.clone())
         .execute(
             CreateInvitationCommand {
                 email: payload.email,
--- a/crates/api/src/handlers/heartbeat.rs
+++ b/crates/api/src/handlers/heartbeat.rs
@@ -120,6 +120,7 @@
 
                     monitor_id: None,
                     execution_id: None,
+                pricing_context: None,
                 },
                 || {
                     state
--- a/crates/api/src/handlers/remove_member.rs
+++ b/crates/api/src/handlers/remove_member.rs
@@ -21,6 +21,7 @@
     let use_case = RemoveMemberUseCase::new(auth_state.tenant_membership_repository.clone());
 
     use_case
+        .with_audit_log_repository(auth_state.audit_log_entry_repository.clone())
         .execute(UserId(user_id), authenticated_user)
         .await
         .map_err(|err| ApiError::internal_with_source("failed to remove member", err))?;
--- a/crates/api/src/handlers/rum_ingest.rs
+++ b/crates/api/src/handlers/rum_ingest.rs
@@ -321,6 +321,7 @@
 
                     monitor_id: None,
                     execution_id: None,
+                pricing_context: None,
                 },
                 || use_case.execute_for_site(payload, country_code, site.clone()),
             )
--- a/crates/api/src/handlers/wallet.rs
+++ b/crates/api/src/handlers/wallet.rs
@@ -68,6 +68,8 @@
     pub monthly_top_up_limit_units: i64,
     pub daily_spend_limit_units: Option<i64>,
     pub monthly_spend_limit_units: Option<i64>,
+    pub daily_usage_alert_threshold_units: Option<i64>,
+    pub monthly_usage_alert_threshold_units: Option<i64>,
     pub retry_limit: i32,
 }
 #[derive(Debug, Deserialize)]
@@ -317,6 +319,8 @@
         monthly_top_up_limit_units: req.monthly_top_up_limit_units,
         daily_spend_limit_units: req.daily_spend_limit_units,
         monthly_spend_limit_units: req.monthly_spend_limit_units,
+        daily_usage_alert_threshold_units: req.daily_usage_alert_threshold_units,
+        monthly_usage_alert_threshold_units: req.monthly_usage_alert_threshold_units,
         retry_limit: req.retry_limit,
     };
     if s.critical_balance_threshold_units > s.low_balance_threshold_units
@@ -328,9 +332,14 @@
         || s.monthly_top_up_limit_units < s.daily_top_up_limit_units
         || s.daily_spend_limit_units.is_some_and(|v| v <= 0)
         || s.monthly_spend_limit_units.is_some_and(|v| v <= 0)
+        || s.daily_usage_alert_threshold_units.is_some_and(|v| v <= 0)
+        || s.monthly_usage_alert_threshold_units.is_some_and(|v| v <= 0)
         || s.monthly_spend_limit_units
             .zip(s.daily_spend_limit_units)
             .is_some_and(|(m, d)| m < d)
+        || s.monthly_usage_alert_threshold_units
+            .zip(s.daily_usage_alert_threshold_units)
+            .is_some_and(|(m, d)| m < d)
         || !(0..=10).contains(&s.retry_limit)
     {
         return Err(ApiError::bad_request("invalid wallet settings"));
--- a/crates/api/src/main.rs
+++ b/crates/api/src/main.rs
@@ -146,7 +146,8 @@
             std::sync::Arc::new(payg_pricing_repository.clone()),
             std::sync::Arc::new(usage_event_repository.clone()),
         )
-        .with_audit_log_repository(financial_audit_repository.clone()),
+        .with_audit_log_repository(financial_audit_repository.clone())
+        .with_tenant_repository(std::sync::Arc::new(tenant_repository.clone())),
     );
 
     // Shared distributed rate limiter. Constructed once and cloned into
@@ -359,6 +360,7 @@
     )
     .with_password_reset(reset_password_token_repository, password_reset_mailer)
     .with_invitation_repository(invitation_repository)
+    .with_audit_log_repository(infra::PostgresAuditLogEntryRepository::new(pool.clone()))
     .with_oauth(
         google_id_token_verifier,
         apple_id_token_verifier,
--- a/crates/api/src/routes/internal_agents.rs
+++ b/crates/api/src/routes/internal_agents.rs
@@ -261,6 +261,7 @@
                                 // The durable agent check ID is the execution identity
                                 // for this metered agent-region attempt.
                                 execution_id: Some(payload.check_id),
+                            pricing_context: None,
                             },
                             payload.attempted,
                         )
--- a/crates/application/src/ports/mod.rs
+++ b/crates/application/src/ports/mod.rs
@@ -202,7 +202,7 @@
 
 pub mod payg_pricing_repository;
 pub use payg_pricing_repository::{
-    PAYGPricingRepository, PaygPricingRepoResult, PricingContext, resolve_price,
+    PAYGPricingRepository, PaygPricingRepoResult, PricingContext, PricingRuleContext, resolve_price,
 };
 pub mod usage_event_repository;
 pub use usage_event_repository::{UsageDailyPoint, UsageOperationPoint, UsageReport};
--- a/crates/application/src/ports/payg_pricing_repository.rs
+++ b/crates/application/src/ports/payg_pricing_repository.rs
@@ -9,6 +9,19 @@
 #[async_trait]
 pub trait PAYGPricingRepository: Send + Sync {
     async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>>;
+
+    /// Resolves the most specific active price for the trusted server-side context.
+    /// Implementations should prefer exact context matches and fall back to the
+    /// generic operation price when no contextual override exists.
+    async fn find_active_for_context(
+        &self,
+        operation: &str,
+        context: &PricingContext,
+    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
+        let _ = context;
+        self.find_active(operation).await
+    }
+
     async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>>;
 
     /// Appends a new immutable PAYG price version. The repository owns the
@@ -21,6 +34,19 @@
         currency: &str,
         effective_from: DateTime<Utc>,
     ) -> PaygPricingRepoResult<PAYGPricing>;
+
+    async fn create_version_with_context(
+        &self,
+        operation: &str,
+        unit_name: &str,
+        unit_price: i64,
+        currency: &str,
+        effective_from: DateTime<Utc>,
+        context: &PricingRuleContext,
+    ) -> PaygPricingRepoResult<PAYGPricing> {
+        let _ = context;
+        self.create_version(operation, unit_name, unit_price, currency, effective_from).await
+    }
 }
 
 #[derive(Debug, Clone, Default)]
@@ -31,15 +57,24 @@
     pub quantity: u64,
 }
 
+#[derive(Debug, Clone, Default)]
+pub struct PricingRuleContext {
+    pub account_type: Option<domain::AccountType>,
+    pub region: Option<domain::RegionCode>,
+    pub monitor_kind: Option<domain::MonitorKind>,
+    pub min_quantity: Option<u64>,
+    pub max_quantity: Option<u64>,
+}
+
 pub async fn resolve_price(
     repository: &dyn PAYGPricingRepository,
     operation: &str,
-    _context: &PricingContext,
+    context: &PricingContext,
 ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
     if operation.trim().is_empty() {
         return Ok(None);
     }
-    repository.find_active(operation).await
+    repository.find_active_for_context(operation, context).await
 }
 
 #[async_trait]
@@ -50,6 +85,13 @@
     async fn find_active(&self, operation: &str) -> PaygPricingRepoResult<Option<PAYGPricing>> {
         (**self).find_active(operation).await
     }
+    async fn find_active_for_context(
+        &self,
+        operation: &str,
+        context: &PricingContext,
+    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
+        (**self).find_active_for_context(operation, context).await
+    }
     async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
         (**self).list_active().await
     }
@@ -65,4 +107,17 @@
             .create_version(operation, unit_name, unit_price, currency, effective_from)
             .await
     }
+    async fn create_version_with_context(
+        &self,
+        operation: &str,
+        unit_name: &str,
+        unit_price: i64,
+        currency: &str,
+        effective_from: DateTime<Utc>,
+        context: &PricingRuleContext,
+    ) -> PaygPricingRepoResult<PAYGPricing> {
+        (**self)
+            .create_version_with_context(operation, unit_name, unit_price, currency, effective_from, context)
+            .await
+    }
 }
--- a/crates/application/src/ports/wallet_repository.rs
+++ b/crates/application/src/ports/wallet_repository.rs
@@ -174,6 +174,14 @@
         let _ = id;
         Ok(false)
     }
+
+    /// Creates at most one daily and one monthly usage-threshold notification
+    /// per tenant/period. Production implementations must claim/insert using
+    /// database uniqueness so multiple workers cannot duplicate alerts.
+    async fn record_usage_threshold_notifications(&self, limit: u32) -> WalletRepoResult<u64> {
+        let _ = limit;
+        Ok(0)
+    }
 }
 
 #[async_trait]
--- a/crates/application/src/use_cases/add_member.rs
+++ b/crates/application/src/use_cases/add_member.rs
@@ -9,7 +9,7 @@
     },
 };
 
-use crate::ports::{TenantMembershipRepository, UserRepository};
+use crate::ports::{AuditLogEntryRepository, TenantMembershipRepository, UserRepository};
 
 use super::membership_common::MembershipError;
 
@@ -28,6 +28,7 @@
 pub struct AddMemberUseCase {
     tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
     user_repository: Arc<dyn UserRepository>,
+    audit_log_repository: Option<Arc<dyn AuditLogEntryRepository>>,
 }
 
 impl AddMemberUseCase {
@@ -38,9 +39,15 @@
         Self {
             tenant_membership_repository,
             user_repository,
+            audit_log_repository: None,
         }
     }
 
+    pub fn with_audit_log_repository(mut self, repository: Arc<dyn AuditLogEntryRepository>) -> Self {
+        self.audit_log_repository = Some(repository);
+        self
+    }
+
     pub async fn execute(
         &self,
         command: AddMemberCommand,
@@ -100,6 +107,22 @@
             .await
             .map_err(MembershipError::Other)?;
 
+        if let Some(audit) = &self.audit_log_repository {
+            let entry = domain::AuditLogEntry {
+                id: domain::AuditLogEntryId::new(),
+                actor_user_id: actor.user_id(),
+                tenant_id: Some(tenant_id),
+                action: domain::AuditAction::MemberInvited,
+                target_id: command.user_id.0.to_string(),
+                metadata: serde_json::json!({
+                    "role": command.role,
+                    "mode": "direct_add",
+                }),
+                created_at: now,
+            };
+            audit.create(&entry).await.map_err(MembershipError::Other)?;
+        }
+
         Ok(membership)
     }
 }
--- a/crates/application/src/use_cases/change_member_role.rs
+++ b/crates/application/src/use_cases/change_member_role.rs
@@ -5,7 +5,7 @@
     types::{AuthenticatedUser, Permission, Role},
 };
 
-use crate::ports::{LastOwnerGuardOutcome, TenantMembershipRepository};
+use crate::ports::{AuditLogEntryRepository, LastOwnerGuardOutcome, TenantMembershipRepository};
 
 use super::membership_common::MembershipError;
 
@@ -26,15 +26,22 @@
 #[derive(Clone)]
 pub struct ChangeMemberRoleUseCase {
     tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
+    audit_log_repository: Option<Arc<dyn AuditLogEntryRepository>>,
 }
 
 impl ChangeMemberRoleUseCase {
     pub fn new(tenant_membership_repository: Arc<dyn TenantMembershipRepository>) -> Self {
         Self {
             tenant_membership_repository,
+            audit_log_repository: None,
         }
     }
 
+    pub fn with_audit_log_repository(mut self, repository: Arc<dyn AuditLogEntryRepository>) -> Self {
+        self.audit_log_repository = Some(repository);
+        self
+    }
+
     pub async fn execute(
         &self,
         command: ChangeMemberRoleCommand,
@@ -82,7 +89,23 @@
             .await
             .map_err(MembershipError::Other)?
         {
-            LastOwnerGuardOutcome::Applied => Ok(()),
+            LastOwnerGuardOutcome::Applied => {
+                if let Some(audit) = &self.audit_log_repository {
+                    let entry = domain::AuditLogEntry {
+                        id: domain::AuditLogEntryId::new(),
+                        actor_user_id: actor.user_id(),
+                        tenant_id: Some(tenant_id),
+                        action: domain::AuditAction::RoleChanged,
+                        target_id: command.user_id.0.to_string(),
+                        metadata: serde_json::json!({
+                            "new_role": command.new_role,
+                        }),
+                        created_at: chrono::Utc::now(),
+                    };
+                    audit.create(&entry).await.map_err(MembershipError::Other)?;
+                }
+                Ok(())
+            },
             LastOwnerGuardOutcome::NotFound => Err(MembershipError::NotFound),
             LastOwnerGuardOutcome::WouldRemoveLastOwner => Err(MembershipError::InvalidOperation(
                 "cannot demote the last Owner".to_string(),
--- a/crates/application/src/use_cases/charge_for_usage.rs
+++ b/crates/application/src/use_cases/charge_for_usage.rs
@@ -17,6 +17,8 @@
     pub monitor_id: Option<Uuid>,
     /// Server-side durable execution identity.
     pub execution_id: Option<Uuid>,
+    /// Trusted server-side pricing dimensions. Public callers must not provide prices.
+    pub pricing_context: Option<PricingContext>,
 }
 
 #[derive(Debug, Clone, PartialEq, Eq)]
@@ -179,7 +181,7 @@
             &input.operation,
             &PricingContext {
                 quantity: input.units.max(0) as u64,
-                ..PricingContext::default()
+                ..input.pricing_context.clone().unwrap_or_default()
             },
         )
         .await
@@ -304,7 +306,7 @@
             &input.operation,
             &PricingContext {
                 quantity: input.units.max(0) as u64,
-                ..PricingContext::default()
+                ..input.pricing_context.clone().unwrap_or_default()
             },
         )
         .await
@@ -470,7 +472,7 @@
             &input.operation,
             &PricingContext {
                 quantity: input.units.max(0) as u64,
-                ..PricingContext::default()
+                ..input.pricing_context.clone().unwrap_or_default()
             },
         )
         .await
@@ -638,7 +640,7 @@
             &input.operation,
             &PricingContext {
                 quantity: input.units.max(0) as u64,
-                ..PricingContext::default()
+                ..input.pricing_context.clone().unwrap_or_default()
             },
         )
         .await
--- a/crates/application/src/use_cases/create_invitation.rs
+++ b/crates/application/src/use_cases/create_invitation.rs
@@ -4,7 +4,7 @@
     shared::types::{AuthenticatedUser, Permission, Role},
 };
 
-use crate::ports::{IdGenerator, PasswordHasher, TenantInvitationRepository};
+use crate::ports::{AuditLogEntryRepository, IdGenerator, PasswordHasher, TenantInvitationRepository};
 use crate::use_cases::{
     invitation_common::InvitationError,
     invitation_support::{INVITATION_TOKEN_TTL_HOURS, invitation_token_prefix},
@@ -37,6 +37,7 @@
     id_generator: G,
     password_hasher: H,
     invitation_repository: R,
+    audit_log_repository: Option<std::sync::Arc<dyn AuditLogEntryRepository>>,
 }
 
 impl<G, H, R> CreateInvitationUseCase<G, H, R>
@@ -50,9 +51,15 @@
             id_generator,
             password_hasher,
             invitation_repository,
+            audit_log_repository: None,
         }
     }
 
+    pub fn with_audit_log_repository(mut self, repository: std::sync::Arc<dyn AuditLogEntryRepository>) -> Self {
+        self.audit_log_repository = Some(repository);
+        self
+    }
+
     pub async fn execute(
         &self,
         command: CreateInvitationCommand,
@@ -107,6 +114,21 @@
             .await
             .map_err(InvitationError::Internal)?;
 
+        if let Some(audit) = &self.audit_log_repository {
+            let entry = domain::AuditLogEntry {
+                id: domain::AuditLogEntryId::new(),
+                actor_user_id: actor.user_id(),
+                tenant_id: Some(invitation.tenant_id),
+                action: domain::AuditAction::MemberInvited,
+                target_id: invitation.id.into_uuid().to_string(),
+                metadata: serde_json::json!({
+                    "role": invitation.role,
+                }),
+                created_at: now,
+            };
+            audit.create(&entry).await.map_err(InvitationError::Internal)?;
+        }
+
         Ok(CreatedTenantInvitation {
             invitation,
             raw_token,
--- a/crates/application/src/use_cases/meter_usage.rs
+++ b/crates/application/src/use_cases/meter_usage.rs
@@ -2,7 +2,8 @@
     ChargeForUsageError, ChargeForUsageInput, ChargeForUsageResult, ChargeForUsageUseCase,
     MeteredAttempt,
 };
-use crate::ports::{PAYGPricingRepository, UsageEventRepository, WalletRepository};
+use crate::ports::{PAYGPricingRepository, PricingContext, TenantRepository, UsageEventRepository, WalletRepository};
+use std::collections::HashMap;
 use std::fmt;
 use std::future::Future;
 use std::sync::Arc;
@@ -18,10 +19,14 @@
     pub monitor_id: Option<Uuid>,
     /// Trusted durable execution context.
     pub execution_id: Option<Uuid>,
+    /// Trusted runtime pricing dimensions; public API inputs must leave this None.
+    pub pricing_context: Option<crate::ports::PricingContext>,
 }
 
 pub struct MeterUsageUseCase {
     charge: ChargeForUsageUseCase,
+    tenant_repository: Option<Arc<dyn TenantRepository>>,
+    account_type_cache: Arc<std::sync::RwLock<HashMap<Uuid, domain::AccountType>>>,
 }
 impl MeterUsageUseCase {
     pub fn new(
@@ -35,9 +40,44 @@
                 pricing_repository,
                 usage_event_repository,
             ),
+            tenant_repository: None,
+            account_type_cache: Arc::new(std::sync::RwLock::new(HashMap::new())),
         }
     }
 
+    pub fn with_tenant_repository(mut self, repository: Arc<dyn TenantRepository>) -> Self {
+        self.tenant_repository = Some(repository);
+        self
+    }
+
+    async fn enrich_pricing_context(
+        &self,
+        tenant_id: Uuid,
+        context: Option<PricingContext>,
+        quantity: u64,
+    ) -> Option<PricingContext> {
+        let mut context = context.unwrap_or_default();
+        context.quantity = quantity;
+        if context.account_type.is_none() {
+            if let Some(account_type) = self
+                .account_type_cache
+                .read()
+                .ok()
+                .and_then(|cache| cache.get(&tenant_id).copied())
+            {
+                context.account_type = Some(account_type);
+            } else if let Some(repository) = &self.tenant_repository {
+                if let Ok(Some(tenant)) = repository.find_by_id(domain::TenantId(tenant_id)).await {
+                    context.account_type = Some(tenant.account_type);
+                    if let Ok(mut cache) = self.account_type_cache.write() {
+                        cache.insert(tenant_id, tenant.account_type);
+                    }
+                }
+            }
+        }
+        Some(context)
+    }
+
     pub fn with_audit_log_repository(
         mut self,
         repo: Arc<dyn crate::ports::FinancialAuditLogRepository>,
@@ -59,6 +99,7 @@
         Fut: Future<Output = Result<(), E>>,
         E: fmt::Display,
     {
+        let pricing_context = self.enrich_pricing_context(input.tenant_id, input.pricing_context.clone(), input.units.max(0) as u64).await;
         self.charge
             .execute(
                 ChargeForUsageInput {
@@ -68,6 +109,7 @@
                     request_id: input.request_id,
                     monitor_id: input.monitor_id,
                     execution_id: input.execution_id,
+                    pricing_context: pricing_context.clone(),
                 },
                 operation,
             )
@@ -93,6 +135,7 @@
         Fut: Future<Output = Result<(), E>>,
         E: fmt::Display,
     {
+        let pricing_context = self.enrich_pricing_context(input.tenant_id, input.pricing_context.clone(), input.units.max(0) as u64).await;
         self.charge
             .execute_attempt(
                 ChargeForUsageInput {
@@ -102,6 +145,7 @@
                     request_id: input.request_id,
                     monitor_id: input.monitor_id,
                     execution_id: input.execution_id,
+                    pricing_context: pricing_context.clone(),
                 },
                 operation,
             )
@@ -119,6 +163,7 @@
         Fut: Future<Output = Result<MeteredAttempt, E>>,
         E: fmt::Display,
     {
+        let pricing_context = self.enrich_pricing_context(input.tenant_id, input.pricing_context.clone(), input.units.max(0) as u64).await;
         self.charge
             .execute_classified_attempt(
                 ChargeForUsageInput {
@@ -128,6 +173,7 @@
                     request_id: input.request_id,
                     monitor_id: input.monitor_id,
                     execution_id: input.execution_id,
+                    pricing_context: pricing_context.clone(),
                 },
                 operation,
             )
@@ -144,6 +190,7 @@
         Fut: Future<Output = Result<(MeteredAttempt, T), E>>,
         E: fmt::Display,
     {
+        let pricing_context = self.enrich_pricing_context(input.tenant_id, input.pricing_context.clone(), input.units.max(0) as u64).await;
         self.charge
             .execute_classified_attempt_with_output(
                 ChargeForUsageInput {
@@ -153,6 +200,7 @@
                     request_id: input.request_id,
                     monitor_id: input.monitor_id,
                     execution_id: input.execution_id,
+                    pricing_context: pricing_context.clone(),
                 },
                 operation,
             )
@@ -166,6 +214,7 @@
         input: MeterUsageInput,
         attempted: bool,
     ) -> Result<ChargeForUsageResult, ChargeForUsageError> {
+        let pricing_context = self.enrich_pricing_context(input.tenant_id, input.pricing_context.clone(), input.units.max(0) as u64).await;
         self.charge
             .finalize_reserved_attempt(
                 ChargeForUsageInput {
@@ -175,6 +224,7 @@
                     request_id: input.request_id,
                     monitor_id: input.monitor_id,
                     execution_id: input.execution_id,
+                    pricing_context: pricing_context.clone(),
                 },
                 attempted,
             )
@@ -193,6 +243,7 @@
         Fut: Future<Output = Result<(), E>>,
         E: fmt::Display,
     {
+        let pricing_context = self.enrich_pricing_context(input.tenant_id, input.pricing_context.clone(), input.units.max(0) as u64).await;
         self.charge
             .execute_recoverable(
                 ChargeForUsageInput {
@@ -202,6 +253,7 @@
                     request_id: input.request_id,
                     monitor_id: input.monitor_id,
                     execution_id: input.execution_id,
+                    pricing_context: pricing_context.clone(),
                 },
                 operation,
             )
--- a/crates/application/src/use_cases/preview_bulk_monitor_import.rs
+++ b/crates/application/src/use_cases/preview_bulk_monitor_import.rs
@@ -8,7 +8,7 @@
     },
     ports::{
         BulkImportExistingMonitor, BulkImportMonitorSnapshot, MonitorIntervalPolicyRepository,
-        PAYGPricingRepository,
+        PAYGPricingRepository, PricingContext, resolve_price,
     },
 };
 use domain::{
@@ -98,14 +98,13 @@
             .list_active()
             .await
             .map_err(BulkMonitorImportPreviewError::Repository)?;
-
-        let mut pricing_by_operation = HashMap::new();
+        let mut generic_pricing_by_operation = HashMap::new();
         for pricing in active_pricing {
-            if pricing.unit_price <= 0 {
-                continue;
+            if pricing.unit_price > 0 {
+                generic_pricing_by_operation.insert(pricing.operation.clone(), pricing);
             }
-            pricing_by_operation.insert(pricing.operation.clone(), pricing);
         }
+        let mut contextual_pricing_cache: HashMap<(String, MonitorKind, Option<String>), Option<PAYGPricing>> = HashMap::new();
 
         let existing_signatures = snapshot
             .monitors
@@ -175,7 +174,30 @@
             let operation = domain::monitor_operation(row.kind, false)
                 .as_str()
                 .to_owned();
-            let pricing = pricing_by_operation.get(&operation);
+            let region_key = row.regions.first().map(ToString::to_string);
+            let cache_key = (operation.clone(), row.kind, region_key.clone());
+            let pricing = if let Some(cached) = contextual_pricing_cache.get(&cache_key) {
+                cached.as_ref()
+            } else {
+                let resolved = resolve_price(
+                    self.pricing_repository.as_ref(),
+                    &operation,
+                    &PricingContext {
+                        account_type: None,
+                        region: row.regions.first().cloned(),
+                        monitor_kind: Some(row.kind),
+                        quantity: 1,
+                    },
+                )
+                .await
+                .map_err(BulkMonitorImportPreviewError::Repository)?;
+                let resolved = resolved.filter(|p| p.unit_price > 0);
+                contextual_pricing_cache.insert(cache_key, resolved);
+                contextual_pricing_cache
+                    .get(&(operation.clone(), row.kind, region_key.clone()))
+                    .and_then(|p| p.as_ref())
+            };
+            let pricing = pricing.or_else(|| generic_pricing_by_operation.get(&operation));
             if pricing.is_none() {
                 row_errors.push(format!(
                     "PAYG pricing unavailable for operation {operation}"
--- a/crates/application/src/use_cases/remove_member.rs
+++ b/crates/application/src/use_cases/remove_member.rs
@@ -5,7 +5,7 @@
     types::{AuthenticatedUser, Permission, Role},
 };
 
-use crate::ports::{LastOwnerGuardOutcome, TenantMembershipRepository};
+use crate::ports::{AuditLogEntryRepository, LastOwnerGuardOutcome, TenantMembershipRepository};
 
 use super::membership_common::MembershipError;
 
@@ -20,15 +20,22 @@
 #[derive(Clone)]
 pub struct RemoveMemberUseCase {
     tenant_membership_repository: Arc<dyn TenantMembershipRepository>,
+    audit_log_repository: Option<Arc<dyn AuditLogEntryRepository>>,
 }
 
 impl RemoveMemberUseCase {
     pub fn new(tenant_membership_repository: Arc<dyn TenantMembershipRepository>) -> Self {
         Self {
             tenant_membership_repository,
+            audit_log_repository: None,
         }
     }
 
+    pub fn with_audit_log_repository(mut self, repository: Arc<dyn AuditLogEntryRepository>) -> Self {
+        self.audit_log_repository = Some(repository);
+        self
+    }
+
     pub async fn execute(
         &self,
         target_user_id: UserId,
@@ -77,7 +84,21 @@
             .await
             .map_err(MembershipError::Other)?
         {
-            LastOwnerGuardOutcome::Applied => Ok(()),
+            LastOwnerGuardOutcome::Applied => {
+                if let Some(audit) = &self.audit_log_repository {
+                    let entry = domain::AuditLogEntry {
+                        id: domain::AuditLogEntryId::new(),
+                        actor_user_id: actor.user_id(),
+                        tenant_id: Some(tenant_id),
+                        action: domain::AuditAction::MemberRemoved,
+                        target_id: target_user_id.0.to_string(),
+                        metadata: serde_json::json!({}),
+                        created_at: chrono::Utc::now(),
+                    };
+                    audit.create(&entry).await.map_err(MembershipError::Other)?;
+                }
+                Ok(())
+            },
             LastOwnerGuardOutcome::NotFound => Err(MembershipError::NotFound),
             LastOwnerGuardOutcome::WouldRemoveLastOwner => Err(MembershipError::InvalidOperation(
                 "cannot remove the last Owner".to_string(),
--- a/crates/application/src/use_cases/upload_status_page_logo.rs
+++ b/crates/application/src/use_cases/upload_status_page_logo.rs
@@ -142,6 +142,7 @@
 
                         monitor_id: None,
                         execution_id: None,
+                    pricing_context: None,
                     },
                     move || async move {
                         let mut page = status_page_for_upload.take().ok_or_else(|| {
--- a/crates/domain/src/entities/audit_log_entry.rs
+++ b/crates/domain/src/entities/audit_log_entry.rs
@@ -7,9 +7,8 @@
 /// Rikodin audit log, append-only ta hanyar design.
 ///
 /// Wannan struct BABU `mut` setters ko `update`/`delete` method
-/// KO'INA — babu DB-level REVOKE don haka domain/application layer
-/// SHI NE layin farko na kariya akan gyara/goge rikodin audit bayan
-/// an kirkiresu.
+/// KO'INA. Application persistence ma ba ta da mutation port, kuma
+/// PostgreSQL migration ta hana UPDATE/DELETE a database level.
 #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
 pub struct AuditLogEntry {
     pub id: AuditLogEntryId,
--- a/crates/domain/src/entities/wallet.rs
+++ b/crates/domain/src/entities/wallet.rs
@@ -113,6 +113,9 @@
     /// Optional PAYG debit protection. NULL/unlimited is represented as None.
     pub daily_spend_limit_units: Option<i64>,
     pub monthly_spend_limit_units: Option<i64>,
+    /// Optional usage thresholds that trigger one notification per period.
+    pub daily_usage_alert_threshold_units: Option<i64>,
+    pub monthly_usage_alert_threshold_units: Option<i64>,
     pub retry_limit: i32,
 }
 
@@ -130,6 +133,8 @@
             monthly_top_up_limit_units: 500_000_000,
             daily_spend_limit_units: None,
             monthly_spend_limit_units: None,
+            daily_usage_alert_threshold_units: None,
+            monthly_usage_alert_threshold_units: None,
             retry_limit: 3,
         }
     }
--- a/crates/domain/src/enums/audit_action.rs
+++ b/crates/domain/src/enums/audit_action.rs
@@ -24,6 +24,7 @@
     CustomDomainRequested,
     CustomDomainRemoved,
     MemberInvited,
+    MemberRemoved,
     RoleChanged,
     ApiKeyCreated,
     ApiKeyDeleted,
--- a/crates/infra/src/repositories/in_memory_payg_pricing_repository.rs
+++ b/crates/infra/src/repositories/in_memory_payg_pricing_repository.rs
@@ -1,4 +1,4 @@
-use application::ports::{PAYGPricingRepository, PaygPricingRepoResult};
+use application::ports::{PAYGPricingRepository, PaygPricingRepoResult, PricingContext, PricingRuleContext};
 use async_trait::async_trait;
 use chrono::{DateTime, Utc};
 use domain::{PAYGPricing, PaygOperation};
@@ -33,6 +33,26 @@
             .cloned();
         Ok(row)
     }
+    async fn find_active_for_context(
+        &self,
+        operation: &str,
+        _context: &PricingContext,
+    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
+        // The in-memory adapter has no contextual columns; preserve the same
+        // safe generic fallback used by production when no override exists.
+        self.find_active(operation).await
+    }
+    async fn create_version_with_context(
+        &self,
+        operation: &str,
+        unit_name: &str,
+        unit_price: i64,
+        currency: &str,
+        effective_from: DateTime<Utc>,
+        _context: &PricingRuleContext,
+    ) -> PaygPricingRepoResult<PAYGPricing> {
+        self.create_version(operation, unit_name, unit_price, currency, effective_from).await
+    }
     async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
         let now = chrono::Utc::now();
         let rows = self.rows.read().unwrap();
--- a/crates/infra/src/repositories/postgres_payg_pricing_repository.rs
+++ b/crates/infra/src/repositories/postgres_payg_pricing_repository.rs
@@ -1,4 +1,4 @@
-use application::ports::{PAYGPricingRepository, PaygPricingRepoResult};
+use application::ports::{PAYGPricingRepository, PaygPricingRepoResult, PricingContext, PricingRuleContext};
 use async_trait::async_trait;
 use chrono::{DateTime, Utc};
 use domain::PAYGPricing;
@@ -49,12 +49,50 @@
              FROM payg_pricing_configs
              WHERE operation=$1 AND active=TRUE AND effective_from<=now()
                AND (effective_until IS NULL OR effective_until>now())
+               AND account_type IS NULL AND region IS NULL AND monitor_kind IS NULL
+               AND min_quantity IS NULL AND max_quantity IS NULL
              ORDER BY version DESC LIMIT 1",
         ).bind(operation).fetch_optional(&self.pool).await?;
         Ok(row.map(Row::into_domain))
     }
+    async fn find_active_for_context(
+        &self,
+        operation: &str,
+        context: &PricingContext,
+    ) -> PaygPricingRepoResult<Option<PAYGPricing>> {
+        let account_type = context.account_type.map(|v| v.as_str());
+        let region = context.region.as_ref().map(|v| v.as_str());
+        let monitor_kind = context.monitor_kind.map(|v| format!("{:?}", v).to_ascii_lowercase());
+        let quantity = i64::try_from(context.quantity).map_err(|_| "pricing quantity exceeds supported range")?;
+        let row = sqlx::query_as::<_, Row>(
+            "SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active
+             FROM payg_pricing_configs
+             WHERE operation=$1 AND active=TRUE AND effective_from<=now()
+               AND (effective_until IS NULL OR effective_until>now())
+               AND (account_type IS NULL OR account_type=$2)
+               AND (region IS NULL OR region=$3)
+               AND (monitor_kind IS NULL OR monitor_kind=$4)
+               AND (min_quantity IS NULL OR min_quantity <= $5)
+               AND (max_quantity IS NULL OR max_quantity >= $5)
+             ORDER BY
+               ((account_type IS NOT NULL)::int + (region IS NOT NULL)::int + (monitor_kind IS NOT NULL)::int
+                + (min_quantity IS NOT NULL)::int + (max_quantity IS NOT NULL)::int) DESC,
+               version DESC
+             LIMIT 1",
+        )
+        .bind(operation)
+        .bind(account_type)
+        .bind(region)
+        .bind(monitor_kind.as_deref())
+        .bind(quantity)
+        .fetch_optional(&self.pool)
+        .await?;
+        Ok(row.map(Row::into_domain))
+    }
+
     async fn list_active(&self) -> PaygPricingRepoResult<Vec<PAYGPricing>> {
         let rows=sqlx::query_as::<_,Row>("SELECT id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active FROM payg_pricing_configs WHERE active=TRUE AND effective_from<=now() AND (effective_until IS NULL OR effective_until>now())
+               AND account_type IS NULL AND region IS NULL AND monitor_kind IS NULL AND min_quantity IS NULL AND max_quantity IS NULL
                AND operation IN ('http_check.one_time','tcp_check.one_time','dns_check.one_time','api_flow_check.one_time','proxy_check.one_time','http_check.monitoring','tcp_check.monitoring','dns_check.monitoring','heartbeat_processing','api_flow_check.monitoring','proxy_check.monitoring','rum_event','sms','storage','network_egress')
                AND version=(SELECT MAX(p2.version) FROM payg_pricing_configs p2 WHERE p2.operation=payg_pricing_configs.operation AND p2.active=TRUE AND p2.effective_from<=now() AND (p2.effective_until IS NULL OR p2.effective_until>now())) ORDER BY operation").fetch_all(&self.pool).await?;
         Ok(rows.into_iter().map(Row::into_domain).collect())
@@ -67,40 +105,53 @@
         currency: &str,
         effective_from: DateTime<Utc>,
     ) -> PaygPricingRepoResult<PAYGPricing> {
+        self.create_version_with_context(
+            operation, unit_name, unit_price, currency, effective_from, &PricingContext::default()
+        ).await
+    }
+
+    async fn create_version_with_context(
+        &self,
+        operation: &str,
+        unit_name: &str,
+        unit_price: i64,
+        currency: &str,
+        effective_from: DateTime<Utc>,
+        context: &PricingRuleContext,
+    ) -> PaygPricingRepoResult<PAYGPricing> {
         if !domain::PaygOperation::is_canonical(operation) {
             return Err("unsupported PAYG operation".into());
         }
         if unit_price <= 0 || currency != "USD" {
             return Err("invalid PAYG price".into());
         }
+        if context.min_quantity.zip(context.max_quantity).is_some_and(|(min, max)| min > max)
+            || context.min_quantity.is_some_and(|v| v > i64::MAX as u64)
+            || context.max_quantity.is_some_and(|v| v > i64::MAX as u64)
+        {
+            return Err("invalid pricing quantity range".into());
+        }
         let mut tx = self.pool.begin().await?;
-        // Serialize version allocation without mutating historical rows.
         sqlx::query("SELECT pg_advisory_xact_lock(hashtext('payg-pricing-version')::bigint)")
             .execute(&mut *tx)
             .await?;
-
-        let version: i64 =
-            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) + 1 FROM payg_pricing_configs")
-                .fetch_one(&mut *tx)
-                .await?;
-
+        let version: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) + 1 FROM payg_pricing_configs")
+            .fetch_one(&mut *tx).await?;
         let id = Uuid::new_v4();
+        let account_type = context.account_type.map(|v| v.as_str());
+        let region = context.region.as_ref().map(|v| v.as_str());
+        let monitor_kind = context.monitor_kind.map(|v| format!("{:?}", v).to_ascii_lowercase());
         let row = sqlx::query_as::<_, Row>(
             r#"INSERT INTO payg_pricing_configs
-               (id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active)
-               VALUES($1,$2,$3,$4,$5,$6,$7,NULL,TRUE)
-               RETURNING id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active"#,
+               (id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active,account_type,region,monitor_kind,min_quantity,max_quantity)
+               VALUES($1,$2,$3,$4,$5,$6,$7,NULL,TRUE,$8,$9,$10,$11,$12)
+               RETURNING id,version,operation,unit_name,unit_price,currency,effective_from,effective_until,active"#
         )
-        .bind(id)
-        .bind(version)
-        .bind(operation)
-        .bind(unit_name)
-        .bind(unit_price)
-        .bind(currency)
-        .bind(effective_from)
-        .fetch_one(&mut *tx)
-        .await?;
-
+        .bind(id).bind(version).bind(operation).bind(unit_name).bind(unit_price).bind(currency)
+        .bind(effective_from).bind(account_type).bind(region).bind(monitor_kind)
+        .bind(context.min_quantity.map(|v| v as i64))
+        .bind(context.max_quantity.map(|v| v as i64))
+        .fetch_one(&mut *tx).await?;
         tx.commit().await?;
         Ok(row.into_domain())
     }
--- a/crates/infra/src/repositories/postgres_wallet_repository.rs
+++ b/crates/infra/src/repositories/postgres_wallet_repository.rs
@@ -810,9 +810,11 @@
             monthly_top_up_limit_units: i64,
             daily_spend_limit_units: Option<i64>,
             monthly_spend_limit_units: Option<i64>,
+            daily_usage_alert_threshold_units: Option<i64>,
+            monthly_usage_alert_threshold_units: Option<i64>,
             retry_limit: i32,
         }
-        let r = sqlx::query_as::<_, SettingsRow>("SELECT low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,daily_spend_limit_units,monthly_spend_limit_units,retry_limit FROM wallet_settings WHERE tenant_id=$1").bind(tenant_id).fetch_optional(&self.pool).await?;
+        let r = sqlx::query_as::<_, SettingsRow>("SELECT low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,daily_spend_limit_units,monthly_spend_limit_units,daily_usage_alert_threshold_units,monthly_usage_alert_threshold_units,retry_limit FROM wallet_settings WHERE tenant_id=$1").bind(tenant_id).fetch_optional(&self.pool).await?;
         Ok(r.map(|x| WalletSettings {
             tenant_id,
             low_balance_threshold_units: x.low_balance_threshold_units,
@@ -825,6 +827,8 @@
             monthly_top_up_limit_units: x.monthly_top_up_limit_units,
             daily_spend_limit_units: x.daily_spend_limit_units,
             monthly_spend_limit_units: x.monthly_spend_limit_units,
+            daily_usage_alert_threshold_units: x.daily_usage_alert_threshold_units,
+            monthly_usage_alert_threshold_units: x.monthly_usage_alert_threshold_units,
             retry_limit: x.retry_limit,
         })
         .unwrap_or_else(|| WalletSettings {
@@ -840,13 +844,17 @@
             || settings.daily_top_up_limit_units <= 0
             || settings.monthly_top_up_limit_units <= 0
             || settings.retry_limit < 0
+            || settings.daily_usage_alert_threshold_units.is_some_and(|v| v <= 0)
+            || settings.monthly_usage_alert_threshold_units.is_some_and(|v| v <= 0)
             || settings.critical_balance_threshold_units > settings.low_balance_threshold_units
         {
             return Err("invalid wallet settings".into());
         }
-        sqlx::query("INSERT INTO wallet_settings (tenant_id,low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,daily_spend_limit_units,monthly_spend_limit_units,retry_limit) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(tenant_id) DO UPDATE SET low_balance_threshold_units=EXCLUDED.low_balance_threshold_units,critical_balance_threshold_units=EXCLUDED.critical_balance_threshold_units,auto_top_up_enabled=EXCLUDED.auto_top_up_enabled,auto_top_up_threshold_units=EXCLUDED.auto_top_up_threshold_units,auto_top_up_amount_units=EXCLUDED.auto_top_up_amount_units,payment_method=EXCLUDED.payment_method,daily_top_up_limit_units=EXCLUDED.daily_top_up_limit_units,monthly_top_up_limit_units=EXCLUDED.monthly_top_up_limit_units,daily_spend_limit_units=EXCLUDED.daily_spend_limit_units,monthly_spend_limit_units=EXCLUDED.monthly_spend_limit_units,retry_limit=EXCLUDED.retry_limit,updated_at=now()").bind(settings.tenant_id).bind(settings.low_balance_threshold_units).bind(settings.critical_balance_threshold_units).bind(settings.auto_top_up_enabled).bind(settings.auto_top_up_threshold_units).bind(settings.auto_top_up_amount_units).bind(&settings.payment_method).bind(settings.daily_top_up_limit_units).bind(settings.monthly_top_up_limit_units)
+        sqlx::query("INSERT INTO wallet_settings (tenant_id,low_balance_threshold_units,critical_balance_threshold_units,auto_top_up_enabled,auto_top_up_threshold_units,auto_top_up_amount_units,payment_method,daily_top_up_limit_units,monthly_top_up_limit_units,daily_spend_limit_units,monthly_spend_limit_units,daily_usage_alert_threshold_units,monthly_usage_alert_threshold_units,retry_limit) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT(tenant_id) DO UPDATE SET low_balance_threshold_units=EXCLUDED.low_balance_threshold_units,critical_balance_threshold_units=EXCLUDED.critical_balance_threshold_units,auto_top_up_enabled=EXCLUDED.auto_top_up_enabled,auto_top_up_threshold_units=EXCLUDED.auto_top_up_threshold_units,auto_top_up_amount_units=EXCLUDED.auto_top_up_amount_units,payment_method=EXCLUDED.payment_method,daily_top_up_limit_units=EXCLUDED.daily_top_up_limit_units,monthly_top_up_limit_units=EXCLUDED.monthly_top_up_limit_units,daily_spend_limit_units=EXCLUDED.daily_spend_limit_units,monthly_spend_limit_units=EXCLUDED.monthly_spend_limit_units,daily_usage_alert_threshold_units=EXCLUDED.daily_usage_alert_threshold_units,monthly_usage_alert_threshold_units=EXCLUDED.monthly_usage_alert_threshold_units,retry_limit=EXCLUDED.retry_limit,updated_at=now()").bind(settings.tenant_id).bind(settings.low_balance_threshold_units).bind(settings.critical_balance_threshold_units).bind(settings.auto_top_up_enabled).bind(settings.auto_top_up_threshold_units).bind(settings.auto_top_up_amount_units).bind(&settings.payment_method).bind(settings.daily_top_up_limit_units).bind(settings.monthly_top_up_limit_units)
             .bind(settings.daily_spend_limit_units)
             .bind(settings.monthly_spend_limit_units)
+            .bind(settings.daily_usage_alert_threshold_units)
+            .bind(settings.monthly_usage_alert_threshold_units)
             .bind(settings.retry_limit).execute(&self.pool).await?;
         Ok(settings.clone())
     }
@@ -1069,6 +1077,8 @@
             monthly_limit: i64,
             daily_spend_limit: Option<i64>,
             monthly_spend_limit: Option<i64>,
+            daily_usage_alert_threshold: Option<i64>,
+            monthly_usage_alert_threshold: Option<i64>,
             retry_limit: i32,
             daily: i64,
             monthly: i64,
@@ -1086,6 +1096,8 @@
                     s.monthly_top_up_limit_units monthly_limit,
                     s.daily_spend_limit_units daily_spend_limit,
                     s.monthly_spend_limit_units monthly_spend_limit,
+                    s.daily_usage_alert_threshold_units daily_usage_alert_threshold,
+                    s.monthly_usage_alert_threshold_units monthly_usage_alert_threshold,
                     s.retry_limit,
                     COALESCE((SELECT SUM(amount_units) FROM wallet_auto_top_up_attempts a
                               WHERE a.tenant_id=w.tenant_id AND a.status='SUCCEEDED'
@@ -1122,6 +1134,8 @@
                     monthly_top_up_limit_units: r.monthly_limit,
                     daily_spend_limit_units: r.daily_spend_limit,
                     monthly_spend_limit_units: r.monthly_spend_limit,
+                    daily_usage_alert_threshold_units: r.daily_usage_alert_threshold,
+                    monthly_usage_alert_threshold_units: r.monthly_usage_alert_threshold,
                     retry_limit: r.retry_limit,
                 },
                 daily_spend_units: r.daily,
@@ -1595,7 +1609,7 @@
                 SELECT id FROM wallet_notifications
                 WHERE email_sent_at IS NULL
                   AND (email_processing_at IS NULL OR email_processing_at < now() - interval '10 minutes')
-                  AND event_type IN ('WARNING','CRITICAL','BLOCKED','TOP_UP_SUCCESS','REFUND_SUCCESS')
+                  AND event_type IN ('WARNING','CRITICAL','BLOCKED','TOP_UP_SUCCESS','REFUND_SUCCESS','DAILY_USAGE','MONTHLY_USAGE')
                 ORDER BY created_at ASC
                 FOR UPDATE SKIP LOCKED
                 LIMIT $1
@@ -1628,4 +1642,77 @@
             .bind(id).execute(&self.pool).await?;
         Ok(r.rows_affected() == 1)
     }
+    async fn record_usage_threshold_notifications(&self, limit: u32) -> WalletRepoResult<u64> {
+        let cap = i64::from(limit.clamp(1, 100));
+        let mut total = 0u64;
+        let daily = sqlx::query(
+            r#"WITH usage AS (
+                   SELECT e.tenant_id, SUM(e.charge_units)::numeric AS usage_units
+                   FROM api_usage_events e
+                   WHERE e.status='CAPTURED'
+                     AND e.created_at >= date_trunc('day', now())
+                     AND e.created_at < date_trunc('day', now()) + interval '1 day'
+                   GROUP BY e.tenant_id
+               ), candidates AS (
+                   SELECT w.tenant_id, u.usage_units
+                   FROM usage u
+                   JOIN wallet_settings s ON s.tenant_id=u.tenant_id
+                   JOIN user_wallets w ON w.tenant_id=u.tenant_id
+                   WHERE s.daily_usage_alert_threshold_units IS NOT NULL
+                     AND s.daily_usage_alert_threshold_units > 0
+                     AND u.usage_units >= s.daily_usage_alert_threshold_units
+                     AND NOT EXISTS (
+                         SELECT 1 FROM wallet_notifications n
+                         WHERE n.tenant_id=u.tenant_id AND n.event_type='DAILY_USAGE'
+                           AND n.dedupe_key='daily:' || to_char(current_date, 'YYYY-MM-DD')
+                     )
+                   ORDER BY u.usage_units DESC
+                   LIMIT $1
+               )
+               INSERT INTO wallet_notifications(id,tenant_id,event_type,dedupe_key,message)
+               SELECT gen_random_uuid(), tenant_id, 'DAILY_USAGE',
+                      'daily:' || to_char(current_date, 'YYYY-MM-DD'),
+                      'Daily PAYG usage has reached ' || to_char(ROUND(usage_units / 1000000.0, 2), 'FM999999990.00') || ' USD.'
+               FROM candidates
+               ON CONFLICT (tenant_id,event_type,dedupe_key) DO NOTHING"#,
+        )
+        .bind(cap).execute(&self.pool).await?;
+        total += daily.rows_affected();
+
+        let monthly = sqlx::query(
+            r#"WITH usage AS (
+                   SELECT e.tenant_id, SUM(e.charge_units)::numeric AS usage_units
+                   FROM api_usage_events e
+                   WHERE e.status='CAPTURED'
+                     AND e.created_at >= date_trunc('month', now())
+                     AND e.created_at < date_trunc('month', now()) + interval '1 month'
+                   GROUP BY e.tenant_id
+               ), candidates AS (
+                   SELECT w.tenant_id, u.usage_units
+                   FROM usage u
+                   JOIN wallet_settings s ON s.tenant_id=u.tenant_id
+                   JOIN user_wallets w ON w.tenant_id=u.tenant_id
+                   WHERE s.monthly_usage_alert_threshold_units IS NOT NULL
+                     AND s.monthly_usage_alert_threshold_units > 0
+                     AND u.usage_units >= s.monthly_usage_alert_threshold_units
+                     AND NOT EXISTS (
+                         SELECT 1 FROM wallet_notifications n
+                         WHERE n.tenant_id=u.tenant_id AND n.event_type='MONTHLY_USAGE'
+                           AND n.dedupe_key='monthly:' || to_char(date_trunc('month', now()), 'YYYY-MM')
+                     )
+                   ORDER BY u.usage_units DESC
+                   LIMIT $1
+               )
+               INSERT INTO wallet_notifications(id,tenant_id,event_type,dedupe_key,message)
+               SELECT gen_random_uuid(), tenant_id, 'MONTHLY_USAGE',
+                      'monthly:' || to_char(date_trunc('month', now()), 'YYYY-MM'),
+                      'Monthly PAYG usage has reached ' || to_char(ROUND(usage_units / 1000000.0, 2), 'FM999999990.00') || ' USD.'
+               FROM candidates
+               ON CONFLICT (tenant_id,event_type,dedupe_key) DO NOTHING"#,
+        )
+        .bind(cap).execute(&self.pool).await?;
+        total += monthly.rows_affected();
+        Ok(total)
+    }
+
 }
--- a/crates/infra/src/services/notification_dispatcher.rs
+++ b/crates/infra/src/services/notification_dispatcher.rs
@@ -157,6 +157,7 @@
 
                         monitor_id: None,
                         execution_id: None,
+                    pricing_context: None,
                     },
                     || self.sms.notify(channel, event),
                 )
--- a/crates/infra/tests/payg_charge_for_usage.rs
+++ b/crates/infra/tests/payg_charge_for_usage.rs
@@ -69,6 +69,7 @@
 
         monitor_id: None,
         execution_id: None,
+    pricing_context: None,
     };
 
     let c = calls.clone();
@@ -133,6 +134,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             };
 
             uc.execute(input, || async { Ok::<_, &'static str>(()) })
@@ -183,6 +185,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async { Ok::<_, &'static str>(()) },
         )
@@ -232,6 +235,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async { Err::<(), _>("downstream failed") },
         )
@@ -285,6 +289,7 @@
 
         monitor_id: None,
         execution_id: None,
+    pricing_context: None,
     };
 
     // Simulate a worker crash after reservation creation by directly reserving
@@ -346,6 +351,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async { Err::<(), _>("HTTP 500") },
         )
@@ -412,6 +418,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async { Ok::<_, &'static str>(()) },
         )
@@ -455,6 +462,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async {
                 Ok::<_, &'static str>(application::MeteredAttempt {
@@ -510,6 +518,7 @@
 
         monitor_id: None,
         execution_id: None,
+    pricing_context: None,
     };
     let result = uc
         .execute_classified_attempt(input.clone(), || async {
@@ -597,6 +606,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async {
                 Ok::<_, &'static str>(application::MeteredAttempt {
@@ -652,6 +662,7 @@
 
                     monitor_id: None,
                     execution_id: None,
+                    pricing_context: None,
                 },
                 || async {
                     Ok::<_, &'static str>(application::MeteredAttempt {
@@ -745,6 +756,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async { Ok::<_, &'static str>(()) },
         )
--- a/crates/infra/tests/worker9_wallet_payg.rs
+++ b/crates/infra/tests/worker9_wallet_payg.rs
@@ -60,6 +60,7 @@
 
         monitor_id: None,
         execution_id: None,
+    pricing_context: None,
     };
 
     let mut tasks = Vec::new();
@@ -207,6 +208,7 @@
 
                     monitor_id: None,
                     execution_id: None,
+                    pricing_context: None,
                 },
                 || async { Ok::<_, &'static str>(()) },
             )
@@ -222,6 +224,7 @@
 
                     monitor_id: None,
                     execution_id: None,
+                    pricing_context: None,
                 },
                 || async { panic!("idempotent runtime operation was executed twice") },
             )
@@ -391,6 +394,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async { Ok::<_, &'static str>(()) },
         )
@@ -406,6 +410,7 @@
 
                 monitor_id: None,
                 execution_id: None,
+            pricing_context: None,
             },
             || async { panic!("durable retry executed network operation twice") },
         )
--- a/crates/worker/src/durable_scheduler.rs
+++ b/crates/worker/src/durable_scheduler.rs
@@ -239,7 +239,14 @@
                         let result = if let Some(metering) = metering {
                             match metering.execute_classified_attempt(
                                 MeterUsageInput { tenant_id: monitor.tenant_id.0, operation, units: 1, request_id: billing_request_id,
-            monitor_id: Some(monitor.id.0), execution_id: Some(claim.request.execution_id),
+                                    monitor_id: Some(monitor.id.0),
+                                    execution_id: Some(claim.request.execution_id),
+                                    pricing_context: Some(application::ports::PricingContext {
+                                        monitor_kind: Some(monitor.kind),
+                                        region: monitor.regions.first().cloned(),
+                                        quantity: 1,
+                                        account_type: None,
+                                    }),
         },
                                 move || {
                                     let executor = executor.clone();
@@ -315,7 +322,14 @@
                                     units: 1,
                                     request_id,
 
-            monitor_id: Some(monitor.id.0), execution_id: Some(claim.execution_id),
+                                    monitor_id: Some(monitor.id.0),
+                                    execution_id: Some(claim.execution_id),
+                                    pricing_context: Some(application::ports::PricingContext {
+                                        monitor_kind: Some(monitor.kind),
+                                        region: monitor.regions.first().cloned(),
+                                        quantity: 1,
+                                        account_type: None,
+                                    }),
         },
                                 || {
                                     let executor = executor.clone();
--- a/crates/worker/src/main.rs
+++ b/crates/worker/src/main.rs
@@ -115,7 +115,8 @@
             payg_pricing_repository,
             usage_event_repository,
         )
-        .with_audit_log_repository(financial_audit_repository.clone()),
+        .with_audit_log_repository(financial_audit_repository.clone())
+        .with_tenant_repository(tenant_repository.clone()),
     );
     let bulk_import_worker = BulkMonitorImportWorker::new(
         Arc::new(PostgresBulkMonitorImportRepository::new(pool.clone())),
--- a/crates/worker/src/proxy_checker.rs
+++ b/crates/worker/src/proxy_checker.rs
@@ -738,6 +738,7 @@
 
             monitor_id: None,
             execution_id: None,
+            pricing_context: None,
         },
                     || async {
                         if had_existing_result {
--- a/crates/worker/src/wallet_notification_email_worker.rs
+++ b/crates/worker/src/wallet_notification_email_worker.rs
@@ -33,6 +33,9 @@
             tokio::select! {
                 _ = cancel.cancelled() => return Ok(()),
                 _ = ticker.tick() => {
+                    if let Err(error) = self.wallet.record_usage_threshold_notifications(50).await {
+                        tracing::warn!(%error, "failed to record PAYG usage threshold notifications");
+                    }
                     for n in self.wallet.claim_pending_email_notifications(50).await? {
                         let channels=self.channels.list_enabled_email_by_tenant(domain::shared::ids::TenantId(n.tenant_id)).await?;
                         let subject=format!("PingVertex PAYG: {}", n.event_type.replace('_'," "));
--- /dev/null
+++ b/migrations/0098_payg_part3_pricing_context.sql
@@ -0,0 +1,52 @@
+BEGIN;
+
+-- Optional, server-owned pricing dimensions. NULL means "generic fallback".
+ALTER TABLE payg_pricing_configs
+    ADD COLUMN IF NOT EXISTS account_type TEXT,
+    ADD COLUMN IF NOT EXISTS region TEXT,
+    ADD COLUMN IF NOT EXISTS monitor_kind TEXT,
+    ADD COLUMN IF NOT EXISTS min_quantity BIGINT,
+    ADD COLUMN IF NOT EXISTS max_quantity BIGINT;
+
+ALTER TABLE payg_pricing_configs
+    DROP CONSTRAINT IF EXISTS chk_payg_pricing_context_account_type;
+ALTER TABLE payg_pricing_configs
+    ADD CONSTRAINT chk_payg_pricing_context_account_type
+    CHECK (account_type IS NULL OR account_type IN ('individual','company'));
+
+ALTER TABLE payg_pricing_configs
+    DROP CONSTRAINT IF EXISTS chk_payg_pricing_context_region;
+ALTER TABLE payg_pricing_configs
+    ADD CONSTRAINT chk_payg_pricing_context_region
+    CHECK (region IS NULL OR region IN ('us-east','eu-west','af-west','ap-southeast'));
+
+ALTER TABLE payg_pricing_configs
+    DROP CONSTRAINT IF EXISTS chk_payg_pricing_context_monitor_kind;
+ALTER TABLE payg_pricing_configs
+    ADD CONSTRAINT chk_payg_pricing_context_monitor_kind
+    CHECK (monitor_kind IS NULL OR monitor_kind IN ('http','tcp','dns','heartbeat','flow'));
+
+ALTER TABLE payg_pricing_configs
+    DROP CONSTRAINT IF EXISTS chk_payg_pricing_context_quantity;
+ALTER TABLE payg_pricing_configs
+    ADD CONSTRAINT chk_payg_pricing_context_quantity
+    CHECK (
+        (min_quantity IS NULL OR min_quantity >= 0)
+        AND (max_quantity IS NULL OR max_quantity >= 0)
+        AND (min_quantity IS NULL OR max_quantity IS NULL OR min_quantity <= max_quantity)
+    );
+
+CREATE INDEX IF NOT EXISTS idx_payg_pricing_context_lookup
+    ON payg_pricing_configs(
+        operation,
+        active,
+        effective_from,
+        account_type,
+        region,
+        monitor_kind,
+        min_quantity,
+        max_quantity,
+        version DESC
+    );
+
+COMMIT;
--- /dev/null
+++ b/migrations/0099_payg_usage_alerts.sql
@@ -0,0 +1,25 @@
+BEGIN;
+
+ALTER TABLE wallet_settings
+    ADD COLUMN IF NOT EXISTS daily_usage_alert_threshold_units BIGINT,
+    ADD COLUMN IF NOT EXISTS monthly_usage_alert_threshold_units BIGINT;
+
+ALTER TABLE wallet_settings DROP CONSTRAINT IF EXISTS chk_wallet_daily_usage_alert_threshold;
+ALTER TABLE wallet_settings ADD CONSTRAINT chk_wallet_daily_usage_alert_threshold
+    CHECK (daily_usage_alert_threshold_units IS NULL OR daily_usage_alert_threshold_units > 0);
+
+ALTER TABLE wallet_settings DROP CONSTRAINT IF EXISTS chk_wallet_monthly_usage_alert_threshold;
+ALTER TABLE wallet_settings ADD CONSTRAINT chk_wallet_monthly_usage_alert_threshold
+    CHECK (monthly_usage_alert_threshold_units IS NULL OR monthly_usage_alert_threshold_units > 0);
+
+ALTER TABLE wallet_settings DROP CONSTRAINT IF EXISTS chk_wallet_usage_alert_thresholds;
+ALTER TABLE wallet_settings ADD CONSTRAINT chk_wallet_usage_alert_thresholds
+    CHECK (
+        (daily_usage_alert_threshold_units IS NULL OR monthly_usage_alert_threshold_units IS NULL)
+        OR monthly_usage_alert_threshold_units >= daily_usage_alert_threshold_units
+    );
+
+CREATE INDEX IF NOT EXISTS idx_api_usage_events_tenant_status_created
+    ON api_usage_events(tenant_id, status, created_at DESC);
+
+COMMIT;
PATCH_EOF

echo "== PAYG Part 3 completion =="
echo "Project: $ROOT"
echo "Backup:  $ROOT/$BACKUP_DIR"
echo

echo "[1/4] Checking patch applicability..."
patch -p1 --dry-run < "$PATCH_FILE" >/dev/null
echo "      Patch dry-run: PASS"

echo "[2/4] Creating backup of existing files..."
mkdir -p "$BACKUP_DIR"
grep '^--- a/' "$PATCH_FILE" | sed 's#^--- a/##' | sort -u | while IFS= read -r file; do
  if [[ -f "$file" ]]; then
    mkdir -p "$BACKUP_DIR/$(dirname "$file")"
    cp -a "$file" "$BACKUP_DIR/$file"
  fi
done
echo "      Backup created."

echo "[3/4] Applying source/database/UI changes..."
patch -p1 < "$PATCH_FILE"
echo "      Patch applied: PASS"

echo "[4/4] Finished."
echo
echo "PAYG Part 3 changes are now installed."
echo
echo "Next run:"
echo "  cargo fmt --all -- --check"
echo "  cargo test --workspace --all-targets"
echo
echo "If formatting/checking reports an error, do not rerun this script."
echo "Fix only the reported file(s) and send me the error output; I will provide"
echo "a small follow-up .sh containing only those corrective changes."
