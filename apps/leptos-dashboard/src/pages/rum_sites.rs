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
    let loading = RwSignal::new(true);
    let busy = RwSignal::new(false);

    let reload = move || {
        loading.set(true);
        spawn_local(async move {
            match rum::list_rum_sites_typed().await {
                Ok(v) => {
                    sites.set(v);
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        })
    };
    reload();

    let create = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        error.set(None);
        let n = name.get().trim().to_string();
        let d = domain.get().trim().to_string();
        if n.is_empty() || d.is_empty() {
            error.set(Some("Site name and domain are required.".into()));
            return;
        }
        busy.set(true);
        let req = rum::CreateRumSitePayload { name: n, domain: d };
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
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Real User Monitoring"</span><h1>"RUM sites"</h1></div><a class="btn" href="/rum">"Analytics"</a></header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            <div class="feature-grid">
                <section class="card">
                    <h2>"Create site"</h2>
                    <p class="muted">"Use the site domain that will send browser telemetry. Validation remains server-side."</p>
                    <form on:submit=create class="stack-form">
                        <label>"Name"<input maxlength="100" prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label>
                        <label>"Domain"<input placeholder="example.com" autocomplete="off" maxlength="253" prop:value=domain on:input=move|e|domain.set(event_target_value(&e)) required/></label>
                        <button class="btn btn-primary" disabled=move || busy.get()>{move || if busy.get() {"Creating…"} else {"Create site"}}</button>
                    </form>
                </section>
                <section class="card">
                    <div class="section-heading"><h2>"Sites"</h2><span class="muted">"Public ingestion identifiers"</span></div>
                    {move || if loading.get() {
                        view! { <div class="loading">"Loading sites…"</div> }.into_any()
                    } else if sites.get().is_empty() {
                        view! { <div class="empty-state"><h3>"No RUM sites"</h3><p>"Create your first site to start collecting analytics."</p></div> }.into_any()
                    } else {
                        view! { <div class="list-stack">{move || sites.get().into_iter().map(|s| view! {
                            <article class="list-row">
                                <div><strong>{s.name}</strong><div class="muted">{s.domain}</div></div>
                                <div><span class="eyebrow">"Site key"</span><code>{s.site_key}</code></div>
                            </article>
                        }).collect_view()}</div> }.into_any()
                    }}
                </section>
            </div>
            <p class="muted">"Site keys identify an ingestion target; they are not API secrets. Keep privileged API keys out of browser code."</p>
        </div>
    }
}
