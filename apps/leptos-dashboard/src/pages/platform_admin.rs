use crate::api::client;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
struct Session {
    authenticated: bool,
    user: SessionUser,
}
#[derive(Clone, Debug, Deserialize)]
struct SessionUser {
    platform_admin: bool,
}
#[derive(Clone, Debug, Deserialize)]
struct Company {
    id: String,
    name: String,
    status: String,
    wallet_balance_units: i64,
}
#[derive(Clone, Debug, Deserialize)]
struct PlatformUser {
    id: String,
    username: String,
    email: String,
    status: String,
}
#[derive(Clone, Debug, Deserialize)]
struct AuditEntry {
    #[allow(dead_code)]
    id: String,
    actor_user_id: String,
    tenant_id: Option<String>,
    action: String,
    target_id: String,
    metadata: serde_json::Value,
    created_at: String,
}
#[derive(Clone, Debug, Serialize)]
struct AdjustmentRequest {
    delta_units: i64,
    idempotency_key: String,
    description: Option<String>,
}

fn load_audit(
    audit_action: RwSignal<String>,
    audit_tenant: RwSignal<String>,
    audit: RwSignal<Vec<AuditEntry>>,
    error: RwSignal<Option<String>>,
    audit_from: RwSignal<String>,
    audit_to: RwSignal<String>,
) {
    let mut path = "/admin/audit?limit=50".to_owned();
    if !audit_action.get().is_empty() {
        path.push_str("&action=");
        path.push_str(&js_sys::encode_uri_component(&audit_action.get()).as_string().unwrap_or_default());
    }
    if !audit_tenant.get().is_empty() {
        path.push_str("&tenant=");
        path.push_str(&js_sys::encode_uri_component(&audit_tenant.get()).as_string().unwrap_or_default());
    }
    if !audit_from.get().is_empty() {
        path.push_str("&from=");
        path.push_str(&js_sys::encode_uri_component(&format!("{}T00:00:00Z", audit_from.get())).as_string().unwrap_or_default());
    }
    if !audit_to.get().is_empty() {
        path.push_str("&to=");
        path.push_str(&js_sys::encode_uri_component(&format!("{}T23:59:59Z", audit_to.get())).as_string().unwrap_or_default());
    }
    spawn_local(async move {
        match client::get_json::<serde_json::Value>(&path).await {
            Ok(value) => match serde_json::from_value::<Vec<AuditEntry>>(
                value.get("items").cloned().unwrap_or_else(|| serde_json::json!([])),
            ) {
                Ok(items) => audit.set(items),
                Err(value) => error.set(Some(value.to_string())),
            },
            Err(value) => error.set(Some(value.to_string())),
        }
    });
}

