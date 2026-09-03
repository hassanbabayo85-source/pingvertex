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

        let days = range.get().parse::<f64>().unwrap_or(1.0);
        loading.set(true);

        let now = js_sys::Date::new_0();
        let end = now.to_iso_string().as_string().unwrap_or_default();

        let start_ms = now.get_time() - (days * 24.0 * 60.0 * 60.0 * 1000.0);

        let start = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(start_ms))
            .to_iso_string()
            .as_string()
            .unwrap_or_default();

        spawn_local(async move {
            if let Ok(value) = rum::rum_overview_typed(&site_id, Some(&start), Some(&end)).await {
                overview.set(Some(value));
            }

            if let Ok(value) =
                rum::rum_slow_pages_typed(&site_id, Some(&start), Some(&end), Some(10)).await
            {
                pages.set(value);
            }

            if let Ok(value) =
                rum::rum_error_breakdown_typed(&site_id, Some(&start), Some(&end), Some(10)).await
            {
                errors.set(Some(value));
            }

            if let Ok(value) =
                rum::rum_browser_device_breakdown_typed(&site_id, Some(&start), Some(&end)).await
            {
                browsers.set(Some(value));
            }

            if let Ok(value) =
                rum::rum_geo_breakdown_typed(&site_id, Some(&start), Some(&end), Some(10)).await
            {
                geo.set(value);
            }

            if let Ok(value) =
                rum::rum_performance_trend_typed(&site_id, Some(&start), Some(&end)).await
            {
                trend.set(value);
            }

            if let Ok(value) =
                rum::rum_session_stats_typed(&site_id, Some(&start), Some(&end)).await
            {
                sessions.set(Some(value));
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
