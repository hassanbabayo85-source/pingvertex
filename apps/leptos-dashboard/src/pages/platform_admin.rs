use crate::api::client;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;

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
        path.push_str(
            &js_sys::encode_uri_component(&audit_action.get())
                .as_string()
                .unwrap_or_default(),
        );
    }
    if !audit_tenant.get().is_empty() {
        path.push_str("&tenant=");
        path.push_str(
            &js_sys::encode_uri_component(&audit_tenant.get())
                .as_string()
                .unwrap_or_default(),
        );
    }
    if !audit_from.get().is_empty() {
        path.push_str("&from=");
        path.push_str(
            &js_sys::encode_uri_component(&format!("{}T00:00:00Z", audit_from.get()))
                .as_string()
                .unwrap_or_default(),
        );
    }
    if !audit_to.get().is_empty() {
        path.push_str("&to=");
        path.push_str(
            &js_sys::encode_uri_component(&format!("{}T23:59:59Z", audit_to.get()))
                .as_string()
                .unwrap_or_default(),
        );
    }
    spawn_local(async move {
        match client::get_json::<serde_json::Value>(&path).await {
            Ok(value) => match serde_json::from_value::<Vec<AuditEntry>>(
                value
                    .get("items")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!([])),
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
                load_audit(
                    audit_action,
                    audit_tenant,
                    audit,
                    error,
                    audit_from,
                    audit_to,
                );
            }
            Ok(_) => error.set(Some("Platform administrator access required.".to_owned())),
            Err(value) => error.set(Some(value.to_string())),
        }
    });

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
                    // Show the full company usage details when one is selected.

                    <section class="card">
                        <h2>"Companies"</h2>
                        <div class="table-shell">
                            <table class="data-table">
                                <thead><tr><th>"Company"</th><th>"Status"</th><th>"Actions"</th></tr></thead>
                                <tbody>
                                    {companies.get().into_iter().map(|company| {
                                        let company_id = company.id.clone();
                                        let next_status = if company.status == "active" { "suspended" } else { "active" };

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



                                        view! {
                                            <tr>
                                                <td>{company.name}</td>
                                                <td>{company.status}</td>
                                                <td>
                                                    <button class="button button-secondary" on:click=on_status>{next_status}</button>
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
