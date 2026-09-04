#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="${1:-.}"
cd "$ROOT"

STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP=".phase5-backup-${STAMP}"
mkdir -p "$BACKUP"
echo "=== PingVertex Phase 5 frontend hardening ==="
echo "Project: $(pwd)"
echo "Backup: $BACKUP"

mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/rum_dashboard.rs")"
cp -f "apps/leptos-dashboard/src/pages/rum_dashboard.rs" "$BACKUP/apps/leptos-dashboard/src/pages/rum_dashboard.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/rum_sites.rs")"
cp -f "apps/leptos-dashboard/src/pages/rum_sites.rs" "$BACKUP/apps/leptos-dashboard/src/pages/rum_sites.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/status_pages.rs")"
cp -f "apps/leptos-dashboard/src/pages/status_pages.rs" "$BACKUP/apps/leptos-dashboard/src/pages/status_pages.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/team.rs")"
cp -f "apps/leptos-dashboard/src/pages/team.rs" "$BACKUP/apps/leptos-dashboard/src/pages/team.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/invitations.rs")"
cp -f "apps/leptos-dashboard/src/pages/invitations.rs" "$BACKUP/apps/leptos-dashboard/src/pages/invitations.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/api_keys.rs")"
cp -f "apps/leptos-dashboard/src/pages/api_keys.rs" "$BACKUP/apps/leptos-dashboard/src/pages/api_keys.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/api_flows.rs")"
cp -f "apps/leptos-dashboard/src/pages/api_flows.rs" "$BACKUP/apps/leptos-dashboard/src/pages/api_flows.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/billing.rs")"
cp -f "apps/leptos-dashboard/src/pages/billing.rs" "$BACKUP/apps/leptos-dashboard/src/pages/billing.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/add_ons.rs")"
cp -f "apps/leptos-dashboard/src/pages/add_ons.rs" "$BACKUP/apps/leptos-dashboard/src/pages/add_ons.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/pages/notifications.rs")"
cp -f "apps/leptos-dashboard/src/pages/notifications.rs" "$BACKUP/apps/leptos-dashboard/src/pages/notifications.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/src/api/developer.rs")"
cp -f "apps/leptos-dashboard/src/api/developer.rs" "$BACKUP/apps/leptos-dashboard/src/api/developer.rs"
mkdir -p "$BACKUP/$(dirname "apps/leptos-dashboard/style.css")"
cp -f "apps/leptos-dashboard/style.css" "$BACKUP/apps/leptos-dashboard/style.css"
echo "[PASS] Existing Phase 5 target files backed up."

cat > "apps/leptos-dashboard/src/pages/rum_dashboard.rs" <<'EOF'
use crate::api::rum;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn metric(value: Option<f64>, decimals: usize) -> String {
    value
        .map(|v| format!("{v:.prec$}", prec = decimals))
        .unwrap_or_else(|| "—".into())
}