#[component]
pub fn PlatformAdminPage() -> impl IntoView {
    let allowed = RwSignal::new(false);
    let companies = RwSignal::new(Vec::<Company>::new());
    let users = RwSignal::new(Vec::<PlatformUser>::new());
    let audit = RwSignal::new(Vec::<AuditEntry>::new());
    let audit_action = RwSignal::new(String::new());
    let audit_tenant = RwSignal::new(String::new());
    let audit_from = RwSignal::new(String::new());
    let audit_to = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let adjustment = RwSignal::new(String::new());
    let pricing_operation = RwSignal::new("http_check.monitoring".to_owned());
    let pricing_units = RwSignal::new("2".to_owned());
    let pricing_reason = RwSignal::new(String::new());

    // 🟡 MATAKI NA 3: Sabbin Signals domin Pricing da Usage
    let active_pricing = RwSignal::new(None::<String>);
    let pricing_logs = RwSignal::new(None::<String>);
    let selected_company_usage = RwSignal::new(None::<(String, String)>);

    spawn_local(async move {
        match client::get_json::<Session>("/auth/me").await {
            Ok(session) if session.authenticated && session.user.platform_admin => {
                allowed.set(true);
                match client::get_json::<Vec<Company>>("/admin/companies").await {
                    Ok(value) => companies.set(value),
                    Err(value) => error.set(Some(value.to_string())),
                }
                match client::get_json::<Vec<PlatformUser>>("/admin/users").await {
                    Ok(value) => users.set(value),
                    Err(value) => error.set(Some(value.to_string())),
                }
                load_audit(audit_action, audit_tenant, audit, error, audit_from, audit_to);
                
                // 🟡 MATAKI NA 3: Ɗauko farashin da ke aiki da kuma audit logs
                if let Ok(v) = client::get_json::<serde_json::Value>("/admin/pricing/configs/active").await {
                    active_pricing.set(Some(serde_json::to_string_pretty(&v).unwrap_or_default()));
                }
                if let Ok(v) = client::get_json::<serde_json::Value>("/admin/pricing/audit-logs").await {
                    pricing_logs.set(Some(serde_json::to_string_pretty(&v).unwrap_or_default()));
                }
            }
            Ok(_) => error.set(Some("Platform administrator access required.".to_owned())),
            Err(value) => error.set(Some(value.to_string())),
        }
    });

    // 🟡 MATAKI NA 3: Function domin ɗauko amfanin kamfani (Company Usage)
    let fetch_company_usage = move |id: String, name: String| {
        spawn_local(async move {
            match client::get_json::<serde_json::Value>(&format!("/admin/companies/{}/usage", id)).await {
                Ok(v) => selected_company_usage.set(Some((name, serde_json::to_string_pretty(&v).unwrap_or_default()))),
                Err(e) => selected_company_usage.set(Some((name, format!("Error loading usage: {}", e)))),
            }
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"PLATFORM"</span>
                    <h1>"Administration"</h1>
                    <p class="muted">"Global controls. Mutations are server-authorized, idempotent and audited."</p>
                </div>
            </header>
            
            {move || error.get().map(|value| view! { <div class="error" role="alert">{value}</div> })}
            
            {move || if allowed.get() {
                view! {
                    // 🟡 MATAKI NA 3: Nuna cikakken bayanin amfanin kamfani idan an zaɓa
                    {move || selected_company_usage.get().map(|(name, usage_data)| {
                        view! {
                            <div class="card" style="margin-bottom: 2rem; border-left: 4px solid var(--primary-color);">
                                <div style="display: flex; justify-content: space-between; align-items: flex-start;">
                                    <h2>"Usage Report: " {name}</h2>
                                    <button class="btn" on:click=move |_| selected_company_usage.set(None)>"Close"</button>
                                </div>
                                <pre style="background: #f4f4f4; padding: 1rem; border-radius: 4px; overflow-x: auto; max-height: 400px; margin-top: 1rem;">
                                    {usage_data}
                                </pre>
                            </div>
                        }
                    })}

                    <section class="card">
                        <h2>"Companies"</h2>
                        <div class="table-shell">
                            <table class="data-table">
                                <thead><tr><th>"Company"</th><th>"Status"</th><th>"Wallet"</th><th>"Actions"</th></tr></thead>
                                <tbody>
                                    {companies.get().into_iter().map(|company| {
                                        let company_id = company.id.clone();
                                        let next_status = if company.status == "active" { "suspended" } else { "active" };
                                        let adjustment_signal = adjustment;
                                        
                                        // Domin Status Toggle
                                        let status_id = company_id.clone();
                                        let status_value = next_status.to_owned();
                                        let on_status = move |_| {
                                            let id = status_id.clone();
                                            let status = status_value.clone();
                                            spawn_local(async move {
                                                let body = serde_json::json!({"status": status});
                                                let _ = client::post_json_no_content_with_body(&format!("/admin/companies/{id}/status"), &body).await;
                                            });
                                        };
                                        
                                        // Domin Wallet Adjust
                                        let adjust_id = company_id.clone();
                                        let on_adjust = move |_| {
                                            let amount = adjustment_signal.get().parse::<i64>().unwrap_or(0);
                                            if amount == 0 { return; }
                                            let id = adjust_id.clone();
                                            spawn_local(async move {
                                                let body = AdjustmentRequest { delta_units: amount, idempotency_key: format!("admin-adjust-{}", js_sys::Date::now() as u64), description: Some("Platform administrator wallet adjustment".to_owned()) };
                                                let _ = client::post_json::<serde_json::Value, _>(&format!("/admin/companies/{id}/wallet-adjustment"), &body).await;
                                            });
                                        };

                                        // 🟡 MATAKI NA 3: Sanya sunan kamfani ya zama Clickable domin ganin Usage
                                        let usage_id = company.id.clone();
                                        let usage_name = company.name.clone();
                                        let on_company_click = move |e: leptos::ev::MouseEvent| {
                                            e.prevent_default();
                                            fetch_company_usage(usage_id.clone(), usage_name.clone());
                                        };

                                        view! {
                                            <tr>
                                                <td><a href="#" on:click=on_company_click style="text-decoration: underline; color: var(--primary-color);">{company.name}</a></td>
                                                <td>{company.status}</td>
                                                <td>{company.wallet_balance_units}</td>
                                                <td>
                                                    <button class="button button-secondary" on:click=on_status>{next_status}</button>
                                                    <input aria-label="wallet adjustment" placeholder="delta" prop:value=move || adjustment_signal.get() on:input=move |event| adjustment_signal.set(event_target_value(&event)) />
                                                    <button class="button button-secondary" on:click=on_adjust>{"Adjust"}</button>
                                                    <button class="button button-secondary" on:click={move |_| { let amount=adjustment_signal.get().parse::<i64>().unwrap_or(0); if amount<=0{return;} let id=company_id.clone(); spawn_local(async move { let body=AdjustmentRequest{delta_units:amount,idempotency_key:format!("admin-promo-{}",js_sys::Date::now() as u64),description:Some("Platform administrator promotional credit".to_owned())}; let _=client::post_json::<serde_json::Value,_>(&format!("/admin/companies/{id}/promotional-credit"),&body).await; }); }}>{"Promo"}</button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    </section>
                    
                    <section class="card">
                        <h2>"Accounts"</h2>
                        <div class="table-shell"><table class="data-table"><thead><tr><th>"Username"</th><th>"Email"</th><th>"Status"</th><th>"Action"</th></tr></thead><tbody>
                            {users.get().into_iter().map(|user| { let id=user.id.clone(); let next=if user.status=="active"{"suspended"}else{"active"}; let on_status=move |_| { let id=id.clone(); let status=next.to_owned(); spawn_local(async move { let body=serde_json::json!({"status":status}); let _=client::post_json_no_content_with_body(&format!("/admin/users/{id}/status"),&body).await; }); }; view! { <tr><td>{user.username}</td><td>{user.email}</td><td>{user.status}</td><td><button class="button button-secondary" on:click=on_status>{next}</button></td></tr> } }).collect_view()}
                        </tbody></table></div>
                    </section>
                    
                    <section class="card">
                        <div class="section-heading">
                            <div>
                                <h2>"PAYG pricing"</h2>
                                <p class="muted">"Create a new immutable price version; historical versions cannot be edited."</p>
                            </div>
                        </div>

                        // 🟡 MATAKI NA 3: Nuna Active Pricing da Audit Logs kafin form ɗin
                        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; margin-bottom: 1.5rem;">
                            <div>
                                <h3 style="font-size: 1rem; margin-bottom: 0.5rem;">"Active Pricing Configuration"</h3>
                                <pre style="background: #f4f4f4; padding: 1rem; border-radius: 4px; overflow-x: auto; max-height: 250px; font-size: 0.85rem;">
                                    {move || active_pricing.get().unwrap_or_else(|| "Loading...".to_string())}
                                </pre>
                            </div>
                            <div>
                                <h3 style="font-size: 1rem; margin-bottom: 0.5rem;">"Recent Pricing Changes (Audit)"</h3>
                                <pre style="background: #f4f4f4; padding: 1rem; border-radius: 4px; overflow-x: auto; max-height: 250px; font-size: 0.85rem;">
                                    {move || pricing_logs.get().unwrap_or_else(|| "Loading...".to_string())}
                                </pre>
                            </div>
                        </div>

                        <div class="form-grid">
                            <label>"Operation"<select prop:value=pricing_operation on:change=move |e| pricing_operation.set(event_target_value(&e))><option value="http_check.monitoring">"HTTP check"</option><option value="tcp_check.monitoring">"TCP check"</option><option value="dns_check.monitoring">"DNS check"</option><option value="api_flow_check.monitoring">"API flow"</option><option value="network_egress">"Network egress"</option></select></label>
                            <label>"Unit price (micro-USD)"<input type="number" min="1" prop:value=pricing_units on:input=move |e| pricing_units.set(event_target_value(&e)) /></label>
                            <label>"Reason"<input placeholder="required change reason" prop:value=pricing_reason on:input=move |e| pricing_reason.set(event_target_value(&e)) /></label>
                            <button class="button button-secondary" on:click=move |_| {
                                let operation=pricing_operation.get(); let units=pricing_units.get().parse::<i64>().unwrap_or(0); let reason=pricing_reason.get();
                                if units<=0 || reason.trim().is_empty() { return; }
                                spawn_local(async move {
                                    let body=serde_json::json!({"operation":operation,"unit_name":"check","unit_price":units,"currency":"USD","reason":reason});
                                    if let Err(e)=client::post_json::<serde_json::Value,_>("/admin/pricing/payg-configs",&body).await { error.set(Some(e.to_string())); }
                                });
                            } >"Create version"</button>
                        </div>
                    </section>
                    
                    <section class="card">
                        <div class="section-heading"><div><h2>"Audit search"</h2><p class="muted">"Bounded, filterable platform audit history."</p></div></div>
                        <div class="form-grid">
                            <label>"Action"<input placeholder="e.g. BulkImportConfirmed" prop:value=audit_action on:input=move |e| audit_action.set(event_target_value(&e)) /></label>
                            <label>"Tenant ID"<input placeholder="UUID" prop:value=audit_tenant on:input=move |e| audit_tenant.set(event_target_value(&e)) /></label>
                            <label>"From"<input type="date" prop:value=audit_from on:change=move |e| audit_from.set(event_target_value(&e)) /></label>
                            <label>"To"<input type="date" prop:value=audit_to on:change=move |e| audit_to.set(event_target_value(&e)) /></label>
                            <button class="button button-secondary" on:click=move |_| load_audit(audit_action, audit_tenant, audit, error, audit_from, audit_to)>"Search"</button>
                        </div>
                        <div class="table-shell"><table class="data-table"><thead><tr><th>"Time"</th><th>"Actor"</th><th>"Tenant"</th><th>"Action"</th><th>"Target"</th><th>"Details"</th></tr></thead><tbody>{audit.get().into_iter().map(|entry|view!{<tr><td>{entry.created_at}</td><td><code>{entry.actor_user_id}</code></td><td>{entry.tenant_id.unwrap_or_else(|| "platform".to_owned())}</td><td>{entry.action}</td><td>{entry.target_id}</td><td><code>{entry.metadata.to_string()}</code></td></tr>}).collect_view()}</tbody></table></div>
                    </section>
                }.into_any()
            } else { ().into_any() }}
        </div>
    }
}
