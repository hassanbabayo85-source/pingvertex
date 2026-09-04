use crate::api::developer;
use crate::auth::context::{use_auth, AuthState, Role};
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role: &Role) -> bool {
    matches!(role, Role::Admin | Role::Owner)
}

#[component]
pub fn ApiKeysPage() -> impl IntoView {
    let auth = use_auth();
    let keys = RwSignal::new(Vec::<developer::ApiKey>::new());
    let name = RwSignal::new(String::new());
    let scopes = RwSignal::new(Vec::<String>::new());
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let secret = RwSignal::new(None::<String>);
    let pending_revoke = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_manage(&u.role),
        _ => false,
    };

    let reload = move || {
        spawn_local(async move {
            match developer::list_keys().await {
                Ok(v) => keys.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    };
    reload();

    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let n = name.get().trim().to_string();
        let s = scopes.get();
        if n.is_empty() {
            error.set(Some("Key name is required.".into()));
            return;
        }
        if s.is_empty() {
            error.set(Some("Select at least one scope.".into()));
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match developer::create_key(&developer::CreateApiKeyRequest {
                name: n,
                scopes: s,
                expires_at: None,
            }).await {
                Ok(k) => {
                    secret.set(Some(k.key));
                    notice.set(Some("API key created. Copy the secret now; it will not be shown again.".into()));
                    reload();
                    name.set(String::new());
                    scopes.set(Vec::new());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Developer"</span><h1>"API keys"</h1></div></header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || secret.get().map(|s| view! {
                <div class="secret-box">
                    <strong>"New API key secret"</strong>
                    <code>{s}</code>
                    <span class="muted">"This secret is displayed once and is never written to browser storage."</span>
                    <button class="btn" type="button" on:click=move |_| secret.set(None)>"Hide secret"</button>
                </div>
            })}
            <section class="card">
                <div class="section-heading"><h2>"Active keys"</h2><span class="muted">"Only prefixes are retained in the list response."</span></div>
                {move || if keys.get().is_empty() {
                    view! { <div class="empty-state"><h3>"No API keys"</h3><p>"Create a scoped key for programmatic access."</p></div> }.into_any()
                } else {
                    view! { <div class="list-stack">
                        {move || keys.get().into_iter().map(|k| {
                            let id = k.id.clone();
                            let scopes_text = k.scopes.join(", ");
                            view! { <div class="list-row">
                                <div>
                                    <strong>{k.name}</strong>
                                    <div class="muted"><code>{k.key_prefix}</code> · {scopes_text}</div>
                                    <div class="muted">{format!("Created {}{}", k.created_at, k.expires_at.as_ref().map(|x| format!(" · expires {x}")).unwrap_or_default())}</div>
                                </div>
                                {move || if can() {
                                    let revoke_id = id.clone();
                                    view! { <button class="btn btn-danger" disabled=move || busy.get() on:click=move |_| pending_revoke.set(Some(revoke_id.clone()))>"Revoke"</button> }.into_any()
                                } else { ().into_any() }}
                            </div> }
                        }).collect_view()}
                    </div> }.into_any()
                }}
            </section>
            {move || if can() {
                view! {
                    <section class="card">
                        <h2>"Create key"</h2>
                        <p class="muted">"Grant the minimum scopes required. Stored secrets are never returned by the list endpoint."</p>
                        <form on:submit=create class="stack-form">
                            <label>"Name"<input autocomplete="off" maxlength="100" prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label>
                            <fieldset>
                                <legend>"Scopes"</legend>
                                {developer::API_KEY_SCOPES.iter().map(|scope| {
                                    let s = (*scope).to_string();
                                    view! { <label class="check-row">
                                        <input type="checkbox" on:change=move|e| {
                                            let checked = event_target::<web_sys::HtmlInputElement>(&e).checked();
                                            scopes.update(|v| if checked {
                                                if !v.contains(&s) { v.push(s.clone()) }
                                            } else { v.retain(|x| x != &s) });
                                        }/>
                                        {*scope}
                                    </label> }
                                }).collect_view()}
                            </fieldset>
                            <button class="btn btn-primary" disabled=move || busy.get()>{move || if busy.get() {"Creating…"} else {"Create key"}}</button>
                        </form>
                    </section>
                }.into_any()
            } else { ().into_any() }}
            {move || pending_revoke.get().map(|id| {
                let confirm_id = id.clone();
                view! {
                    <ConfirmDialog
                        title="Revoke API key"
                        body="Any client using this key will lose access immediately."
                        on_confirm=Callback::new(move |_| {
                            let id = confirm_id.clone();
                            busy.set(true);
                            spawn_local(async move {
                                match developer::delete_key(&id).await {
                                    Ok(()) => {
                                        keys.update(|v| v.retain(|x| x.id != id));
                                        notice.set(Some("API key revoked.".into()));
                                    }
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                pending_revoke.set(None);
                                busy.set(false);
                            });
                        })
                        on_cancel=Callback::new(move |_| pending_revoke.set(None))
                    />
                }
            })}
        </div>
    }
}