#[component]
pub fn RumDashboardPage() -> impl IntoView {
    let sites = RwSignal::new(Vec::<rum::RumSite>::new());
    let selected = RwSignal::new(None::<String>);
    let range = RwSignal::new("1".to_string());

    let overview = RwSignal::new(None::<rum::BackendRumOverview>);
    let pages = RwSignal::new(Vec::<rum::BackendPagePerformanceRow>::new());
    let errors = RwSignal::new(None::<rum::BackendRumErrorBreakdown>);
    let browsers = RwSignal::new(None::<rum::BackendBrowserDeviceBreakdown>);
    let geo = RwSignal::new(Vec::<rum::BackendGeoBreakdownRow>::new());
    let trend = RwSignal::new(Vec::<rum::BackendPerformanceTrendPoint>::new());
    let sessions = RwSignal::new(None::<rum::BackendSessionStats>);

    let error = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(async move {
            match rum::list_rum_sites_typed().await {
                Ok(value) => {
                    if selected.get_untracked().is_none() {
                        selected.set(value.first().map(|site| site.id.clone()));
                    }
                    sites.set(value);
                }
                Err(err) => {
                    error.set(Some(err.to_string()));
                }
            }
        });
    });

    let reload = move || {
        let Some(site_id) = selected.get() else {
            return;
        };

        let days = match range.get().as_str() {
            "1" => 1.0,
            "7" => 7.0,
            "30" => 30.0,
            _ => {
                error.set(Some("Invalid analytics range.".into()));
                return;
            }
        };
        loading.set(true);

        let now = js_sys::Date::new_0();
        let end = now.to_iso_string().as_string().unwrap_or_default();

        let start_ms = now.get_time() - (days * 24.0 * 60.0 * 60.0 * 1000.0);

        let start = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(start_ms))
            .to_iso_string()
            .as_string()
            .unwrap_or_default();

        spawn_local(async move {
            error.set(None);
            let mut failures = Vec::new();

            match rum::rum_overview_typed(&site_id, Some(&start), Some(&end)).await {
                Ok(value) => overview.set(Some(value)),
                Err(e) => failures.push(format!("Overview: {e}")),
            }
            match rum::rum_slow_pages_typed(&site_id, Some(&start), Some(&end), Some(10)).await {
                Ok(value) => pages.set(value),
                Err(e) => failures.push(format!("Pages: {e}")),
            }
            match rum::rum_error_breakdown_typed(&site_id, Some(&start), Some(&end), Some(10)).await {
                Ok(value) => errors.set(Some(value)),
                Err(e) => failures.push(format!("Errors: {e}")),
            }
            match rum::rum_browser_device_breakdown_typed(&site_id, Some(&start), Some(&end)).await {
                Ok(value) => browsers.set(Some(value)),
                Err(e) => failures.push(format!("Browser/device: {e}")),
            }
            match rum::rum_geo_breakdown_typed(&site_id, Some(&start), Some(&end), Some(10)).await {
                Ok(value) => geo.set(value),
                Err(e) => failures.push(format!("Geography: {e}")),
            }
            match rum::rum_performance_trend_typed(&site_id, Some(&start), Some(&end)).await {
                Ok(value) => trend.set(value),
                Err(e) => failures.push(format!("Trend: {e}")),
            }
            match rum::rum_session_stats_typed(&site_id, Some(&start), Some(&end)).await {
                Ok(value) => sessions.set(Some(value)),
                Err(e) => failures.push(format!("Sessions: {e}")),
            }

            if !failures.is_empty() {
                error.set(Some(format!("Some RUM panels could not be loaded: {}", failures.join(" · "))));
            }
            loading.set(false);
        });
    };

    Effect::new(move |_| {
        let _ = selected.get();
        let _ = range.get();
        reload();
    });

    view! {
        <div class="feature-page">

            <header class="page-header">
                <h1>"Real User Monitoring"</h1>

                <div class="page-header-actions">
                    <a class="btn" href="/rum/sites">
                        "Manage sites"
                    </a>
                </div>
            </header>

            <div class="filters-bar">
                <label>
                    "Site"

                    <select
                        prop:value=move || selected.get().unwrap_or_default()
                        on:change=move |event| {
                            let value = event_target_value(&event);

                            selected.set(
                                if value.is_empty() {
                                    None
                                } else {
                                    Some(value)
                                }
                            );
                        }
                    >
                        <option value="">"Select site"</option>

                        {move || {
                            sites
                                .get()
                                .into_iter()
                                .map(|site| {
                                    view! {
                                        <option value={site.id}>
                                            {site.name}
                                        </option>
                                    }
                                })
                                .collect_view()
                        }}
                    </select>
                </label>

                <label>
                    "Range"

                    <select
                        prop:value=range
                        on:change=move |event| {
                            range.set(event_target_value(&event));
                        }
                    >
                        <option value="1">"24 hours"</option>
                        <option value="7">"7 days"</option>
                        <option value="30">"30 days"</option>
                    </select>
                </label>
            </div>

            {move || {
                error.get().map(|message| {
                    view! {
                        <div class="error">
                            {message}
                        </div>
                    }
                })
            }}

            {move || {
                if loading.get() {
                    view! {
                        <div class="loading">
                            "Loading analytics..."
                        </div>
                    }
                    .into_any()
                } else if selected.get().is_none() {
                    view! {
                        <div class="empty-state">
                            <h2>"No RUM site selected"</h2>
                            <p>"Create a site to start collecting analytics."</p>
                        </div>
                    }
                    .into_any()
                } else {
                    view! {
                        <div>

                            <section class="overview-cards">
                                {move || {
                                    overview.get().map(|data| {
                                        view! {
                                            <div class="card">
                                                <span class="card-label">
                                                    "Page views"
                                                </span>
                                                <span class="card-value">
                                                    {data.page_view_count}
                                                </span>
                                            </div>

                                            <div class="card">
                                                <span class="card-label">
                                                    "Sessions"
                                                </span>
                                                <span class="card-value">
                                                    {data.session_count}
                                                </span>
                                            </div>

                                            <div class="card">
                                                <span class="card-label">
                                                    "Avg load"
                                                </span>
                                                <span class="card-value">
                                                    {metric(data.avg_load_time_ms, 0)}
                                                    " ms"
                                                </span>
                                            </div>

                                            <div class="card">
                                                <span class="card-label">
                                                    "P95 load"
                                                </span>
                                                <span class="card-value">
                                                    {metric(data.p95_load_time_ms, 0)}
                                                    " ms"
                                                </span>
                                            </div>

                                            <div class="card">
                                                <span class="card-label">
                                                    "P75 LCP"
                                                </span>
                                                <span class="card-value">
                                                    {metric(data.p75_lcp_ms, 0)}
                                                    " ms"
                                                </span>
                                            </div>

                                            <div class="card">
                                                <span class="card-label">
                                                    "P75 INP"
                                                </span>
                                                <span class="card-value">
                                                    {metric(data.p75_inp_ms, 0)}
                                                    " ms"
                                                </span>
                                            </div>

                                            <div class="card">
                                                <span class="card-label">
                                                    "P75 CLS"
                                                </span>
                                                <span class="card-value">
                                                    {metric(data.p75_cls, 3)}
                                                </span>
                                            </div>

                                            <div class="card">
                                                <span class="card-label">
                                                    "Error rate"
                                                </span>
                                                <span class="card-value">
                                                    {data.error_rate
                                                        .map(|value| {
                                                            format!("{:.2}%", value * 100.0)
                                                        })
                                                        .unwrap_or_else(|| "—".into())}
                                                </span>
                                            </div>
                                        }
                                    })
                                }}
                            </section>

                            <section class="card">
                                <h2>"Performance trend"</h2>

                                {move || {
                                    trend
                                        .get()
                                        .into_iter()
                                        .map(|point| {
                                            view! {
                                                <div class="list-row">
                                                    <span>
                                                        {point.bucket_start}
                                                    </span>

                                                    <span>
                                                        {point.page_view_count}
                                                        " views · "
                                                        {metric(point.avg_load_time_ms, 0)}
                                                        " ms"
                                                    </span>
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </section>

                            <section class="card">
                                <div class="section-heading">
                                    <h2>"Performance trend"</h2>
                                    <span class="muted">"Average load time"</span>
                                </div>
                                {move || {
                                    let points = trend.get();
                                    let max = points.iter().filter_map(|p| p.avg_load_time_ms).fold(0.0_f64, f64::max);
                                    if points.is_empty() {
                                        view! { <p class="muted">"No trend data for this range."</p> }.into_any()
                                    } else {
                                        view! { <div class="performance-bars">
                                            {points.into_iter().map(|point| {
                                                let value = point.avg_load_time_ms.unwrap_or(0.0);
                                                let width = if max > 0.0 { ((value / max) * 100.0).clamp(2.0, 100.0) } else { 2.0 };
                                                view! { <div class="performance-row">
                                                    <span class="performance-index">{point.bucket_start}</span>
                                                    <div class="performance-track"><div class="performance-fill" style=format!("width:{width:.1}%")></div></div>
                                                    <span class="performance-value">{format!("{value:.0} ms")}</span>
                                                </div> }
                                            }).collect_view()}
                                        </div> }.into_any()
                                    }
                                }}
                            </section>

                            <section class="card">
                                <h2>"Slow pages"</h2>

                                {move || {
                                    pages
                                        .get()
                                        .into_iter()
                                        .map(|page| {
                                            view! {
                                                <div class="list-row">
                                                    <span>
                                                        <code>
                                                            {page.path}
                                                        </code>
                                                    </span>

                                                    <span>
                                                        {page.view_count}
                                                        " views · "
                                                        {metric(page.avg_load_time_ms, 0)}
                                                        " ms avg"
                                                    </span>
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </section>

                            <div class="feature-grid">

                                <section class="card">
                                    <h2>"JavaScript errors"</h2>

                                    {move || {
                                        errors.get().map(|data| {
                                            view! {
                                                <p>
                                                    {data.js_error_count}
                                                    " JS errors"
                                                </p>

                                                {data
                                                    .top_js_errors
                                                    .into_iter()
                                                    .map(|item| {
                                                        view! {
                                                            <div class="list-row">
                                                                <span>
                                                                    {item.message}
                                                                </span>

                                                                <strong>
                                                                    {item.occurrence_count}
                                                                </strong>
                                                            </div>
                                                        }
                                                    })
                                                    .collect_view()}
                                            }
                                        })
                                    }}
                                </section>

                                <section class="card">
                                    <h2>"Browsers & devices"</h2>

                                    {move || {
                                        browsers.get().map(|data| {
                                            view! {
                                                <div>
                                                    {data
                                                        .browsers
                                                        .into_iter()
                                                        .map(|item| {
                                                            view! {
                                                                <div class="list-row">
                                                                    <span>
                                                                        {item
                                                                            .browser_name
                                                                            .unwrap_or_else(
                                                                                || "Unknown".into()
                                                                            )}
                                                                    </span>

                                                                    <strong>
                                                                        {item.session_count}
                                                                    </strong>
                                                                </div>
                                                            }
                                                        })
                                                        .collect_view()}
                                                </div>
                                            }
                                        })
                                    }}
                                </section>

                            </div>

                            <div class="feature-grid">

                                <section class="card">
                                    <h2>"Geography"</h2>

                                    {move || {
                                        geo
                                            .get()
                                            .into_iter()
                                            .map(|item| {
                                                view! {
                                                    <div class="list-row">
                                                        <span>
                                                            {item
                                                                .country_code
                                                                .unwrap_or_else(
                                                                    || "Unknown".into()
                                                                )}
                                                        </span>

                                                        <strong>
                                                            {item.session_count}
                                                        </strong>
                                                    </div>
                                                }
                                            })
                                            .collect_view()
                                    }}
                                </section>

                                <section class="card">
                                    <h2>"Sessions"</h2>

                                    {move || {
                                        sessions.get().map(|data| {
                                            view! {
                                                <div>
                                                    <p>
                                                        {data.session_count}
                                                        " sessions"
                                                    </p>

                                                    <p>
                                                        {data
                                                            .avg_page_views_per_session
                                                            .map(|v| format!("{v:.2}"))
                                                            .unwrap_or_else(
                                                                || "—".into()
                                                            )}
                                                        " avg page views/session"
                                                    </p>

                                                    <p>
                                                        {data
                                                            .avg_session_duration_secs
                                                            .map(|v| format!("{v:.0} s"))
                                                            .unwrap_or_else(
                                                                || "—".into()
                                                            )}
                                                        " avg duration"
                                                    </p>
                                                </div>
                                            }
                                        })
                                    }}
                                </section>

                            </div>

                        </div>
                    }
                    .into_any()
                }
            }}

        </div>
    }
}
EOF

cat > "apps/leptos-dashboard/src/pages/rum_sites.rs" <<'EOF'
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
EOF

cat > "apps/leptos-dashboard/src/pages/status_pages.rs" <<'EOF'
use crate::api::{monitoring, status_pages as api};
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;

#[component]
pub fn StatusPagesPage() -> impl IntoView {
    let pages = RwSignal::new(Vec::<api::StatusPage>::new());
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let selected = RwSignal::new(None::<String>);
    let pending_delete = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    let reload = move || {
        spawn_local(async move {
            let p = api::list().await;
            let m = api::monitors().await;
            match (p, m) {
                (Ok(p), Ok(m)) => {
                    pages.set(p);
                    monitors.set(m);
                    error.set(None);
                }
                (Err(e), _) | (_, Err(e)) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };
    reload();

    view! {
        <div class="feature-page">
            <header class="page-header"><h1>"Status pages"</h1></header>
            {move || notice.get().map(|v| view!{<div class="success">{v}</div>})}
            {move || error.get().map(|v| view!{<div class="error">{v}</div>})}
            {move || {
                let nav = navigate.clone();
                if loading.get() {
                    view!{<div class="loading">"Loading status pages…"</div>}.into_any()
                } else {
                    view! {
                        <div class="feature-grid">
                            <section class="card"><h2>"Create status page"</h2><StatusPageForm monitors=monitors on_created=Callback::new(move |p| { pages.update(|v| v.push(p)); notice.set(Some("Status page created.".into())); }) /></section>
                            <section class="card"><h2>"Your status pages"</h2>
                                {move || pages.get().into_iter().map(|p| {
                                    let id=p.id.clone();
                                    let id2=id.clone();
                                    let _id3=id.clone();
                                    let _nav = nav.clone();
                                    view!{
                                        <article class="list-row">
                                            <div>
                                                <strong>{p.name.clone()}</strong>
                                                <div class="muted">{"/status/"}{p.subdomain.clone()}</div>
                                                <div class="muted">{format!("{} monitor(s)", p.monitor_ids.len())}</div>
                                            </div>
                                            <div class="row-actions">
                                                <a class="btn" href={format!("/status/{}", p.subdomain)}>"Preview"</a>
                                                <button class="btn" on:click=move |_| {selected.set(Some(id.clone()));}>"Manage"</button>
                                                <button class="btn btn-danger" on:click=move |_| pending_delete.set(Some(id2.clone()))>"Delete"</button>
                                            </div>
                                        </article>
                                    }
                                }).collect_view()}
                                {move || if pages.get().is_empty() { view!{<p class="muted">"No status pages yet."</p>}.into_any() } else { ().into_any() }}
                            </section>
                        </div>
                        {move || selected.get().and_then(|id| pages.get().into_iter().find(|p: &api::StatusPage| p.id == id)).map(|p| view!{<StatusPageEditor page=p monitors=monitors on_saved=Callback::new(move |p: api::StatusPage| { pages.update(|v| { if let Some(old)=v.iter_mut().find(|x| x.id==p.id) {*old=p;} }); notice.set(Some("Status page updated.".into())); }) on_close=Callback::new(move |_| selected.set(None)) />})}
                    }.into_any()
                }
            }}
            {move || pending_delete.get().map(|id| {
                let id2=id.clone();
                view!{
                    <ConfirmDialog
                        title="Delete status page"
                        body="This permanently removes the status page."
                        on_confirm=Callback::new(move |_| {
                            let id=id2.clone();
                            spawn_local(async move {
                                match api::delete(&id).await {
                                    Ok(())=>{
                                        pages.update(|v|v.retain(|p|p.id!=id));
                                        notice.set(Some("Status page deleted.".into()));
                                    },
                                    Err(e)=>error.set(Some(e.to_string()))
                                }
                                pending_delete.set(None);
                            });
                        })
                        on_cancel=Callback::new(move |_| pending_delete.set(None))
                    />
                }
            })}
        </div>
    }
}

#[component]
fn StatusPageForm(
    monitors: RwSignal<Vec<monitoring::Monitor>>,
    on_created: Callback<api::StatusPage>,
) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let subdomain = RwSignal::new(String::new());
    let selected = RwSignal::new(Vec::<String>::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let req = api::CreateStatusPageRequest {
            name: name.get(),
            subdomain: subdomain.get(),
            monitor_ids: selected.get(),
        };
        busy.set(true);
        spawn_local(async move {
            match api::create(&req).await {
                Ok(p) => {
                    on_created.run(p);
                    name.set(String::new());
                    subdomain.set(String::new());
                    selected.set(Vec::new());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    view! {<form on:submit=submit class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Subdomain"<input placeholder="status.example" prop:value=subdomain on:input=move|e|subdomain.set(event_target_value(&e)) required/></label><fieldset><legend>"Monitors"</legend>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<label class="check-row"><input type="checkbox" on:change=move|e|{let checked=event_target::<web_sys::HtmlInputElement>(&e).checked();selected.update(|v|if checked{if !v.contains(&id){v.push(id.clone())}}else{v.retain(|x|x!=&id)});}/>{m.name}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get() type="submit">"Create"</button>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</form>}
}

#[component]
fn StatusPageEditor(
    page: api::StatusPage,
    monitors: RwSignal<Vec<monitoring::Monitor>>,
    on_saved: Callback<api::StatusPage>,
    on_close: Callback<()>,
) -> impl IntoView {
    let domain_initial = page.custom_domain.clone();
    let domain_verified = page.custom_domain_verified;
    let name = RwSignal::new(page.name.clone());
    let color = RwSignal::new(page.primary_color.clone().unwrap_or_default());
    let selected = RwSignal::new(page.monitor_ids.clone());
    let domain = RwSignal::new(domain_initial.clone().unwrap_or_default());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = page.id.clone();
    let logo_url = page.logo_url.clone();
    let save_callback = on_saved;
    let domain_callback = on_saved;
    let remove_callback = on_saved;
    let upload_callback = on_saved;

    let save = {
        let id = id.clone();
        move |ev: leptos::ev::SubmitEvent| {
            ev.prevent_default();
            let id = id.clone();
            let req = api::UpdateStatusPageRequest {
                name: name.get(),
                monitor_ids: selected.get(),
                logo_url: logo_url.clone(),
                primary_color: if color.get().trim().is_empty() {
                    None
                } else {
                    Some(color.get())
                },
            };
            busy.set(true);
            spawn_local(async move {
                match api::update(&id, &req).await {
                    Ok(p) => save_callback.run(p),
                    Err(e) => error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    };

    let request_domain = {
        let id2 = id.clone();
        move || {
            let d = domain.get();
            if d.trim().is_empty() {
                return;
            }
            let id2 = id2.clone();
            busy.set(true);
            spawn_local(async move {
                match api::request_custom_domain(&id2, &d).await {
                    Ok(p) => domain_callback.run(p),
                    Err(e) => error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    };

    let remove_domain = {
        let id3 = id.clone();
        move || {
            let id3 = id3.clone();
            busy.set(true);
            spawn_local(async move {
                match api::remove_custom_domain(&id3).await {
                    Ok(p) => remove_callback.run(p),
                    Err(e) => error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    };

    let upload_id = page.id.clone();
    let upload = move |ev: leptos::ev::Event| {
        let input = event_target::<HtmlInputElement>(&ev);
        if let Some(file) = input.files().and_then(|f| f.get(0)) {
            let file_name = file.name();
            let mime = file.type_();
            let id = upload_id.clone();
            busy.set(true);
            spawn_local(async move {
                match JsFuture::from(file.array_buffer()).await {
                    Ok(buf) => {
                        let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                        match api::upload_logo(&id, bytes, &file_name, &mime).await {
                            Ok(p) => upload_callback.run(p),
                            Err(e) => error.set(Some(e.to_string())),
                        }
                    }
                    Err(_) => error.set(Some("Could not read the selected image.".into())),
                }
                busy.set(false);
            });
        }
    };

    view! {<section class="card feature-editor"><header class="page-header"><h2>"Manage status page"</h2><button class="btn" on:click=move |_|on_close.run(())>"Close"</button></header><form on:submit=save class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Primary color"<input type="text" placeholder="#111827" prop:value=color on:input=move|e|color.set(event_target_value(&e))/></label><fieldset><legend>"Monitors"</legend>{move||monitors.get().into_iter().map(|m|{let mid=m.id.clone();let checked=selected.get().contains(&mid);view!{<label class="check-row"><input type="checkbox" prop:checked=checked on:change=move|e|{let c=event_target::<HtmlInputElement>(&e).checked();selected.update(|v|if c{if !v.contains(&mid){v.push(mid.clone())}}else{v.retain(|x|x!=&mid)});}/>{m.name}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get() type="submit">"Save"</button></form><div class="subsection"><h3>"Logo"</h3><input type="file" accept="image/*" on:change=upload/><p class="muted">"The server validates the image and enforces its upload limit."</p></div><div class="subsection"><h3>"Custom domain"</h3><p>{move||if domain_verified{"Verified".to_string()}else if page.custom_domain.is_some(){"Pending verification".to_string()}else{"Not configured".to_string()}}</p><div class="inline-form"><input placeholder="status.example.com" prop:value=domain on:input=move|e|domain.set(event_target_value(&e))/><button class="btn" type="button" disabled=move||busy.get() on:click=move|_|request_domain()>"Request"</button><button class="btn btn-danger" type="button" disabled=move||busy.get() on:click=move|_|remove_domain()>"Remove"</button></div></div>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</section>}
}
EOF

cat > "apps/leptos-dashboard/src/pages/team.rs" <<'EOF'
use crate::api::team;
use crate::auth::context::{use_auth, AuthState, Role};
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role: &Role) -> bool {
    matches!(role, Role::Admin | Role::Owner)
}

#[component]
pub fn TeamPage() -> impl IntoView {
    let auth = use_auth();
    let members = RwSignal::new(Vec::<team::Member>::new());
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let loading = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let pending_remove = RwSignal::new(None::<String>);
    let new_user = RwSignal::new(String::new());
    let new_role = RwSignal::new("Member".to_string());

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_manage(&u.role),
        _ => false,
    };

    let reload = move || {
        loading.set(true);
        spawn_local(async move {
            match team::list_members().await {
                Ok(v) => {
                    members.set(v);
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };
    reload();

    let add = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let uid = new_user.get();
        let role = new_role.get();
        if uid.trim().is_empty() {
            error.set(Some("User ID is required.".into()));
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match team::add_member(&team::AddMemberRequest { user_id: uid, role }).await {
                Ok(v) => {
                    members.update(|m| m.push(v));
                    new_user.set(String::new());
                    notice.set(Some("Member added.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"Access control"</span>
                    <h1>"Team members"</h1>
                </div>
                <a class="btn" href="/invitations">"Invitations"</a>
            </header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            <section class="card">
                <div class="section-heading">
                    <h2>"Members"</h2>
                    <span class="muted">"Roles are enforced by the server."</span>
                </div>
                {move || if loading.get() {
                    view! { <div class="loading">"Loading members…"</div> }.into_any()
                } else if members.get().is_empty() {
                    view! { <div class="empty-state"><h3>"No members"</h3><p>"Add a member or send an invitation to get started."</p></div> }.into_any()
                } else {
                    view! {
                        <div class="table-scroll">
                            <table>
                                <thead><tr><th>"User"</th><th>"Role"</th><th>"Joined"</th><th></th></tr></thead>
                                <tbody>
                                    {move || members.get().into_iter().map(|m| {
                                        let id = m.user_id.clone();
                                        let initial_role = m.role.clone();
                                        view! {
                                            <tr>
                                                <td><code>{m.user_id}</code></td>
                                                <td>
                                                    <select
                                                        disabled=move || !can() || busy.get()
                                                        prop:value=initial_role
                                                        on:change={
                                                            let id = id.clone();
                                                            move |e| {
                                                                let role = event_target_value(&e);
                                                                let id = id.clone();
                                                                busy.set(true);
                                                                spawn_local(async move {
                                                                    match team::change_role(&id, &role).await {
                                                                        Ok(()) => notice.set(Some("Role updated.".into())),
                                                                        Err(e) => error.set(Some(e.to_string())),
                                                                    }
                                                                    busy.set(false);
                                                                });
                                                            }
                                                        }
                                                    >
                                                        <option value="Member">"Member"</option>
                                                        <option value="Admin">"Admin"</option>
                                                        <option value="Owner">"Owner"</option>
                                                        <option value="Auditor">"Auditor"</option>
                                                        <option value="Viewer">"Viewer"</option>
                                                    </select>
                                                </td>
                                                <td>{m.joined_at.unwrap_or_else(|| "Pending".into())}</td>
                                                <td>
                                                    {move || if can() {
                                                        let remove_id = id.clone();
                                                        view! { <button class="btn btn-danger" disabled=move || busy.get() on:click=move |_| pending_remove.set(Some(remove_id.clone()))>"Remove"</button> }.into_any()
                                                    } else { ().into_any() }}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>
            {move || if can() {
                view! {
                    <section class="card">
                        <h2>"Add existing user"</h2>
                        <p class="muted">"Use an existing user UUID. The API remains the authorization boundary."</p>
                        <form on:submit=add class="inline-form">
                            <input placeholder="User UUID" autocomplete="off" prop:value=new_user on:input=move |e| new_user.set(event_target_value(&e)) required/>
                            <select prop:value=new_role on:change=move |e| new_role.set(event_target_value(&e))>
                                <option value="Member">"Member"</option>
                                <option value="Admin">"Admin"</option>
                                <option value="Auditor">"Auditor"</option>
                                <option value="Viewer">"Viewer"</option>
                            </select>
                            <button class="btn btn-primary" disabled=move || busy.get()>"Add"</button>
                        </form>
                    </section>
                }.into_any()
            } else {
                ().into_any()
            }}
            {move || pending_remove.get().map(|id| {
                let confirm_id = id.clone();
                view! {
                    <ConfirmDialog
                        title="Remove team member"
                        body="This removes the member from the tenant. This action cannot be undone here."
                        on_confirm=Callback::new(move |_| {
                            let id = confirm_id.clone();
                            busy.set(true);
                            spawn_local(async move {
                                match team::remove_member(&id).await {
                                    Ok(()) => {
                                        members.update(|v| v.retain(|x| x.user_id != id));
                                        notice.set(Some("Member removed.".into()));
                                    }
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                pending_remove.set(None);
                                busy.set(false);
                            });
                        })
                        on_cancel=Callback::new(move |_| pending_remove.set(None))
                    />
                }
            })}
        </div>
    }
}
EOF

cat > "apps/leptos-dashboard/src/pages/invitations.rs" <<'EOF'
use crate::api::team;
use crate::auth::context::{use_auth, AuthState, Role};
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role: &Role) -> bool {
    matches!(role, Role::Admin | Role::Owner)
}

#[component]
pub fn InvitationsPage() -> impl IntoView {
    let auth = use_auth();
    let invitations = RwSignal::new(Vec::<team::Invitation>::new());
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let email = RwSignal::new(String::new());
    let role = RwSignal::new("Member".into());
    let token = RwSignal::new(String::new());
    let created_token = RwSignal::new(None::<String>);
    let pending_revoke = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_manage(&u.role),
        _ => false,
    };

    let reload = move || {
        spawn_local(async move {
            match team::list_invitations().await {
                Ok(v) => invitations.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    };
    reload();

    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let email_value = email.get().trim().to_string();
        if email_value.is_empty() {
            error.set(Some("Email is required.".into()));
            return;
        }
        let req = team::CreateInvitationRequest { email: email_value, role: role.get() };
        busy.set(true);
        spawn_local(async move {
            match team::create_invitation(&req).await {
                Ok(v) => {
                    created_token.set(Some(v.invitation_token));
                    email.set(String::new());
                    notice.set(Some("Invitation created. Save the token now; it is returned only once.".into()));
                    reload();
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let accept = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let t = token.get().trim().to_string();
        if t.is_empty() {
            error.set(Some("Invitation token is required.".into()));
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match team::accept_invitation(&t).await {
                Ok(_) => {
                    token.set(String::new());
                    notice.set(Some("Invitation accepted.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Access control"</span><h1>"Invitations"</h1></div><a class="btn" href="/team">"Team"</a></header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || created_token.get().map(|t| view! {
                <div class="secret-box">
                    <strong>"Invitation token — save it now"</strong>
                    <code>{t}</code>
                    <span class="muted">"This token is shown only in this session. Do not share it publicly."</span>
                    <button class="btn" type="button" on:click=move |_| created_token.set(None)>"Hide token"</button>
                </div>
            })}
            <div class="feature-grid">
                {move || if can() {
                    view! {
                        <section class="card">
                            <h2>"Create invitation"</h2>
                            <form on:submit=create class="stack-form">
                                <label>"Email"<input type="email" autocomplete="email" maxlength="254" prop:value=email on:input=move |e| email.set(event_target_value(&e)) required/></label>
                                <label>"Role"<select prop:value=role on:change=move |e| role.set(event_target_value(&e))>
                                    <option value="Member">"Member"</option><option value="Admin">"Admin"</option><option value="Auditor">"Auditor"</option><option value="Viewer">"Viewer"</option>
                                </select></label>
                                <button class="btn btn-primary" disabled=move || busy.get()>{move || if busy.get() {"Creating…"} else {"Create invitation"}}</button>
                            </form>
                        </section>
                    }.into_any()
                } else {
                    view! { <section class="card"><h2>"Invitation management"</h2><p class="muted">"You do not have permission to create or revoke tenant invitations."</p></section> }.into_any()
                }}
                <section class="card">
                    <div class="section-heading"><h2>"Pending"</h2><span class="muted">"Expired invitations are handled by the server."</span></div>
                    {move || if invitations.get().is_empty() {
                        view! { <div class="empty-state"><p>"No pending invitations."</p></div> }.into_any()
                    } else {
                        view! { <div class="list-stack">{move || invitations.get().into_iter().map(|i| {
                            let id = i.id.clone();
                            view! { <div class="list-row"><div><strong>{i.email}</strong><div class="muted">{i.role} · expires {i.expires_at}</div></div>
                                {move || if can() {
                                    let revoke_id = id.clone();
                                    view! { <button class="btn btn-danger" disabled=move || busy.get() on:click=move |_| pending_revoke.set(Some(revoke_id.clone()))>"Revoke"</button> }.into_any()
                                } else { ().into_any() }}
                            </div> }
                        }).collect_view()}</div> }.into_any()
                    }}
                </section>
            </div>
            <section class="card">
                <h2>"Accept invitation"</h2>
                <p class="muted">"Paste a valid invitation token. The server validates ownership, expiry and tenant membership."</p>
                <form on:submit=accept class="inline-form">
                    <input type="password" autocomplete="off" placeholder="Invitation token" prop:value=token on:input=move |e| token.set(event_target_value(&e)) required/>
                    <button class="btn" disabled=move || busy.get()>"Accept"</button>
                </form>
            </section>
            {move || pending_revoke.get().map(|id| {
                let confirm_id = id.clone();
                view! {
                    <ConfirmDialog
                        title="Revoke invitation"
                        body="The invitation token will stop working immediately."
                        on_confirm=Callback::new(move |_| {
                            let id = confirm_id.clone();
                            busy.set(true);
                            spawn_local(async move {
                                match team::revoke_invitation(&id).await {
                                    Ok(()) => {
                                        invitations.update(|v| v.retain(|x| x.id != id));
                                        notice.set(Some("Invitation revoked.".into()));
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
EOF

cat > "apps/leptos-dashboard/src/pages/api_keys.rs" <<'EOF'
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
EOF

cat > "apps/leptos-dashboard/src/pages/api_flows.rs" <<'EOF'
use crate::api::{developer, monitoring};
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone)]
struct StepDraft {
    name: String,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    extract: Vec<developer::ExtractionRequest>,
    assertions: Vec<developer::AssertionRequest>,
}

fn flow_steps_to_drafts(steps: &[developer::ApiFlowStep]) -> Vec<StepDraft> {
    steps.iter().map(|s| StepDraft {
        name: s.name.clone(),
        method: s.method.clone(),
        url: s.url_template.clone(),
        headers: s.headers.clone(),
        body: s.body.clone(),
        extract: s.extract.iter().map(|x| match x {
            developer::Extraction::StatusCode => developer::ExtractionRequest::StatusCode,
            developer::Extraction::Header { name } => developer::ExtractionRequest::Header { name: name.clone() },
            developer::Extraction::JsonPath { path } => developer::ExtractionRequest::JsonPath { path: path.clone() },
            developer::Extraction::Body => developer::ExtractionRequest::Body,
        }).collect(),
        assertions: s.assertions.iter().map(|x| developer::AssertionRequest {
            response_part: x.response_part.clone(),
            operator: x.operator.clone(),
            target: x.target.clone(),
            expected_value: x.expected_value.clone(),
        }).collect(),
    }).collect()
}

#[component]
pub fn ApiFlowsPage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let selected = RwSignal::new(None::<String>);
    let flows = RwSignal::new(Vec::<developer::ApiFlow>::new());
    let steps = RwSignal::new(Vec::<StepDraft>::new());
    let step_name = RwSignal::new(String::new());
    let step_url = RwSignal::new(String::new());
    let step_method = RwSignal::new("GET".into());
    let editing = RwSignal::new(None::<String>);
    let pending_delete = RwSignal::new(None::<(String, String)>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(async move {
            match monitoring::fetch_monitors().await {
                Ok(v) => monitors.set(v),
                Err(e) => error.set(Some(e)),
            }
        })
    });
    Effect::new(move |_| {
        if let Some(id) = selected.get() {
            let id = id.clone();
            spawn_local(async move {
                match developer::list_flows(&id).await {
                    Ok(v) => flows.set(v),
                    Err(e) => error.set(Some(e.to_string())),
                }
            })
        } else {
            flows.set(Vec::new());
        }
    });

    let add = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let n = step_name.get().trim().to_string();
        let u = step_url.get().trim().to_string();
        if n.is_empty() || u.is_empty() {
            error.set(Some("Step name and URL are required.".into()));
            return;
        }
        steps.update(|v| v.push(StepDraft {
            name: n,
            method: step_method.get(),
            url: u,
            headers: Vec::new(),
            body: None,
            extract: Vec::new(),
            assertions: Vec::new(),
        }));
        step_name.set(String::new());
        step_url.set(String::new());
    };

    let save = move |ev: leptos::ev::MouseEvent| {
        ev.prevent_default();
        let Some(mid) = selected.get() else { return };
        if steps.get().is_empty() {
            error.set(Some("Add at least one step.".into()));
            return;
        }
        let payload = developer::CreateApiFlowRequest {
            steps: steps.get().iter().enumerate().map(|(i, s)| developer::ApiFlowStepRequest {
                order: (i + 1) as u16,
                name: s.name.clone(),
                method: s.method.clone(),
                url_template: s.url.clone(),
                headers: s.headers.clone(),
                body: s.body.clone(),
                extract: s.extract.clone(),
                assertions: s.assertions.clone(),
            }).collect(),
        };
        let mid2 = mid.clone();
        busy.set(true);
        spawn_local(async move {
            let result = match editing.get_untracked() {
                Some(flow_id) => developer::update_flow(&mid2, &flow_id, &payload).await,
                None => developer::create_flow(&mid2, &payload).await,
            };
            match result {
                Ok(_) => {
                    editing.set(None);
                    steps.set(Vec::new());
                    notice.set(Some("API flow saved.".into()));
                    match developer::list_flows(&mid2).await {
                        Ok(v) => flows.set(v),
                        Err(e) => error.set(Some(e.to_string())),
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Developer"</span><h1>"API flows"</h1></div></header>
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            <section class="card">
                <label>"Monitor"
                    <select prop:value=move || selected.get().unwrap_or_default() on:change=move |e| {
                        let v = event_target_value(&e);
                        selected.set((!v.is_empty()).then_some(v));
                        steps.set(Vec::new());
                        editing.set(None);
                    }>
                        <option value="">"Select a monitor"</option>
                        {move || monitors.get().into_iter().map(|m| view! { <option value={m.id}>{m.name}</option> }).collect_view()}
                    </select>
                </label>
            </section>
            {move || selected.get().map(|_| view! {
                <section class="card">
                    <div class="section-heading"><h2>{move || if editing.get().is_some() {"Edit flow"} else {"Build flow"}}</h2><span class="muted">"Only backend-supported fields are sent."</span></div>
                    <form on:submit=add class="inline-form">
                        <input placeholder="Step name" maxlength="100" prop:value=step_name on:input=move |e| step_name.set(event_target_value(&e)) required/>
                        <select prop:value=step_method on:change=move |e| step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select>
                        <input type="url" placeholder="https://example.com/api" prop:value=step_url on:input=move |e| step_url.set(event_target_value(&e)) required/>
                        <button class="btn">"Add step"</button>
                    </form>
                    {move || if steps.get().is_empty() {
                        view! { <p class="muted">"No steps yet."</p> }.into_any()
                    } else {
                        view! { <ol>{move || steps.get().into_iter().enumerate().map(|(i,s)| view! {
                            <li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_| steps.update(|v| { if i < v.len() { v.remove(i); } })>"Remove"</button></li>
                        }).collect_view()}</ol> }.into_any()
                    }}
                    <div class="form-actions">
                        {move || editing.get().is_some().then(|| view! { <button class="btn" type="button" on:click=move |_| { editing.set(None); steps.set(Vec::new()); }>"Cancel edit"</button> })}
                        <button class="btn btn-primary" type="button" on:click=save disabled=move || busy.get() || steps.get().is_empty()>{move || if busy.get() {"Saving…"} else if editing.get().is_some() {"Save flow"} else {"Create flow"}}</button>
                    </div>
                </section>
            })}
            <section class="card">
                <div class="section-heading"><h2>"Existing flows"</h2><span class="muted">"Delete is permanent."</span></div>
                {move || if flows.get().is_empty() {
                    view! { <p class="muted">"No API flows for this monitor."</p> }.into_any()
                } else {
                    view! { <div class="list-stack">{move || flows.get().into_iter().map(|f| {
                        let id = f.id.clone();
                        let monitor_id = f.monitor_id.clone();
                        let loaded = flow_steps_to_drafts(&f.steps);
                        view! { <article class="list-row">
                            <div><strong>{format!("Flow {}",f.id)}</strong><div class="muted">{format!("{} step(s)",f.steps.len())}</div></div>
                            <div class="row-actions">
                                <button class="btn" on:click=move |_| { editing.set(Some(id.clone())); steps.set(loaded.clone()); }>"Edit"</button>
                                <button class="btn btn-danger" on:click=move |_| pending_delete.set(Some((monitor_id.clone(), id.clone())))>"Delete"</button>
                            </div>
                        </article> }
                    }).collect_view()}</div> }.into_any()
                }}
            </section>
            {move || pending_delete.get().map(|(mid, fid)| {
                let mid2 = mid.clone();
                let fid2 = fid.clone();
                view! { <ConfirmDialog
                    title="Delete API flow"
                    body="This permanently removes the flow and its steps."
                    on_confirm=Callback::new(move |_| {
                        let mid = mid2.clone(); let fid = fid2.clone();
                        busy.set(true);
                        spawn_local(async move {
                            match developer::delete_flow(&mid, &fid).await {
                                Ok(()) => {
                                    flows.update(|v| v.retain(|x| x.id != fid));
                                    notice.set(Some("API flow deleted.".into()));
                                }
                                Err(e) => error.set(Some(e.to_string())),
                            }
                            pending_delete.set(None);
                            busy.set(false);
                        });
                    })
                    on_cancel=Callback::new(move |_| pending_delete.set(None))
                /> }
            })}
        </div>
    }
}
EOF

cat > "apps/leptos-dashboard/src/pages/billing.rs" <<'EOF'
use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_bill(role: &Role) -> bool { matches!(role, Role::Owner) }

#[component]
pub fn BillingPage() -> impl IntoView {
    let auth = use_auth();
    let summary = RwSignal::new(None::<billing::BillingSummary>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let plan = RwSignal::new("tier_1".to_string());
    let session = RwSignal::new(String::new());
    let checkout_status = RwSignal::new(None::<String>);
    let quote = RwSignal::new(None::<billing::Quote>);
    let monitors = RwSignal::new("1".to_string());
    let interval = RwSignal::new("60".to_string());
    let retention = RwSignal::new("30".to_string());
    let quote_busy = RwSignal::new(false);
    let checkout_busy = RwSignal::new(false);
    let verify_busy = RwSignal::new(false);

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_bill(&u.role),
        _ => false,
    };

    Effect::new(move |_| {
        spawn_local(async move {
            match billing::summary().await {
                Ok(v) => summary.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });

    let get_quote = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let monitors_value = match monitors.get().trim().parse::<u64>() {
            Ok(v) if v >= 1 => v,
            _ => { error.set(Some("Monitors must be at least 1.".into())); return; }
        };
        let interval_value = match interval.get().trim().parse::<u32>() {
            Ok(v) if v >= 1 => v,
            _ => { error.set(Some("Interval must be at least 1 second.".into())); return; }
        };
        let retention_value = match retention.get().trim().parse::<u32>() {
            Ok(v) if v >= 1 => v,
            _ => { error.set(Some("Retention must be at least 1 day.".into())); return; }
        };
        quote_busy.set(true);
        let req = billing::QuoteRequest { monitors: monitors_value, interval_seconds: interval_value, retention_days: retention_value };
        spawn_local(async move {
            match billing::quote(&req).await {
                Ok(v) => quote.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
            quote_busy.set(false);
        });
    };

    let checkout = move |_| {
        if checkout_busy.get() { return; }
        error.set(None);
        let p = plan.get();
        checkout_busy.set(true);
        spawn_local(async move {
            match billing::checkout(&p).await {
                Ok(v) => {
                    notice.set(Some("Checkout created. Continue securely in the payment provider.".into()));
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href(&v.checkout_url);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            checkout_busy.set(false);
        });
    };

    let verify = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let id = session.get().trim().to_string();
        if id.is_empty() { error.set(Some("Checkout session ID is required.".into())); return; }
        verify_busy.set(true);
        spawn_local(async move {
            match billing::verify_checkout(&id).await {
                Ok(v) => checkout_status.set(Some(v.status)),
                Err(e) => error.set(Some(e.to_string())),
            }
            verify_busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Subscription"</span><h1>"Billing"</h1></div></header>
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}

            <section class="card">
                <div class="section-heading"><h2>"Current billing"</h2><span class="muted">"Pricing is authoritative on the server."</span></div>
                {move || match summary.get() {
                    Some(s) => view! { <div class="overview-cards">
                        <div class="card"><span class="card-label">"Base"</span><span class="card-value">{format!("{} {}",s.base_price,s.currency)}</span></div>
                        <div class="card"><span class="card-label">"Add-ons"</span><span class="card-value">{format!("{} {}",s.add_ons_total,s.currency)}</span></div>
                        <div class="card"><span class="card-label">"Total"</span><span class="card-value">{format!("{} {}",s.total,s.currency)}</span></div>
                    </div> }.into_any(),
                    None => view! { <div class="loading">"Loading billing…"</div> }.into_any(),
                }}
            </section>

            <section class="card">
                <h2>"Pricing quote"</h2>
                <p class="muted">"Quotes are calculated by the backend; changing these values never charges your account."</p>
                <form on:submit=get_quote class="inline-form">
                    <label>"Monitors"<input type="number" min="1" prop:value=monitors on:input=move|e|monitors.set(event_target_value(&e))/></label>
                    <label>"Interval (s)"<input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e))/></label>
                    <label>"Retention (days)"<input type="number" min="1" prop:value=retention on:input=move|e|retention.set(event_target_value(&e))/></label>
                    <button class="btn" disabled=move || quote_busy.get()>{move || if quote_busy.get() {"Calculating…"} else {"Get quote"}}</button>
                </form>
                {move || quote.get().map(|q| view! { <div class="quote-box"><strong>{format!("{} {} / month",q.monthly_price,q.currency)}</strong><span>{q.interval_label}</span><span>{q.retention_label}</span></div> })}
            </section>

            <section class="card">
                <h2>"Verify checkout"</h2>
                <p class="muted">"Verification asks the backend for the authoritative payment state."</p>
                <form on:submit=verify class="inline-form">
                    <input type="password" autocomplete="off" placeholder="Checkout session ID" prop:value=session on:input=move|e|session.set(event_target_value(&e)) required/>
                    <button class="btn" disabled=move || verify_busy.get()>{move || if verify_busy.get() {"Verifying…"} else {"Verify"}}</button>
                </form>
                {move || checkout_status.get().map(|v| view! { <p><strong>{v}</strong></p> })}
            </section>

            {move || if can() {
                view! { <section class="card">
                    <h2>"Change plan"</h2>
                    <p class="muted">"You will be redirected to the payment provider before any charge is finalized."</p>
                    <div class="inline-form">
                        <select prop:value=plan on:change=move|e|plan.set(event_target_value(&e))>
                            <option value="free">"Free"</option><option value="tier_1">"Tier 1"</option><option value="tier_5">"Tier 5"</option><option value="tier_10">"Tier 10"</option><option value="tier_20">"Tier 20"</option>
                        </select>
                        <button class="btn btn-primary" disabled=move || checkout_busy.get() on:click=checkout>{move || if checkout_busy.get() {"Starting checkout…"} else {"Continue to checkout"}}</button>
                    </div>
                </section> }.into_any()
            } else {
                view! { <section class="card"><p class="muted">"Only the tenant owner can change the subscription."</p></section> }.into_any()
            }}
        </div>
    }
}
EOF

cat > "apps/leptos-dashboard/src/pages/add_ons.rs" <<'EOF'
use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn AddOnsPage() -> impl IntoView {
    let auth = use_auth();
    let kind = RwSignal::new("enterprise".into());
    let qty = RwSignal::new(String::from("1"));
    let country = RwSignal::new(String::new());
    let result = RwSignal::new(None::<billing::AddOn>);
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => matches!(u.role, Role::Owner),
        _ => false,
    };
    let add = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let quantity = match qty.get().trim().parse::<u64>() {
            Ok(v) if v >= 1 => v,
            _ => {
                error.set(Some(
                    "Quantity must be a valid number of at least 1.".into(),
                ));
                return;
            }
        };
        error.set(None);
        busy.set(true);
        let req = billing::AddOnRequest {
            addon_type: kind.get(),
            quantity,
            country_code: if country.get().is_empty() {
                None
            } else {
                Some(country.get())
            },
            tier: None,
        };
        spawn_local(async move {
            match billing::add_on(&req).await {
                Ok(v) => result.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Add-ons"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||result.get().map(|r|view!{<div class="success">{format!("Created {} × {} for {} {}",r.quantity,r.addon_type,r.total_price,r.currency)}</div>})}{move||if can(){view!{<section class="card"><form on:submit=add class="stack-form"><label>"Type"<select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))><option value="enterprise">"Enterprise SLA"</option><option value="sms">"SMS credits"</option></select></label><label>"Quantity"<input type="number" min="1" prop:value=qty on:input=move|e|qty.set(event_target_value(&e))/></label>{move||if kind.get()=="sms"{view!{<label>"Country code"<input placeholder="NG" prop:value=country on:input=move|e|country.set(event_target_value(&e)) required/></label>}.into_any()}else{().into_any()}}<button class="btn btn-primary" disabled=move || busy.get()>{move || if busy.get() {"Creating…"} else {"Create add-on"}}</button></form></section>}.into_any()}else{view!{<p class="muted">"Only the tenant owner can manage add-ons."</p>}.into_any()}}</div>}
}
EOF

cat > "apps/leptos-dashboard/src/pages/notifications.rs" <<'EOF'
use crate::api::monitoring;
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn NotificationsPage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let channels = RwSignal::new(Vec::<monitoring::NotificationChannel>::new());
    let selected = RwSignal::new(String::new());
    let kind = RwSignal::new("email".into());
    let target = RwSignal::new(String::new());
    let chat_id = RwSignal::new(String::new());
    let secret = RwSignal::new(None::<String>);
    let pending_delete = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(async move {
            match monitoring::fetch_monitors().await {
                Ok(v) => monitors.set(v),
                Err(e) => error.set(Some(e)),
            }
        })
    });
    Effect::new(move |_| {
        let id = selected.get();
        if id.is_empty() {
            channels.set(Vec::new());
            return;
        }
        spawn_local(async move {
            match monitoring::list_notification_channels(&id).await {
                Ok(v) => channels.set(v),
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let add = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        error.set(None);
        let id = selected.get();
        let target_value = target.get().trim().to_string();
        if id.is_empty() || target_value.is_empty() {
            error.set(Some("Select a monitor and provide a target.".into()));
            return;
        }
        let kind_value = kind.get();
        let req = monitoring::ChannelPayload {
            kind: kind_value.clone(),
            target: target_value,
            chat_id: if kind_value == "telegram" { Some(chat_id.get().trim().to_string()) } else { None },
        };
        busy.set(true);
        spawn_local(async move {
            match monitoring::create_notification_channel(&id, &req).await {
                Ok(v) => {
                    secret.set(v.signing_secret);
                    channels.update(|c| c.push(v.channel));
                    target.set(String::new());
                    chat_id.set(String::new());
                    notice.set(Some("Notification channel created. Copy the signing secret now if you need it for webhook verification.".into()));
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Alerting"</span><h1>"Notifications"</h1></div></header>
            <p class="muted">"Channels are configured per monitor. Secrets are returned only at creation time."</p>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || secret.get().map(|s| view! {
                <div class="secret-box"><strong>"Signing secret — save it now"</strong><code>{s}</code><span class="muted">"It is not stored in this UI after you hide it."</span><button class="btn" type="button" on:click=move |_| secret.set(None)>"Hide secret"</button></div>
            })}
            <section class="card">
                <label>"Monitor"<select prop:value=selected on:change=move|e|{selected.set(event_target_value(&e));secret.set(None);} >
                    <option value="">"Select monitor"</option>
                    {move || monitors.get().into_iter().map(|m| view! { <option value={m.id}>{m.name}</option> }).collect_view()}
                </select></label>
                {move || if selected.get().is_empty() {
                    view! { <p class="muted">"Choose a monitor to configure its channels."</p> }.into_any()
                } else {
                    view! { <>
                        <form on:submit=add class="inline-form">
                            <select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))>
                                <option value="email">"Email"</option><option value="webhook">"Webhook"</option><option value="telegram">"Telegram"</option>
                            </select>
                            {move || if kind.get()=="telegram" {
                                view! { <input placeholder="Telegram chat ID" autocomplete="off" prop:value=chat_id on:input=move|e|chat_id.set(event_target_value(&e)) required/> }.into_any()
                            } else { ().into_any() }}
                            <input placeholder="Target" autocomplete="off" prop:value=target on:input=move|e|target.set(event_target_value(&e)) required/>
                            <button class="btn btn-primary" disabled=move || busy.get()>{move || if busy.get() {"Adding…"} else {"Add channel"}}</button>
                        </form>
                        {move || if channels.get().is_empty() {
                            view! { <p class="muted">"No channels configured for this monitor."</p> }.into_any()
                        } else {
                            view! { <div class="list-stack">{move || channels.get().into_iter().map(|c| {
                                let id=c.id.clone();
                                view! { <div class="list-row"><div><strong>{format!("{}",c.kind)}</strong><div class="muted">{c.target}</div></div><div class="row-actions"><span class="muted">{if c.enabled {"Enabled"} else {"Disabled"}}</span><button class="btn btn-danger" disabled=move || busy.get() on:click=move |_| pending_delete.set(Some(id.clone()))>"Delete"</button></div></div> }
                            }).collect_view()}</div> }.into_any()
                        }}
                    </> }.into_any()
                }}
            </section>
            {move || pending_delete.get().map(|id| {
                let confirm_id=id.clone();
                view! { <ConfirmDialog
                    title="Delete notification channel"
                    body="This stops notifications through this channel. The action is permanent."
                    on_confirm=Callback::new(move |_| {
                        let id=confirm_id.clone();
                        busy.set(true);
                        spawn_local(async move {
                            match monitoring::delete_notification_channel(&id).await {
                                Ok(()) => { channels.update(|v|v.retain(|x|x.id!=id)); notice.set(Some("Notification channel deleted.".into())); }
                                Err(e) => error.set(Some(e)),
                            }
                            pending_delete.set(None); busy.set(false);
                        });
                    })
                    on_cancel=Callback::new(move |_| pending_delete.set(None))
                /> }
            })}
        </div>
    }
}
EOF

cat > "apps/leptos-dashboard/src/api/developer.rs" <<'EOF'
use crate::api::client::{self, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreatedApiKey {
    pub id: String,
    pub name: String,
    pub key: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiFlow {
    pub id: String,
    pub monitor_id: String,
    pub steps: Vec<ApiFlowStep>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ApiFlowStep {
    pub order: u16,
    pub name: String,
    pub method: String,
    pub url_template: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub extract: Vec<Extraction>,
    pub assertions: Vec<Assertion>,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Extraction {
    StatusCode,
    Header { name: String },
    JsonPath { path: String },
    Body,
}
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Assertion {
    pub id: String,
    pub response_part: String,
    pub operator: String,
    pub target: Option<String>,
    pub expected_value: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CreateApiFlowRequest {
    pub steps: Vec<ApiFlowStepRequest>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ApiFlowStepRequest {
    pub order: u16,
    pub name: String,
    pub method: String,
    pub url_template: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub extract: Vec<ExtractionRequest>,
    pub assertions: Vec<AssertionRequest>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtractionRequest {
    StatusCode,
    Header { name: String },
    JsonPath { path: String },
    Body,
}
#[derive(Debug, Clone, Serialize)]
pub struct AssertionRequest {
    pub response_part: String,
    pub operator: String,
    pub target: Option<String>,
    pub expected_value: Option<String>,
}

pub const API_KEY_SCOPES: &[&str] = &[
    "monitor:read",
    "monitor:write",
    "status_page:read",
    "status_page:write",
    "incident:read",
    "incident:write",
    "notification:read",
    "notification:write",
    "api_flow:read",
    "api_flow:write",
];

pub async fn list_keys() -> Result<Vec<ApiKey>, ApiError> {
    client::get_json("/api-keys").await
}
pub async fn create_key(request: &CreateApiKeyRequest) -> Result<CreatedApiKey, ApiError> {
    client::post_json("/api-keys", request).await
}
pub async fn delete_key(id: &str) -> Result<(), ApiError> {
    client::delete_json_no_content(&format!("/api-keys/{id}")).await
}
pub async fn list_flows(monitor_id: &str) -> Result<Vec<ApiFlow>, ApiError> {
    client::get_json(&format!("/monitors/{monitor_id}/flows")).await
}
pub async fn create_flow(
    monitor_id: &str,
    request: &CreateApiFlowRequest,
) -> Result<ApiFlow, ApiError> {
    client::post_json(&format!("/monitors/{monitor_id}/flows"), request).await
}
pub async fn update_flow(
    monitor_id: &str,
    flow_id: &str,
    request: &CreateApiFlowRequest,
) -> Result<ApiFlow, ApiError> {
    client::put_json(&format!("/monitors/{monitor_id}/flows/{flow_id}"), request).await
}


pub async fn delete_flow(monitor_id: &str, flow_id: &str) -> Result<(), ApiError> {
    client::delete_json_no_content(&format!("/monitors/{monitor_id}/flows/{flow_id}")).await
}
EOF

cat > "apps/leptos-dashboard/style.css" <<'EOF'
/* ==========================================================================
   PingVertex — design tokens
   Precision/signal aesthetic for an uptime & RUM monitoring dashboard:
   grotesk sans for UI text, monospace for data/metrics, status color used
   only where it carries meaning (up/down/degraded).
   ========================================================================== */

:root {
  --font-sans: 'Inter', ui-sans-serif, system-ui, -apple-system, sans-serif;
  --font-mono: 'IBM Plex Mono', ui-monospace, SFMono-Regular, Menlo, monospace;

  --bg: #f6f7f9;
  --surface: #ffffff;
  --surface-sunken: #f0f2f5;
  --border: #e1e4ea;
  --border-strong: #c7ccd6;
  --ink: #12151c;
  --ink-muted: #5b6472;
  --ink-faint: #8991a0;

  --accent: #4338ca;
  --accent-hover: #372fb0;
  --accent-soft: #eef0fe;

  --up: #12855a;
  --up-soft: #e5f6ef;
  --down: #c2311f;
  --down-soft: #fbeae8;
  --degraded: #9a6300;
  --degraded-soft: #fbf1dd;
  --neutral: #6b7280;
  --neutral-soft: #eef0f3;

  --radius-sm: 4px;
  --radius-md: 6px;
  --shadow-card: 0 1px 2px rgba(18, 21, 28, 0.05);

  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 32px;
  --space-7: 48px;
}

/* ---------- Reset & base ---------- */

* { margin: 0; padding: 0; box-sizing: border-box; }

html { -webkit-text-size-adjust: 100%; }

body {
  font-family: var(--font-sans);
  background-color: var(--bg);
  color: var(--ink);
  font-size: 15px;
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
}

h1, h2, h3 { font-weight: 600; letter-spacing: -0.01em; color: var(--ink); }
h1 { font-size: 1.625rem; }
h2 { font-size: 1.0625rem; margin-bottom: var(--space-3); }

a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }

p { color: var(--ink-muted); }

:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: 0.001ms !important; transition-duration: 0.001ms !important; }
}

/* ---------- Buttons & form controls (generic, so undecorated <button> tags
   across the app still look designed) ---------- */

button, .btn {
  font-family: var(--font-sans);
  font-size: 0.875rem;
  font-weight: 500;
  background-color: var(--surface);
  color: var(--ink);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  padding: 0.5rem 0.875rem;
  cursor: pointer;
  transition: background-color 0.12s ease, border-color 0.12s ease;
}

button:hover, .btn:hover { background-color: var(--surface-sunken); }
button:disabled { opacity: 0.5; cursor: not-allowed; }

.btn-primary {
  background-color: var(--accent);
  color: #fff;
  border-color: var(--accent);
}
.btn-primary:hover { background-color: var(--accent-hover); border-color: var(--accent-hover); }

.btn-danger { color: var(--down); border-color: var(--down); background: var(--down-soft); }
.btn-danger:hover { background-color: var(--down); color: #fff; }

.btn-block { width: 100%; padding: 0.625rem 1rem; }

input, select, textarea {
  font-family: var(--font-sans);
  font-size: 0.9375rem;
  color: var(--ink);
  background-color: var(--surface);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  padding: 0.5rem 0.625rem;
  width: 100%;
}

input:focus, select:focus, textarea:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

label { display: block; font-size: 0.8125rem; font-weight: 500; color: var(--ink-muted); margin-bottom: var(--space-1); }

/* ---------- App shell ---------- */

.app-shell { display: flex; flex-direction: column; min-height: 100vh; }

.app-header {
  background-color: var(--ink);
  color: #fff;
  padding: 0 var(--space-5);
  height: 56px;
  display: flex;
  align-items: center;
  gap: var(--space-6);
}

.app-brand a { color: #fff; font-weight: 700; letter-spacing: -0.01em; }

.app-nav { display: flex; gap: var(--space-4); flex: 1; }
.app-nav a { color: rgba(255, 255, 255, 0.72); font-size: 0.875rem; font-weight: 500; }
.app-nav a:hover, .app-nav a.active { color: #fff; text-decoration: none; }

.app-user { display: flex; align-items: center; gap: var(--space-3); font-size: 0.8125rem; color: rgba(255, 255, 255, 0.72); font-family: var(--font-mono); }
.app-user a { color: rgba(255, 255, 255, 0.85); font-family: var(--font-sans); }

.app-main { flex: 1; padding: var(--space-6) var(--space-5); max-width: 1180px; width: 100%; margin: 0 auto; }

.app-footer {
  background-color: var(--surface);
  border-top: 1px solid var(--border);
  color: var(--ink-faint);
  text-align: center;
  padding: var(--space-4);
  font-size: 0.8125rem;
}

/* ---------- Page header (title + primary action) ---------- */

.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  margin-bottom: var(--space-5);
  flex-wrap: wrap;
}
.page-header h1 { margin: 0; }

/* ---------- Stat cards / overview grid ---------- */

.overview-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-card);
  padding: var(--space-4);
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.card-label {
  font-size: 0.75rem;
  font-weight: 500;
  color: var(--ink-muted);
}

.card-value {
  font-family: var(--font-mono);
  font-size: 1.75rem;
  font-weight: 600;
  color: var(--ink);
  font-variant-numeric: tabular-nums;
}

.card-value.up { color: var(--up); }
.card-value.down { color: var(--down); }

/* ---------- Tables ---------- */

table {
  width: 100%;
  border-collapse: collapse;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  font-size: 0.875rem;
}

.monitors-table, table { margin-bottom: var(--space-5); }

th {
  text-align: left;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--ink-muted);
  background: var(--surface-sunken);
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
}

td {
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
  color: var(--ink);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

td:first-child, th:first-child { font-family: var(--font-sans); }

tr:last-child td { border-bottom: none; }
tbody tr:hover td { background-color: var(--surface-sunken); }

td button, td .btn { padding: 0.3rem 0.625rem; font-size: 0.8125rem; margin-right: var(--space-2); }

.table-responsive { overflow-x: auto; border-radius: var(--radius-md); }
.table-responsive table { border-radius: 0; border: none; }
.data-section { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); padding: var(--space-4); margin-bottom: var(--space-5); box-shadow: var(--shadow-card); }
.data-section.two-col { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-5); }

/* ---------- Loading / error / empty states ---------- */

.loading, .auth-loading {
  color: var(--ink-muted);
  font-size: 0.875rem;
  padding: var(--space-5) 0;
}

.error, .form-error {
  background-color: var(--down-soft);
  border: 1px solid rgba(194, 49, 31, 0.25);
  color: var(--down);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-sm);
  font-size: 0.875rem;
  margin: var(--space-3) 0;
}

.auth-success { color: var(--up); background: var(--up-soft); border: 1px solid rgba(18, 133, 90, 0.25); padding: var(--space-3) var(--space-4); border-radius: var(--radius-sm); }

.error-boundary {
  background-color: var(--down-soft);
  border: 1px solid rgba(194, 49, 31, 0.25);
  color: var(--down);
  padding: var(--space-4);
  border-radius: var(--radius-md);
  margin: var(--space-4) 0;
}

.loading-container { display: flex; align-items: center; gap: var(--space-3); color: var(--ink-muted); font-size: 0.875rem; }

.loading-spinner {
  display: inline-block;
  border: 2.5px solid var(--border);
  border-radius: 50%;
  border-top-color: var(--accent);
  width: 18px;
  height: 18px;
  animation: spin 0.8s linear infinite;
  flex-shrink: 0;
}
@keyframes spin { to { transform: rotate(360deg); } }

.skeleton { display: flex; flex-direction: column; gap: var(--space-2); }
.skeleton-line { height: 12px; border-radius: var(--radius-sm); background: linear-gradient(90deg, var(--surface-sunken) 25%, var(--border) 37%, var(--surface-sunken) 63%); background-size: 400% 100%; animation: shimmer 1.4s ease infinite; width: 100%; }
.skeleton-line.short { width: 40%; }
@keyframes shimmer { 0% { background-position: 100% 50%; } 100% { background-position: 0 50%; } }

/* ---------- Auth pages ---------- */

.auth-shell {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-5);
  background-color: var(--ink);
  background-image: radial-gradient(rgba(255, 255, 255, 0.06) 1px, transparent 1px);
  background-size: 22px 22px;
}

.auth-card {
  width: 100%;
  max-width: 380px;
  background: var(--surface);
  border-radius: var(--radius-md);
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.28);
  padding: var(--space-6) var(--space-5);
}

.auth-card-center { text-align: center; }

.auth-brand { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-5); }
.auth-brand-name { font-weight: 700; letter-spacing: -0.01em; color: var(--ink); }

.auth-brand-mark {
  width: 20px; height: 20px;
  border-radius: 50%;
  background: var(--accent-soft);
  display: flex; align-items: center; justify-content: center;
}
.pulse-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); position: relative; }
.pulse-dot::after {
  content: "";
  position: absolute; inset: 0;
  border-radius: 50%;
  background: var(--accent);
  animation: pulse 2s ease-out infinite;
}
@keyframes pulse {
  0% { transform: scale(1); opacity: 0.6; }
  100% { transform: scale(2.6); opacity: 0; }
}

.auth-subtitle { font-size: 0.875rem; margin-bottom: var(--space-5); }

.auth-form { display: flex; flex-direction: column; gap: var(--space-4); }
.field { display: flex; flex-direction: column; }
.auth-hint { font-size: 0.875rem; color: var(--ink-muted); }

.auth-links { margin-top: var(--space-5); font-size: 0.8125rem; color: var(--ink-muted); text-align: center; }
.auth-links-sep { margin: 0 var(--space-2); color: var(--ink-faint); }

/* ---------- Monitor detail lists ---------- */

.monitor-info { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: var(--space-3) var(--space-5); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); padding: var(--space-4); margin-bottom: var(--space-5); box-shadow: var(--shadow-card); }
.monitor-info p { color: var(--ink); font-size: 0.875rem; }
.monitor-info strong { color: var(--ink-muted); font-weight: 500; }

.checks-list, .incidents-list, .channels-list { display: flex; flex-direction: column; gap: var(--space-2); margin-bottom: var(--space-5); }

.check-item, .incident-item, .channel-item {
  display: flex;
  gap: var(--space-4);
  align-items: center;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-3) var(--space-4);
  font-size: 0.8125rem;
  font-family: var(--font-mono);
}

.channel-item { justify-content: space-between; font-family: var(--font-sans); }
.channel-item button { margin: 0; }

.channel-form { display: flex; gap: var(--space-3); flex-wrap: wrap; align-items: flex-end; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); padding: var(--space-4); }
.channel-form input, .channel-form select { width: auto; flex: 1; min-width: 160px; }

/* ---------- RUM dashboard ---------- */

.filters-bar {
  display: flex;
  gap: var(--space-5);
  align-items: flex-end;
  flex-wrap: wrap;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4);
  margin-bottom: var(--space-5);
}
.filters-bar label { display: flex; flex-direction: column; gap: var(--space-1); font-size: 0.75rem; }
.filters-bar select, .filters-bar input { width: auto; min-width: 150px; }

.chart-section { margin-bottom: var(--space-5); }
.chart-container {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4);
  box-shadow: var(--shadow-card);
}
.chart-container svg polyline { stroke: var(--accent); }
.chart-container svg text { fill: var(--ink-faint); font-size: 12px; font-family: var(--font-mono); }

/* ---------- Public status page ---------- */

.status-page {
  min-height: 100vh;
  background-color: var(--bg);
  padding: var(--space-7) var(--space-5);
}
.status-page-inner { max-width: 720px; margin: 0 auto; }
.status-page-inner h1 { margin-bottom: var(--space-5); }

.status-hero {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4) var(--space-5);
  margin-bottom: var(--space-6);
  box-shadow: var(--shadow-card);
}
.status-hero-label { font-weight: 600; }

.status-dot { width: 10px; height: 10px; border-radius: 50%; background: var(--neutral); flex-shrink: 0; }
[data-variant="operational"] .status-dot, .status-dot[data-variant="operational"] { background: var(--up); }
[data-variant="degraded"] .status-dot, .status-dot[data-variant="degraded"] { background: var(--degraded); }
[data-variant="outage"] .status-dot, .status-dot[data-variant="outage"] { background: var(--down); }
[data-variant="paused"] .status-dot, .status-dot[data-variant="paused"] { background: var(--ink-faint); }

.status-section { margin-bottom: var(--space-6); }
.service-list, .incident-list { list-style: none; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden; box-shadow: var(--shadow-card); }

.service-row, .incident-row { display: flex; align-items: center; justify-content: space-between; padding: var(--space-4); border-bottom: 1px solid var(--border); }
.service-row:last-child, .incident-row:last-child { border-bottom: none; }
.service-row-main { display: flex; align-items: center; gap: var(--space-3); }
.service-name, .incident-title { font-weight: 500; font-size: 0.9375rem; }
.service-url, .incident-description { font-size: 0.8125rem; color: var(--ink-muted); }
.service-uptime, .incident-date { font-family: var(--font-mono); font-size: 0.8125rem; color: var(--ink-muted); }

/* ---------- Responsive ---------- */

@media (max-width: 720px) {
  .app-header { flex-wrap: wrap; height: auto; padding: var(--space-3) var(--space-4); gap: var(--space-3); }
  .app-main { padding: var(--space-4); }
  .data-section.two-col { grid-template-columns: 1fr; }
  .monitor-info { grid-template-columns: 1fr; }
}

/* ==========================================================================
   Path 2 additions — sidebar shell, status badges, metric cards, empty/error
   states, confirm dialog, search input, filter bar, data-table convention
   ========================================================================== */

/* ---------- Sidebar app shell ---------- */

.app-shell {
  display: flex;
  flex-direction: row;
  min-height: 100vh;
}

.sidebar {
  width: 240px;
  min-width: 240px;
  background-color: var(--ink);
  color: #fff;
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  padding: var(--space-4) var(--space-3);
  position: sticky;
  top: 0;
  height: 100vh;
  overflow-y: auto;
}

.sidebar-brand {
  padding: var(--space-2) var(--space-3);
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  padding-bottom: var(--space-4);
}

.sidebar-brand a {
  color: #fff;
  font-weight: 700;
  letter-spacing: -0.01em;
  font-size: 1.0625rem;
  text-decoration: none;
}

.sidebar-nav {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.sidebar-group {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.sidebar-group-label {
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.45);
  padding: 0 var(--space-3);
  margin-bottom: var(--space-1);
}

.sidebar-group a {
  color: rgba(255, 255, 255, 0.72);
  font-size: 0.875rem;
  font-weight: 500;
  text-decoration: none;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  transition: background-color 0.12s ease, color 0.12s ease;
}

.sidebar-group a:hover {
  color: #fff;
  background-color: rgba(255, 255, 255, 0.06);
  text-decoration: none;
}

.sidebar-group a.active {
  color: #fff;
  background-color: var(--accent);
}

/* ---------- Path 1 additions — coming-soon nav items ---------- */

.sidebar-group-disabled {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  color: rgba(255, 255, 255, 0.32);
  font-size: 0.875rem;
  font-weight: 500;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  cursor: default;
  user-select: none;
}

.sidebar-soon-badge {
  font-size: 0.625rem;
  font-weight: 600;
  letter-spacing: 0.03em;
  text-transform: uppercase;
  color: rgba(255, 255, 255, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.16);
  border-radius: var(--radius-sm);
  padding: 1px 5px;
}

.sidebar-footer {
  border-top: 1px solid rgba(255, 255, 255, 0.08);
  padding-top: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.sidebar-user-email {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.55);
  padding: 0 var(--space-3);
  word-break: break-all;
}

.sidebar-signout {
  color: rgba(255, 255, 255, 0.85);
  font-family: var(--font-sans);
  font-size: 0.8125rem;
  font-weight: 500;
  text-decoration: none;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  transition: background-color 0.12s ease, color 0.12s ease;
}

.sidebar-signout:hover {
  color: #fff;
  background-color: rgba(255, 255, 255, 0.06);
  text-decoration: none;
}

.app-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}

.app-main {
  flex: 1;
  padding: var(--space-6) var(--space-5);
  max-width: 1180px;
  width: 100%;
  margin: 0 auto;
}

/* ---------- Status badge ---------- */

.status-badge {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  font-size: 0.8125rem;
  font-weight: 500;
  font-family: var(--font-sans);
  padding: 0.25rem 0.625rem;
  border-radius: var(--radius-sm);
  white-space: nowrap;
}

.status-badge-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
  background: currentColor;
}

.status-badge-label {
  color: inherit;
}

.status-badge-up {
  background-color: var(--up-soft);
  color: var(--up);
}

.status-badge-down {
  background-color: var(--down-soft);
  color: var(--down);
}

.status-badge-degraded {
  background-color: var(--degraded-soft);
  color: var(--degraded);
}

.status-badge-neutral,
.status-badge-paused {
  background-color: var(--neutral-soft);
  color: var(--neutral);
}

/* ---------- Metric card ---------- */

.metric-card {
  gap: var(--space-2);
}

.card-trend {
  font-size: 0.75rem;
  font-weight: 500;
  font-family: var(--font-sans);
  color: var(--ink-muted);
}

.card-trend-up {
  color: var(--up);
}

.card-trend-down {
  color: var(--down);
}

.card-trend-flat {
  color: var(--ink-faint);
}

/* ---------- Empty state ---------- */

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: var(--space-2);
  padding: var(--space-6) var(--space-4);
  background: var(--surface);
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-md);
  margin: var(--space-4) 0;
}

.empty-state-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--ink);
  margin: 0;
}

.empty-state-description {
  font-size: 0.875rem;
  color: var(--ink-muted);
  margin: 0;
  max-width: 420px;
}

.empty-state-actions {
  margin-top: var(--space-2);
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
  justify-content: center;
}

/* ---------- Error state (inline, not boundary) ---------- */

.error-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  gap: var(--space-2);
  padding: var(--space-5) var(--space-4);
  background-color: var(--down-soft);
  border: 1px solid rgba(194, 49, 31, 0.25);
  border-radius: var(--radius-md);
  margin: var(--space-4) 0;
}

.error-state-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--down);
  margin: 0;
}

.error-state-description {
  font-size: 0.875rem;
  color: var(--ink-muted);
  margin: 0;
  max-width: 420px;
}

.error-state .btn {
  margin-top: var(--space-2);
}

/* ---------- Confirm dialog ---------- */

.dialog-overlay {
  position: fixed;
  inset: 0;
  background-color: rgba(18, 21, 28, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: var(--space-4);
}

.dialog-panel {
  background: var(--surface);
  border-radius: var(--radius-md);
  box-shadow: 0 16px 32px rgba(0, 0, 0, 0.24);
  padding: var(--space-5);
  max-width: 420px;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.dialog-title {
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--ink);
  margin: 0;
}

.dialog-body {
  font-size: 0.875rem;
  color: var(--ink-muted);
  margin: 0;
}

.dialog-actions {
  display: flex;
  gap: var(--space-3);
  justify-content: flex-end;
  margin-top: var(--space-2);
}

/* ---------- Search input ---------- */

.search-input {
  position: relative;
  width: 100%;
  max-width: 320px;
}

.search-input input {
  width: 100%;
  padding-left: var(--space-4);
  font-size: 0.875rem;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background-color: var(--surface);
  color: var(--ink);
}

.search-input input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

/* ---------- Filter bar ---------- */

.filter-bar {
  display: flex;
  gap: var(--space-4);
  align-items: flex-end;
  flex-wrap: wrap;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--space-4);
  margin-bottom: var(--space-5);
  box-shadow: var(--shadow-card);
}

.filter-bar label {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  font-size: 0.75rem;
  font-weight: 500;
  color: var(--ink-muted);
}

.filter-bar select,
.filter-bar input {
  width: auto;
  min-width: 150px;
  font-size: 0.875rem;
}

/* ---------- Data-table convention ---------- */

.data-table {
  width: 100%;
  border-collapse: collapse;
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
  font-size: 0.875rem;
  margin: 0;
}

.data-table th {
  text-align: left;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--ink-muted);
  background: var(--surface-sunken);
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
}

.data-table td {
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--border);
  color: var(--ink);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.data-table td:first-child,
.data-table th:first-child {
  font-family: var(--font-sans);
}

.data-table tbody tr:last-child td {
  border-bottom: none;
}

.data-table tbody tr:hover td {
  background-color: var(--surface-sunken);
}

/* ---------- Responsive navigation ---------- */

.mobile-nav-toolbar,
.mobile-nav-backdrop,
.mobile-nav-close {
  display: none;
}

.sidebar-brand {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

@media (max-width: 900px) {
  .app-shell {
    flex-direction: row;
    min-width: 0;
  }

  .sidebar {
    position: fixed;
    inset: 0 auto 0 0;
    z-index: 1200;
    width: min(280px, 88vw);
    min-width: min(280px, 88vw);
    height: 100dvh;
    max-height: 100dvh;
    padding: var(--space-4) var(--space-3);
    overflow-x: hidden;
    overflow-y: auto;
    transform: translateX(-105%);
    transition: transform 0.18s ease;
    box-shadow: var(--shadow-lg);
  }

  .sidebar.mobile-nav-open {
    transform: translateX(0);
  }

  .sidebar-brand {
    padding: 0 var(--space-2) var(--space-4);
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .mobile-nav-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    min-width: 36px;
    height: 36px;
    min-height: 36px;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.16);
    color: #fff;
    background: transparent;
    font-size: 1.5rem;
    line-height: 1;
  }

  .mobile-nav-close:hover {
    color: #fff;
    background: rgba(255, 255, 255, 0.08);
  }

  .sidebar-nav {
    flex-direction: column;
    gap: var(--space-5);
    overflow: visible;
  }

  .sidebar-group {
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-1);
  }

  .sidebar-group-label {
    display: block;
  }

  .sidebar-group a {
    white-space: normal;
  }

  .sidebar-footer {
    flex-direction: column;
    align-items: stretch;
    justify-content: initial;
    gap: var(--space-2);
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    padding-top: var(--space-4);
  }

  .mobile-nav-toolbar {
    display: flex;
    align-items: center;
    min-height: 52px;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }

  .mobile-nav-toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 40px;
    padding: 0.5rem 0.75rem;
    font-weight: 600;
  }

  .mobile-nav-toggle-icon {
    font-size: 1.1rem;
    line-height: 1;
  }

  .mobile-nav-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1100;
    display: block;
    width: 100%;
    height: 100%;
    border: 0;
    padding: 0;
    background: rgba(18, 21, 28, 0.55);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.18s ease;
    cursor: default;
  }

  .mobile-nav-backdrop.mobile-nav-backdrop-open {
    opacity: 1;
    pointer-events: auto;
  }

  .app-content {
    width: 100%;
    min-width: 0;
  }

  .app-main {
    padding: var(--space-4);
    min-width: 0;
  }
}

@media (max-width: 600px) {
  .app-main {
    padding: var(--space-5) var(--space-3);
  }
}

@media (max-width: 375px) {
  .mobile-nav-toolbar {
    padding-inline: var(--space-2);
  }

  .metric-strip {
    grid-template-columns: 1fr;
  }
}

/* Keep dense dashboard tables usable on narrow screens without forcing
   page-level horizontal overflow. */
.monitor-table-wrap {
  max-width: 100%;
  overflow-x: auto;
  overflow-y: hidden;
  -webkit-overflow-scrolling: touch;
  overscroll-behavior-x: contain;
}

.dashboard-monitor-table {
  min-width: 640px;
}

/* ---------- Page header actions slot (follow-up fix) ---------- */

.page-header-actions {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-wrap: wrap;
}

/* ---------- Path 7 additions — security page (2FA, muted text) ---------- */

.muted {
  color: var(--muted, #6b7280);
  font-size: 0.875rem;
}

.totp-actions {
  display: flex;
  gap: var(--space-3);
  flex-wrap: wrap;
}

.totp-setup,
.totp-backup-codes {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.totp-uri {
  display: block;
  word-break: break-all;
  background: var(--surface-muted, #f3f4f6);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-3);
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  user-select: all;
}

.backup-code-list {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--space-2);
  list-style: none;
  padding: 0;
  margin: 0;
}

.backup-code-list code {
  display: block;
  background: var(--surface-muted, #f3f4f6);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-2);
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  text-align: center;
}

/* ---------- Frontend coverage expansion: compact, accessible feature UI ---------- */
.feature-page { display: flex; flex-direction: column; gap: var(--space-4); max-width: 1200px; }
.feature-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: var(--space-4); }
.stack-form { display: flex; flex-direction: column; gap: var(--space-3); }
.stack-form label, .stack-form fieldset { display: flex; flex-direction: column; gap: var(--space-2); }
.stack-form fieldset { border: 1px solid var(--border); padding: var(--space-3); border-radius: var(--radius-sm); }
.inline-form { display: flex; align-items: end; gap: var(--space-2); flex-wrap: wrap; }
.inline-form label { display: flex; flex-direction: column; gap: var(--space-1); }
.row-actions { display: flex; gap: var(--space-2); flex-wrap: wrap; }
.list-row { display: flex; justify-content: space-between; align-items: center; gap: var(--space-3); padding: var(--space-3) 0; border-bottom: 1px solid var(--border); }
.list-row:last-child { border-bottom: 0; }
.check-row { display: flex !important; flex-direction: row !important; align-items: center; gap: var(--space-2); }
.check-row input { width: auto; }
.secret-box { display: flex; flex-direction: column; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--surface-muted, #f3f4f6); }
.secret-box code { user-select: all; overflow-wrap: anywhere; }
.quote-box { display: flex; flex-direction: column; gap: var(--space-1); margin-top: var(--space-3); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); }
.subsection { margin-top: var(--space-4); padding-top: var(--space-4); border-top: 1px solid var(--border); }
.feature-editor { width: 100%; }
.link-button { width: 100%; background: transparent; border: 0; text-align: left; cursor: pointer; font: inherit; }
.article-content { white-space: pre-wrap; line-height: 1.65; }
.message { padding: var(--space-3); border-left: 3px solid var(--border); margin: var(--space-3) 0; }
.details { display: grid; grid-template-columns: minmax(120px, 180px) 1fr; gap: var(--space-2) var(--space-4); }
.details dt { font-weight: 600; }
.details dd { margin: 0; overflow-wrap: anywhere; }
.empty-state { padding: var(--space-5); text-align: center; border: 1px dashed var(--border); border-radius: var(--radius-sm); }

@media (max-width: 720px) {
  .feature-grid { grid-template-columns: 1fr; }
  .details { grid-template-columns: 1fr; }
  .inline-form { align-items: stretch; flex-direction: column; }
  .inline-form > * { width: 100%; }
}

/* ==========================================================================
   Phase 2 — Design-system standardization
   Single semantic language; existing selectors remain source-compatible.
   ========================================================================== */
:root {
  --surface-raised: #ffffff;
  --surface-muted: #f7f8fa;
  --surface-overlay: rgba(18, 21, 28, 0.55);
  --border-subtle: #eceef2;
  --ink-inverse: #ffffff;

  --status-up: #12855a;
  --status-up-soft: #e5f6ef;
  --status-down: #c2311f;
  --status-down-soft: #fbeae8;
  --status-warning: #9a6300;
  --status-warning-soft: #fbf1dd;
  --status-unknown: #6b7280;
  --status-unknown-soft: #eef0f3;
  --status-disabled: #8991a0;
  --status-disabled-soft: #f0f2f5;

  --radius-lg: 10px;
  --radius-full: 999px;
  --shadow-sm: 0 1px 2px rgba(18, 21, 28, 0.05);
  --shadow-md: 0 8px 24px rgba(18, 21, 28, 0.10);
  --shadow-lg: 0 16px 40px rgba(18, 21, 28, 0.16);
  --transition-fast: 120ms ease;
  --transition-standard: 180ms ease;
  --breakpoint-sm: 640px;
  --breakpoint-md: 720px;
  --breakpoint-lg: 1024px;
}

html { color-scheme: light; }

button, .btn, input, select, textarea { min-height: 38px; }
button, .btn {
  transition: background-color var(--transition-fast), border-color var(--transition-fast),
              color var(--transition-fast), box-shadow var(--transition-fast),
              transform var(--transition-fast);
}
button:focus-visible, .btn:focus-visible,
input:focus-visible, select:focus-visible, textarea:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
button:active:not(:disabled), .btn:active:not(:disabled) { transform: translateY(1px); }

input:disabled, select:disabled, textarea:disabled {
  background: var(--surface-sunken);
  color: var(--ink-faint);
  cursor: not-allowed;
}

input[aria-invalid="true"], select[aria-invalid="true"], textarea[aria-invalid="true"],
.input-error {
  border-color: var(--down);
}
input[aria-invalid="true"]:focus-visible, select[aria-invalid="true"]:focus-visible,
textarea[aria-invalid="true"]:focus-visible {
  box-shadow: 0 0 0 3px var(--down-soft);
}

.form-field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  width: 100%;
}
.form-field-label {
  margin: 0;
  color: var(--ink);
  font-size: 0.8125rem;
  font-weight: 600;
}
.required-indicator { color: var(--down); margin-left: 2px; }
.form-field-control { width: 100%; }
.form-help { margin: 0; font-size: 0.75rem; color: var(--ink-muted); }
.form-error-message {
  margin: 0;
  font-size: 0.75rem;
  color: var(--down);
  font-weight: 500;
}
.form-submitting { opacity: 0.75; pointer-events: none; }

.status-badge {
  border: 1px solid transparent;
  border-radius: var(--radius-full);
  min-height: 28px;
}
.status-badge-up { color: var(--status-up); background: var(--status-up-soft); }
.status-badge-down { color: var(--status-down); background: var(--status-down-soft); }
.status-badge-warning, .status-badge-degraded { color: var(--status-warning); background: var(--status-warning-soft); }
.status-badge-unknown, .status-badge-neutral { color: var(--status-unknown); background: var(--status-unknown-soft); }
.status-badge-disabled, .status-badge-paused { color: var(--status-disabled); background: var(--status-disabled-soft); }
.status-badge-dot { width: 7px; height: 7px; }

.alert, .toast, .success, .error, .form-error, .auth-success {
  border-radius: var(--radius-sm);
  padding: var(--space-3) var(--space-4);
  font-size: 0.875rem;
  border: 1px solid var(--border);
}
.alert-info, .toast-info {
  color: var(--ink);
  background: var(--accent-soft);
  border-color: rgba(67, 56, 202, 0.20);
}
.alert-success, .toast-success, .success, .auth-success {
  color: var(--up);
  background: var(--up-soft);
  border-color: rgba(18, 133, 90, 0.25);
}
.alert-warning, .toast-warning {
  color: var(--degraded);
  background: var(--degraded-soft);
  border-color: rgba(154, 99, 0, 0.25);
}
.alert-error, .toast-error, .error, .form-error {
  color: var(--down);
  background: var(--down-soft);
  border-color: rgba(194, 49, 31, 0.25);
}
.toast {
  position: fixed;
  right: var(--space-5);
  bottom: var(--space-5);
  z-index: 1100;
  max-width: min(420px, calc(100vw - 32px));
  box-shadow: var(--shadow-md);
}

.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-4);
  background: var(--surface-overlay);
}
.modal-panel {
  width: min(560px, 100%);
  max-height: min(720px, calc(100vh - 32px));
  overflow: auto;
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
}
.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-4) var(--space-5);
  border-bottom: 1px solid var(--border);
}
.modal-title { margin: 0; }
.modal-body { padding: var(--space-5); }
.icon-button {
  width: 36px;
  min-height: 36px;
  padding: 0;
  border: 0;
  background: transparent;
  font-size: 1.5rem;
}
.icon-button:hover { background: var(--surface-sunken); }

.tabs {
  display: flex;
  gap: var(--space-1);
  border-bottom: 1px solid var(--border);
  overflow-x: auto;
}
.tabs > * { flex: 0 0 auto; }

.dropdown { position: relative; }
.dropdown-trigger {
  display: inline-flex;
  align-items: center;
  min-height: 38px;
  padding: 0.5rem 0.875rem;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--surface);
  cursor: pointer;
  font-size: 0.875rem;
}
.dropdown-menu {
  position: absolute;
  z-index: 900;
  top: calc(100% + 4px);
  right: 0;
  min-width: 180px;
  padding: var(--space-2);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-md);
}
.dropdown-menu > * { display: block; width: 100%; }

.pagination {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-3);
  margin-top: var(--space-4);
}
.pagination span { color: var(--ink-muted); font-size: 0.8125rem; }

.tooltip { position: relative; display: inline-flex; }
.tooltip-content {
  position: absolute;
  z-index: 1000;
  left: 50%;
  bottom: calc(100% + 8px);
  transform: translateX(-50%);
  width: max-content;
  max-width: 260px;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--ink);
  color: var(--ink-inverse);
  font-size: 0.75rem;
  pointer-events: none;
  opacity: 0;
  transition: opacity var(--transition-fast);
}
.tooltip:hover .tooltip-content, .tooltip:focus-within .tooltip-content { opacity: 1; }

.skeleton-card {
  padding: var(--space-4);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--surface);
}
.skeleton-circle { width: 40px; height: 40px; border-radius: 50%; }
.skeleton-text { height: 14px; width: 70%; }
.skeleton-rect { height: 120px; width: 100%; }
.skeleton-card, .skeleton-line, .skeleton-circle, .skeleton-text, .skeleton-rect {
  background-color: var(--surface-sunken);
}

.date-range-picker {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-3);
}
.date-range-picker label { margin: 0; }
.date-range-picker label span { display: block; margin-bottom: var(--space-1); }

.command-palette {
  width: min(640px, calc(100vw - 32px));
  padding: var(--space-3);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
}
.command-palette-results { margin-top: var(--space-2); max-height: 360px; overflow: auto; }

.check-row, .switch-row {
  display: flex !important;
  align-items: center;
  gap: var(--space-2);
  min-height: 38px;
  margin: 0;
}
.check-row input { width: 18px; min-height: 18px; }
.switch-row { cursor: pointer; }
.switch-input {
  position: absolute;
  opacity: 0;
  width: 1px;
  height: 1px;
}
.switch-track {
  width: 40px;
  height: 22px;
  padding: 2px;
  border-radius: var(--radius-full);
  background: var(--border-strong);
  transition: background-color var(--transition-fast);
}
.switch-thumb {
  display: block;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--surface);
  box-shadow: var(--shadow-sm);
  transition: transform var(--transition-fast);
}
.switch-input:checked + .switch-track { background: var(--accent); }
.switch-input:checked + .switch-track .switch-thumb { transform: translateX(18px); }
.switch-input:focus-visible + .switch-track { outline: 2px solid var(--accent); outline-offset: 2px; }

.data-table caption {
  text-align: left;
  padding: var(--space-3) var(--space-4);
  font-weight: 600;
  color: var(--ink);
}
.data-table tbody tr:focus-within td { background: var(--surface-sunken); }

@media (max-width: 720px) {
  .page-header-actions { width: 100%; }
  .page-header-actions > .btn, .page-header-actions > a { width: 100%; }
  .date-range-picker { grid-template-columns: 1fr; }
  .toast { right: var(--space-4); bottom: var(--space-4); }
}


/* ---------- Flagship dashboard ---------- */

.dashboard-overview {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.dashboard-page-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--space-4);
  padding-bottom: var(--space-4);
  border-bottom: 1px solid var(--border);
}

.dashboard-page-header h1 {
  margin: 0;
  font-size: 2rem;
}

.page-subtitle {
  margin-top: var(--space-1);
  font-size: 0.8125rem;
  color: var(--ink-faint);
  font-family: var(--font-mono);
}

.eyebrow {
  margin-bottom: 2px;
  color: var(--ink-faint);
  font-size: 0.6875rem;
  line-height: 1.3;
  font-weight: 700;
  letter-spacing: 0.09em;
}

.dashboard-content {
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

.dashboard-refresh-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: 1px solid var(--down-soft);
  border-radius: var(--radius-md);
  background: var(--surface);
  color: var(--down);
  font-size: 0.8125rem;
}

.dashboard-refresh-error .btn {
  flex: 0 0 auto;
  padding: 0.35rem 0.625rem;
}

.metric-strip {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: var(--space-3);
}

.metric-card {
  min-width: 0;
  min-height: 118px;
  justify-content: space-between;
}

.metric-card.metric-primary {
  border-color: var(--border-strong);
}

.card-hint {
  color: var(--ink-faint);
  font-size: 0.6875rem;
}

.dashboard-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.section-heading,
.widget-heading {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: var(--space-4);
}

.section-heading h2,
.widget-heading h2 {
  margin-bottom: 0;
}

.dashboard-trend-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--space-4);
}

.dashboard-trend-card {
  min-height: 220px;
}

.widget-description {
  margin-top: 2px;
  font-size: 0.75rem;
  color: var(--ink-faint);
}

.trend-value {
  flex-shrink: 0;
  font-family: var(--font-mono);
  font-size: 1rem;
  font-variant-numeric: tabular-nums;
}

.trend-chart-wrap {
  margin-top: var(--space-5);
  overflow: hidden;
  border-top: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
}

.trend-chart {
  display: block;
  width: 100%;
  height: 120px;
  color: var(--accent);
}

.trend-line {
  vector-effect: non-scaling-stroke;
}

.trend-axis {
  display: flex;
  justify-content: space-between;
  padding-top: var(--space-2);
  color: var(--ink-faint);
  font-family: var(--font-mono);
  font-size: 0.625rem;
}

.trend-empty {
  display: flex;
  min-height: 132px;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-1);
  margin-top: var(--space-3);
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-sm);
  color: var(--ink-muted);
  text-align: center;
  font-size: 0.8125rem;
}

.trend-empty small {
  color: var(--ink-faint);
}

.monitor-table-wrap {
  overflow-x: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--surface);
}

.dashboard-monitor-table {
  min-width: 640px;
  border: 0;
}

.dashboard-monitor-table th,
.dashboard-monitor-table td {
  padding: 0.75rem 1rem;
}

.dashboard-monitor-table th {
  white-space: nowrap;
}

.dashboard-monitor-table tbody tr:last-child td {
  border-bottom: 0;
}

.monitor-link,
.incident-link {
  color: var(--ink);
  font-weight: 600;
}

.monitor-link:hover,
.incident-link:hover {
  color: var(--accent);
  text-decoration: none;
}

.data-cell {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  font-variant-numeric: tabular-nums;
}

.muted-cell {
  color: var(--ink-faint);
}

.dashboard-empty {
  display: flex;
  align-items: center;
  text-align: center;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-7) var(--space-5);
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface);
}

.dashboard-empty h3 {
  font-size: 1rem;
}

.dashboard-empty p {
  max-width: 480px;
  font-size: 0.8125rem;
}

.dashboard-empty .btn {
  margin-top: var(--space-2);
}

.dashboard-empty.compact {
  align-items: flex-start;
  text-align: left;
  padding: var(--space-5);
}

.incident-list {
  list-style: none;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--surface);
}

.incident-item + .incident-item {
  border-top: 1px solid var(--border);
}

.incident-link {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
}

.incident-marker {
  width: 8px;
  height: 8px;
  flex: 0 0 8px;
  border-radius: 50%;
  background: var(--down);
}

.incident-copy {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  gap: 2px;
}

.incident-copy strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.incident-copy small {
  color: var(--ink-faint);
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 400;
}

.incident-arrow {
  color: var(--ink-faint);
  font-size: 1rem;
}

.dashboard-data-note {
  color: var(--ink-faint);
  font-size: 0.6875rem;
  line-height: 1.5;
}

@media (max-width: 1100px) {
  .dashboard-trend-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 900px) {
  .metric-strip {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .dashboard-trend-grid {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 600px) {
  .app-main {
    padding: var(--space-5) var(--space-3);
  }

  .dashboard-page-header {
    align-items: flex-start;
  }

  .dashboard-page-header h1 {
    font-size: 1.5rem;
  }

  .metric-strip {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .metric-card {
    min-height: 104px;
  }
}

@media (max-width: 375px) {
  .metric-strip {
    grid-template-columns: 1fr;
  }

  .dashboard-page-header {
    flex-direction: column;
    align-items: stretch;
  }

  .dashboard-page-header .btn {
    width: 100%;
  }

  .dashboard-trend-card {
    padding: var(--space-3);
  }
}


/* ---------- Phase 4 — monitor detail/form polish ---------- */
.monitor-detail {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  max-width: 1200px;
}

.monitor-identity {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
}

.monitor-identity h2 { margin: 2px 0 2px; }
.monitor-status { flex: 0 0 auto; }

.eyebrow {
  display: block;
  color: var(--ink-faint);
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.muted { color: var(--ink-muted); font-size: 0.8125rem; }
.mono, code { font-family: var(--font-mono); font-size: 0.82em; }

.section-heading {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-3);
  margin-bottom: var(--space-3);
}

.section-heading h2 { margin-bottom: 0; }

.monitor-health-metrics {
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  margin-bottom: 0;
}

.performance-bars {
  display: flex;
  flex-direction: column;
  gap: 7px;
}

.performance-row {
  display: grid;
  grid-template-columns: 28px 1fr 72px;
  align-items: center;
  gap: 8px;
  min-height: 24px;
}

.performance-index {
  color: var(--ink-faint);
  font: 0.7rem var(--font-mono);
  text-align: right;
}

.performance-track {
  height: 8px;
  overflow: hidden;
  background: var(--surface-sunken);
  border-radius: 999px;
}

.performance-fill {
  height: 100%;
  min-width: 2px;
  border-radius: inherit;
  background: var(--accent);
}

.performance-up { background: var(--up); }
.performance-down { background: var(--down); }
.performance-value {
  color: var(--ink-muted);
  font: 0.72rem var(--font-mono);
  text-align: right;
}

.list-stack {
  display: flex;
  flex-direction: column;
  gap: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.list-row {
  display: flex;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-3) var(--space-4);
  background: var(--surface);
  border-bottom: 1px solid var(--border);
}

.list-row:last-child { border-bottom: 0; }

.escalation-steps {
  margin: var(--space-2) 0 0 var(--space-5);
  color: var(--ink-muted);
  font-size: 0.8rem;
}

.monitor-subform { margin-top: var(--space-4); }
.form-section { margin-bottom: 0; }
.monitor-form { gap: var(--space-4); }
.monitor-form .form-section h2 { margin-bottom: var(--space-3); }
.form-help { margin: 0 0 var(--space-2); }
.form-error-message {
  margin: -4px 0 var(--space-2);
  color: var(--down);
  font-size: 0.78rem;
}
.form-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}

@media (max-width: 640px) {
  .monitor-identity { align-items: flex-start; flex-direction: column; }
  .performance-row { grid-template-columns: 22px 1fr 62px; }
  .channel-form { align-items: stretch; }
  .channel-form input, .channel-form select { width: 100%; }
}



/* Phase 5: restrained, reusable UX primitives. */
.table-scroll {
  width: 100%;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}
.table-scroll table {
  min-width: 620px;
}
.secret-box {
  position: relative;
}
.secret-box code {
  max-height: 8rem;
  overflow: auto;
}
.row-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: var(--space-2);
}
.empty-state {
  padding: var(--space-5);
  text-align: center;
}
.empty-state h2,
.empty-state h3,
.empty-state p {
  margin-bottom: var(--space-2);
}
.performance-bars {
  overflow-x: auto;
}
.performance-row {
  min-width: 520px;
}
@media (max-width: 640px) {
  .row-actions {
    justify-content: flex-start;
  }
}
EOF

echo "[PASS] Phase 5 frontend changes written."

echo "=== SOURCE-LEVEL CONTRACT CHECKS ==="
grep -q '/dashboard/rum/sites/{site_id}/overview' crates/api/src/routes/rum_dashboard.rs
grep -q '/status-pages/{id}/logo' crates/api/src/routes/status_pages.rs
grep -q '/members/{user_id}/role' crates/api/src/routes/members.rs
grep -q '/invitations/pending' crates/api/src/routes/invitations.rs
grep -q '/api-keys/{id}' crates/api/src/routes/api_keys.rs
grep -q '/monitors/{id}/flows/{flow_id}' crates/api/src/routes/api_flows.rs
grep -q '/billing/checkout' crates/api/src/routes/billing.rs
grep -q 'pub async fn delete_flow' apps/leptos-dashboard/src/api/developer.rs
! grep -R -nE 'localStorage|sessionStorage' apps/leptos-dashboard/src
echo "[PASS] Frontend secret-storage and endpoint contract checks passed."

echo "=== RUST TOOLCHAIN CHECKS ==="
if command -v cargo >/dev/null 2>&1; then
  echo "[RUN] cargo check -p leptos-dashboard"
  cargo check -p leptos-dashboard
  echo "[PASS] cargo check -p leptos-dashboard"
  echo "[RUN] cargo test -p leptos-dashboard"
  cargo test -p leptos-dashboard
  echo "[PASS] cargo test -p leptos-dashboard"
else
  echo "[WARN] cargo is not installed; Rust compile/test checks were NOT run."
  echo "       Install/use the project Rust toolchain, then run: cargo check -p leptos-dashboard && cargo test -p leptos-dashboard"
fi

echo
echo "=== PHASE 5 RESULT ==="
echo "Applied: RUM, RUM Sites, Status Pages, Team, Invitations, API Keys, API Flows, Billing, Add-ons, Notifications."
echo "Security: no browser storage for secrets; one-time secrets are explicitly hideable; destructive actions require confirmation; duplicate async actions are guarded."
echo "UX: loading/empty/error states, compact tables/lists, previews, scoped controls, simple responsive presentation."
echo "Backup retained at: $BACKUP"
