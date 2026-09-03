use crate::api::rum;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn RumSitesPage() -> impl IntoView {
    let sites = RwSignal::new(Vec::<rum::RumSite>::new());
    let name = RwSignal::new(String::new());
    let domain = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let reload = move || {
        spawn_local(async move {
            match rum::list_rum_sites_typed().await {
                Ok(v) => sites.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    };
    reload();
    let create = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let req = rum::CreateRumSitePayload {
            name: name.get(),
            domain: domain.get(),
        };
        spawn_local(async move {
            match rum::create_rum_site_typed(&req).await {
                Ok(v) => {
                    sites.update(|s| s.push(v));
                    name.set(String::new());
                    domain.set(String::new());
                    notice.set(Some("RUM site created.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"RUM sites"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<div class="feature-grid"><section class="card"><h2>"Create site"</h2><form on:submit=create class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Domain"<input placeholder="example.com" prop:value=domain on:input=move|e|domain.set(event_target_value(&e)) required/></label><button class="btn btn-primary">"Create"</button></form></section><section class="card"><h2>"Sites"</h2>{move||sites.get().into_iter().map(|s|view!{<article class="list-row"><div><strong>{s.name}</strong><div class="muted">{s.domain}</div></div><code>{s.site_key}</code></article>}).collect_view()}</section></div><p class="muted">"Site keys are public ingestion identifiers, not authentication secrets."</p></div>}
}
