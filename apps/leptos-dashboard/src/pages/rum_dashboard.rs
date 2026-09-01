use crate::api::rum::{self, RumDashboardData, RumSite};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;

#[component]
pub fn RumDashboardPage() -> impl IntoView {
    let sites = RwSignal::new(Vec::<RumSite>::new());
    let selected_site = RwSignal::new(None::<String>);
    let range_days = RwSignal::new(String::from("1")); // "1" (24h), "7", "30"
    let custom_from = RwSignal::new(String::new());
    let custom_to = RwSignal::new(String::new());
    let data = RwSignal::new(RumDashboardData::default());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let query = use_query_map();

    // On mount: load sites, read initial query params
    Effect::new(move |_| {
        spawn_local(async move {
            match rum::fetch_rum_sites().await {
                Ok(site_list) => {
                    sites.set(site_list);
                }
                Err(e) => error.set(Some(e)),
            }
        });
        // Read initial query params if present
        let params = query.get();
        if let Some(site) = params.get("site") {
            selected_site.set(Some(site));
        }
        if let Some(days) = params.get("days") {
            range_days.set(days);
        }
        if let Some(from) = params.get("from") {
            custom_from.set(from);
        }
        if let Some(to) = params.get("to") {
            custom_to.set(to);
        }
    });

    // Fetch data when filters change
    Effect::new(move |_| {
        let site = selected_site.get();
        let days = range_days.get();
        let from = custom_from.get();
        let to = custom_to.get();

        let from_param = if days == "custom" {
            if from.is_empty() {
                None
            } else {
                Some(from)
            }
        } else {
            None // backend defaults to last N days if from/to omitted?
        };
        let to_param = if days == "custom" {
            if to.is_empty() {
                None
            } else {
                Some(to)
            }
        } else {
            None
        };

        let days_val = days.parse::<u32>().unwrap_or(1);
        let limit = Some(100u32); // hardcoded reasonable limit

        spawn_local(async move {
            let site_ref = site.as_deref();
            loading.set(true);
            let result = if days == "custom" {
                rum::fetch_rum_dashboard(
                    site_ref,
                    from_param.as_deref(),
                    to_param.as_deref(),
                    limit,
                )
                .await
            } else {
                // For preset ranges, compute from/to based on days
                // Backend likely accepts "from" and "to" only, so we'll compute.
                // We'll compute ISO timestamps for last N days.
                use js_sys::Date;
                use wasm_bindgen::JsValue;
                let now = Date::now();
                let days_ms = days_val as f64 * 24.0 * 60.0 * 60.0 * 1000.0;
                let from_ms = now - days_ms;
                let from_date = Date::new(&JsValue::from_f64(from_ms));
                let to_date = Date::new(&JsValue::from_f64(now));
                let from_iso = from_date.to_iso_string().as_string().unwrap();
                let to_iso = to_date.to_iso_string().as_string().unwrap();
                rum::fetch_rum_dashboard(site_ref, Some(&from_iso), Some(&to_iso), limit).await
            };
            match result {
                Ok(d) => data.set(d),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

    view! {
        <div class="rum-dashboard-page">
            <header class="page-header">
                <h1>"Real User Monitoring"</h1>
            </header>

            <div class="filters-bar">
                <label>
                    "Site:"
                    <select
                        prop:value=move || selected_site.get().unwrap_or_default()
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            if value.is_empty() {
                                selected_site.set(None);
                            } else {
                                selected_site.set(Some(value));
                            }
                        }
                    >
                        <option value="">"All Sites"</option>
                        {move || sites.get().into_iter().map(|s| {
                            let id = s.id.clone();
                            view! { <option value={id.clone()}>{s.name.clone()}</option> }.into_any()
                        }).collect_view()}
                    </select>
                </label>
                <label>
                    "Range:"
                    <select
                        prop:value=range_days
                        on:change=move |ev| range_days.set(event_target_value(&ev))
                    >
                        <option value="1">"Last 24 hours"</option>
                        <option value="7">"Last 7 days"</option>
                        <option value="30">"Last 30 days"</option>
                        <option value="custom">"Custom..."</option>
                    </select>
                </label>
                {move || if range_days.get() == "custom" {
                    view! {
                        <label>
                            "From:"
                            <input type="datetime-local" prop:value=custom_from on:input=move |ev| custom_from.set(event_target_value(&ev)) />
                        </label>
                        <label>
                            "To:"
                            <input type="datetime-local" prop:value=custom_to on:input=move |ev| custom_to.set(event_target_value(&ev)) />
                        </label>
                    }.into_any()
                } else {
                    ().into_any()
                }}
            </div>

            {move || {
                if loading.get() {
                    view! { <div class="loading">"Loading RUM data..."</div> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <div class="error">{err}</div> }.into_any()
                } else {
                    let d = data.get();
                    let overview = &d.overview;

                    view! {
                        <section class="overview-cards">
                            <div class="card"><span class="card-label">"Page Views"</span><span class="card-value">{overview.total_page_views}</span></div>
                            <div class="card"><span class="card-label">"Sessions"</span><span class="card-value">{overview.total_sessions}</span></div>
                            <div class="card"><span class="card-label">"Avg LCP (ms)"</span><span class="card-value">{format!("{:.0}", overview.avg_lcp)}</span></div>
                            <div class="card"><span class="card-label">"Avg CLS"</span><span class="card-value">{format!("{:.3}", overview.avg_cls)}</span></div>
                            <div class="card"><span class="card-label">"Avg FCP (ms)"</span><span class="card-value">{format!("{:.0}", overview.avg_fcp)}</span></div>
                            <div class="card"><span class="card-label">"Avg TTFB (ms)"</span><span class="card-value">{format!("{:.0}", overview.avg_ttfb)}</span></div>
                            <div class="card"><span class="card-label">"Error Rate"</span><span class="card-value">{format!("{:.2}%", overview.error_rate * 100.0)}</span></div>
                        </section>

                        <section class="chart-section">
                            <h2>"Performance Trend (LCP)"</h2>
                            <div class="chart-container">
                                <svg width="100%" height="200" viewBox="0 0 600 200" preserveAspectRatio="none">
                                    {move || {
                                        let points = &d.trends;
                                        if points.len() < 2 {
                                            view! { <text x="300" y="100" text-anchor="middle">"Not enough data"</text> }.into_any()
                                        } else {
                                            let max_lcp = points.iter().map(|p| p.avg_lcp).fold(0.0, f64::max);
                                            let min_lcp = points.iter().map(|p| p.avg_lcp).fold(f64::MAX, f64::min);
                                            let span = if (max_lcp - min_lcp) < 0.001 { 1.0 } else { max_lcp - min_lcp };
                                            let step_x = 580.0 / (points.len() as f64 - 1.0);
                                            let coords: Vec<(f64, f64)> = points.iter().enumerate().map(|(i, p)| {
                                                let x = 10.0 + i as f64 * step_x;
                                                let y = 190.0 - ((p.avg_lcp - min_lcp) / span) * 170.0;
                                                (x, y)
                                            }).collect();
                                            view! {
                                                <polyline points={coords.iter().map(|(x,y)| format!("{:.1},{:.1}",x,y)).collect::<Vec<_>>().join(" ")} fill="none" stroke="blue" stroke-width="2"/>
                                            }.into_any()
                                        }
                                    }}
                                </svg>
                            </div>
                        </section>

                        <section class="data-section">
                            <h2>"Page Performance"</h2>
                            <div class="table-responsive">
                                <table>
                                    <thead><tr><th>"Path"</th><th>"Views"</th><th>"Avg LCP"</th><th>"Avg CLS"</th><th>"Avg FCP"</th><th>"Errors"</th></tr></thead>
                                    <tbody>
                                        {d.pages.iter().map(|p| view! {
                                            <tr>
                                                <td>{p.path.clone()}</td>
                                                <td>{p.view_count}</td>
                                                <td>{format!("{:.0}", p.avg_lcp)}</td>
                                                <td>{format!("{:.3}", p.avg_cls)}</td>
                                                <td>{format!("{:.0}", p.avg_fcp)}</td>
                                                <td>{p.error_count}</td>
                                            </tr>
                                        }.into_any()).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        </section>

                        <section class="data-section">
                            <h2>"Slow Pages"</h2>
                            <div class="table-responsive">
                                <table>
                                    <thead><tr><th>"Path"</th><th>"Views"</th><th>"Avg LCP"</th><th>"P75 LCP"</th><th>"P95 LCP"</th></tr></thead>
                                    <tbody>
                                        {d.slow_pages.iter().map(|p| view! {
                                            <tr>
                                                <td>{p.path.clone()}</td>
                                                <td>{p.view_count}</td>
                                                <td>{format!("{:.0}", p.avg_lcp)}</td>
                                                <td>{format!("{:.0}", p.p75_lcp)}</td>
                                                <td>{format!("{:.0}", p.p95_lcp)}</td>
                                            </tr>
                                        }.into_any()).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        </section>

                        <section class="data-section">
                            <h2>"JavaScript Errors"</h2>
                            <div class="table-responsive">
                                <table>
                                    <thead><tr><th>"Message"</th><th>"Count"</th><th>"Affected Sessions"</th><th>"Last Seen"</th></tr></thead>
                                    <tbody>
                                        {d.js_errors.iter().map(|e| view! {
                                            <tr>
                                                <td>{e.message.clone()}</td>
                                                <td>{e.error_count}</td>
                                                <td>{e.affected_sessions}</td>
                                                <td>{e.last_seen.clone()}</td>
                                            </tr>
                                        }.into_any()).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        </section>

                        <section class="data-section two-col">
                            <div>
                                <h2>"Browser Statistics"</h2>
                                <table>
                                    <thead><tr><th>"Browser"</th><th>"Count"</th><th>"%"</th></tr></thead>
                                    <tbody>
                                        {d.browser_stats.iter().map(|b| view! {
                                            <tr>
                                                <td>{b.browser.clone()}</td>
                                                <td>{b.count}</td>
                                                <td>{format!("{:.1}", b.percentage)}</td>
                                            </tr>
                                        }.into_any()).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                            <div>
                                <h2>"Device Statistics"</h2>
                                <table>
                                    <thead><tr><th>"Device"</th><th>"Count"</th><th>"%"</th></tr></thead>
                                    <tbody>
                                        {d.device_stats.iter().map(|dev| view! {
                                            <tr>
                                                <td>{dev.device_type.clone()}</td>
                                                <td>{dev.count}</td>
                                                <td>{format!("{:.1}", dev.percentage)}</td>
                                            </tr>
                                        }.into_any()).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        </section>

                        <section class="data-section">
                            <h2>"Recent Sessions"</h2>
                            <div class="table-responsive">
                                <table>
                                    <thead><tr><th>"Session ID"</th><th>"Country"</th><th>"Device"</th><th>"Browser"</th><th>"OS"</th><th>"Page Views"</th><th>"Duration (s)"</th></tr></thead>
                                    <tbody>
                                        {d.sessions.iter().take(50).map(|s| view! {
                                            <tr>
                                                <td>{s.session_id.clone()}</td>
                                                <td>{s.country_code.clone()}</td>
                                                <td>{s.device_type.clone()}</td>
                                                <td>{s.browser.clone()}</td>
                                                <td>{s.os.clone()}</td>
                                                <td>{s.page_views}</td>
                                                <td>{format!("{:.1}", s.total_duration_ms as f64 / 1000.0)}</td>
                                            </tr>
                                        }.into_any()).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        </section>
                    }.into_any()
                }
            }}
        </div>
    }
}
