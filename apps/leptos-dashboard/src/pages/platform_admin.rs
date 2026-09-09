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
    action: String,
    target_id: String,
    created_at: String,
}
#[derive(Clone, Debug, Serialize)]
struct AdjustmentRequest {
    delta_units: i64,
    idempotency_key: String,
    description: Option<String>,
}

#[component]
pub fn PlatformAdminPage() -> impl IntoView {
    let allowed = RwSignal::new(false);
    let companies = RwSignal::new(Vec::<Company>::new());
    let users = RwSignal::new(Vec::<PlatformUser>::new());
    let audit = RwSignal::new(Vec::<AuditEntry>::new());
    let error = RwSignal::new(None::<String>);
    let adjustment = RwSignal::new(String::new());

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
                match client::get_json::<Vec<AuditEntry>>("/admin/audit").await {
                    Ok(value) => audit.set(value),
                    Err(value) => error.set(Some(value.to_string())),
                }
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
                                        view! {
                                            <tr>
                                                <td>{company.name}</td>
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
                    <section class="card"><h2>"Audit"</h2><div class="table-shell"><table class="data-table"><thead><tr><th>"Time"</th><th>"Action"</th><th>"Target"</th></tr></thead><tbody>{audit.get().into_iter().map(|entry|view!{<tr><td>{entry.created_at}</td><td>{format!("{:?}",entry.action)}</td><td>{entry.target_id}</td></tr>}).collect_view()}</tbody></table></div></section>
                }.into_any()
            } else { ().into_any() }}
        </div>
    }
}
