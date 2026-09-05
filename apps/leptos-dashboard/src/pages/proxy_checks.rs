use crate::api::proxy_checks;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn ProxyChecksPage() -> impl IntoView {
    let input = RwSignal::new(String::new());
    let job = RwSignal::new(None::<proxy_checks::ProxyJob>);
    let results = RwSignal::new(Vec::<proxy_checks::ProxyResult>::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let proxies = input
            .get()
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    return None;
                }
                let (host, port) = line.rsplit_once(':')?;
                Some(proxy_checks::ProxyInput {
                    host: host.trim_matches(['[', ']']).to_string(),
                    port: port.parse().ok()?,
                    scheme: "http".into(),
                    username: None,
                    password: None,
                })
            })
            .collect::<Vec<_>>();
        if proxies.is_empty() {
            error.set(Some("Enter at least one host:port proxy.".into()));
            return;
        }
        if proxies.len() > 500 {
            error.set(Some("Maximum 500 proxies per job.".into()));
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match proxy_checks::create(&proxy_checks::CreateProxyCheckRequest { proxies }).await {
                Ok(v) => job.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let refresh = move |_| {
        let Some(current) = job.get() else {
            return;
        };
        let id = current.id;
        spawn_local(async move {
            match proxy_checks::get(&id).await {
                Ok(v) => job.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
            match proxy_checks::results(&id, 50, 0, false).await {
                Ok(v) => results.set(v.items),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let cancel = move |_| {
        let Some(current) = job.get() else {
            return;
        };
        let id = current.id;
        spawn_local(async move {
            match proxy_checks::cancel(&id).await {
                Ok(v) => job.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div><span class="eyebrow">"TOOLS"</span><h1>"Proxy checks"</h1>
                <p class="muted">"Check up to 500 public proxies with strict SSRF protection."</p></div>
            </header>
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            <section class="card">
                <form on:submit=submit class="stack-form">
                    <label>"Proxy list"<textarea rows="10" maxlength="100000" prop:value=input placeholder="1.2.3.4:8080" on:input=move |e| input.set(event_target_value(&e))></textarea></label>
                    <button class="btn btn-primary" type="submit" disabled=move || busy.get()>{move || if busy.get() {"Checking…"} else {"Start proxy check"}}</button>
                </form>
            </section>
            {move || job.get().map(|j| {
                view! {
                    <section class="card">
                        <div class="section-heading"><h2>"Job status"</h2>
                            <div>
                                <button class="btn" on:click=refresh>"Refresh"</button>
                                {if j.status == "pending" || j.status == "running" { view! { <button class="btn btn-danger" on:click=cancel>"Cancel"</button> }.into_any() } else { ().into_any() }}
                            </div>
                        </div>
                        <div class="metric-grid">
                            <div><strong>{j.status.clone()}</strong><span>"Status"</span></div>
                            <div><strong>{format!("{}/{}", j.checked_count, j.total_count)}</strong><span>"Checked"</span></div>
                            <div><strong>{j.successful_count}</strong><span>"Working"</span></div>
                            <div><strong>{j.failed_count}</strong><span>"Failed"</span></div>
                        </div>
                    </section>
                }
            })}
            {move || if results.get().is_empty() { ().into_any() } else {
                view! { <section class="card"><div class="section-heading"><h2>"Results"</h2></div>
                    <div class="list-stack">
                        {move || results.get().into_iter().map(|r| {
                            let body = r.result;
                            view! { <div class="list-row">
                                <div><strong>{format!("{}://{}:{}", r.scheme, r.host, r.port)}</strong>
                                {body.map(|b| view! { <div class="muted">{format!("{} · {} ms", if b.reachable {"Working"} else {"Failed"}, b.latency_ms.unwrap_or(0))}{b.error_code.map(|c| format!(" · {c}")).unwrap_or_default()}</div> })}</div>
                            </div> }
                        }).collect_view()}
                    </div>
                </section> }.into_any()
            }}
        </div>
    }
}
