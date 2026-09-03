use crate::api::developer;
use crate::auth::context::{use_auth, AuthState, Role};
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
    let secret = RwSignal::new(None::<String>);
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
        let n = name.get();
        let s = scopes.get();
        busy.set(true);
        spawn_local(async move {
            match developer::create_key(&developer::CreateApiKeyRequest {
                name: n,
                scopes: s,
                expires_at: None,
            })
            .await
            {
                Ok(k) => {
                    secret.set(Some(k.key));
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
            <header class="page-header"><h1>"API keys"</h1></header>
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || secret.get().map(|s| view! { <div class="secret-box"><strong>"API key — copy it now"</strong><code>{s}</code><span class="muted">"It will not be shown again."</span></div> })}
            <section class="card">
                <h2>"Keys"</h2>
                {move || keys.get().into_iter().map(|k| {
                    let id = k.id.clone();
                    view! {
                        <div class="list-row">
                            <div>
                                <strong>{k.name}</strong>
                                <div class="muted"><code>{k.key_prefix}</code>  -  {k.scopes.join(", ")}</div>
                            </div>
                            {move || {
                                let can_manage = can();
                                if can_manage {
                                    let id_clone = id.clone();
                                    view! {
                                        <button class="btn btn-danger" on:click=move |_| {
                                            let id_clone = id_clone.clone();
                                            spawn_local(async move {
                                                match developer::delete_key(&id_clone).await {
                                                    Ok(()) => keys.update(|v| v.retain(|x| x.id != id_clone)),
                                                    Err(e) => error.set(Some(e.to_string())),
                                                }
                                            });
                                        }>
                                            "Revoke"
                                        </button>
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }
                            }}
                        </div>
                    }
                }).collect_view()}
            </section>
            {move || if can() {
                view! {
                    <section class="card">
                        <h2>"Create key"</h2>
                        <form on:submit=create class="stack-form">
                            <label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label>
                            <fieldset>
                                <legend>"Scopes"</legend>
                                {developer::API_KEY_SCOPES.iter().map(|scope| {
                                    let s = (*scope).to_string();
                                    view! {
                                        <label class="check-row">
                                            <input type="checkbox" on:change=move|e| {
                                                let checked = event_target::<web_sys::HtmlInputElement>(&e).checked();
                                                scopes.update(|v| if checked {
                                                    if !v.contains(&s) { v.push(s.clone()) }
                                                } else {
                                                    v.retain(|x| x != &s)
                                                });
                                            }/>
                                            {*scope}
                                        </label>
                                    }
                                }).collect_view()}
                            </fieldset>
                            <button class="btn btn-primary" disabled=move||busy.get()>"Create key"</button>
                        </form>
                    </section>
                }.into_any()
            } else {
                ().into_any()
            }}
            <p class="muted">"The server enforces scope permissions. Secrets are never stored in browser storage."</p>
        </div>
    }
}
