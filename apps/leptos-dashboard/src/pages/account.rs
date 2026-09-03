use crate::api::account;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn AccountPage() -> impl IntoView {
    let profile = RwSignal::new(None::<account::AccountProfile>);
    let error = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        spawn_local(async move {
            match account::fetch_account_profile().await {
                Ok(v) => profile.set(Some(v)),
                Err(e) => error.set(Some(e)),
            }
        })
    });
    view! {<div class="feature-page"><header class="page-header"><h1>"Account"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||profile.get().map(|p|view!{<section class="card"><dl class="details"><dt>"Username"</dt><dd>{p.username}</dd><dt>"Email"</dt><dd>{p.email}</dd><dt>"Email verified"</dt><dd>{if p.email_verified{"Yes"}else{"No"}}</dd><dt>"Role"</dt><dd>{format!("{:?}",p.role)}</dd><dt>"Tenant"</dt><dd><code>{p.tenant_id}</code></dd></dl></section>})}<p class="muted">"Profile editing is intentionally unavailable until the backend exposes a dedicated account-update contract."</p></div>}
}
