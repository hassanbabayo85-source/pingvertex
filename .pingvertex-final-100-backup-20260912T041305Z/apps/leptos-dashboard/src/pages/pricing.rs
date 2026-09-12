use crate::api::billing;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(u: i64) -> String {
    format!("${}.{:06}", u / 1_000_000, u.abs() % 1_000_000)
}

// Fallback values used only until the server-supplied limits load. The
// server response is the source of truth; these MUST match the constants
// in `crates/application/src/use_cases/calculate_payg_preview.rs`.
const FALLBACK_MAX_REGIONS: u32 = 32;
const FALLBACK_MAX_FORECAST_DAYS: u32 = 3650;
const FALLBACK_MIN_INTERVAL_HTTP: u32 = 30;
const FALLBACK_MIN_INTERVAL_FLOW: u32 = 60;

#[component]
pub fn PricingPage() -> impl IntoView {
    let prices = RwSignal::new(Vec::<billing::PaygPricing>::new());
    let free_tier = RwSignal::new(None::<billing::FreeTier>);
    let preview_limits = RwSignal::new(None::<billing::PreviewLimits>);
    let error = RwSignal::new(None::<String>);

    let monitors = RwSignal::new("100".to_string());
    let interval = RwSignal::new("300".to_string());
    let regions = RwSignal::new("1".to_string());
    let forecast_days = RwSignal::new("30".to_string());
    let kind = RwSignal::new("http".to_string());
    let preview = RwSignal::new(None::<billing::PaygPreview>);
    let preview_error = RwSignal::new(None::<String>);

    let max_regions = move || {
        preview_limits
            .get()
            .map(|l| l.max_regions)
            .unwrap_or(FALLBACK_MAX_REGIONS)
    };
    let max_forecast_days = move || {
        preview_limits
            .get()
            .map(|l| l.max_forecast_days)
            .unwrap_or(FALLBACK_MAX_FORECAST_DAYS)
    };
    let min_interval_for_kind = move || {
        let k = kind.get();
        let (http, flow) = preview_limits
            .get()
            .map(|l| (l.min_interval_seconds_http, l.min_interval_seconds_flow))
            .unwrap_or((FALLBACK_MIN_INTERVAL_HTTP, FALLBACK_MIN_INTERVAL_FLOW));
        if k.as_str() == "flow" {
            flow
        } else {
            http
        }
    };

    Effect::new(move |_| {
        spawn_local(async move {
            match billing::payg_pricing().await {
                Ok(page) => {
                    prices.set(page.prices);
                    free_tier.set(Some(page.free_tier));
                    preview_limits.set(Some(page.preview_limits));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });

    Effect::new(move |_| {
        let parsed = (
            monitors.get().parse::<u64>().ok(),
            interval.get().parse::<u32>().ok(),
            regions.get().parse::<u32>().ok(),
            forecast_days.get().parse::<u32>().ok(),
        );
        let (Some(monitors), Some(interval_seconds), Some(regions), Some(forecast_days)) = parsed
        else {
            preview.set(None);
            return;
        };
        let kind_value = kind.get();
        if monitors == 0
            || interval_seconds < min_interval_for_kind()
            || regions == 0
            || regions > max_regions()
            || forecast_days == 0
            || forecast_days > max_forecast_days()
        {
            preview.set(None);
            return;
        }

        spawn_local(async move {
            match billing::payg_preview(&billing::PaygPreviewRequest {
                monitors,
                interval_seconds,
                regions,
                forecast_days,
                kind: kind_value,
            })
            .await
            {
                Ok(value) => {
                    preview_error.set(None);
                    preview.set(Some(value));
                }
                Err(e) => {
                    preview.set(None);
                    preview_error.set(Some(e.to_string()));
                }
            }
        });
    });

    view! {
        <div class="feature-page">
            <header class="page-header">
                <div>
                    <span class="eyebrow">"PAYG"</span>
                    <h1>"Pricing"</h1>
                    <p class="muted">"Server-controlled prices per actual billable unit."</p>
                </div>
            </header>

            {move || error.get().map(|e| view! { <div class="error" role="alert">{e}</div> })}

            <section class="card">
                <h2>"Cost estimator"</h2>
                <p class="muted">"This is an estimate, not a guaranteed invoice."</p>
                <div class="form-grid">
                    <label>
                        "Monitors"
                        <input type="number" min="1" prop:value=monitors
                            on:input=move |e| monitors.set(event_target_value(&e))/>
                    </label>
                    <label>
                        "Check interval (seconds)"
                        <input
                            type="number"
                            prop:min=move || min_interval_for_kind().to_string()
                            prop:value=interval
                            on:input=move |e| interval.set(event_target_value(&e))/>
                    </label>
                    <label>
                        "Regions"
                        <input
                            type="number"
                            min="1"
                            prop:max=move || max_regions().to_string()
                            prop:value=regions
                            on:input=move |e| regions.set(event_target_value(&e))/>
                    </label>
                    <label>
                        "Forecast days"
                        <input
                            type="number"
                            min="1"
                            prop:max=move || max_forecast_days().to_string()
                            prop:value=forecast_days
                            on:input=move |e| forecast_days.set(event_target_value(&e))/>
                    </label>
                    <label>
                        "Monitor type"
                        <select prop:value=kind on:change=move |e| kind.set(event_target_value(&e))>
                            <option value="http">"HTTP"</option>
                            <option value="tcp">"TCP"</option>
                            <option value="dns">"DNS"</option>
                            <option value="flow">"Flow/API"</option>
                        </select>
                    </label>
                </div>

                {move || match preview.get() {
                    Some(p) => view! {
                        <div class="pricing-preview" aria-live="polite">
                            <dl>
                                <div><dt>"Checks/day"</dt><dd>{p.checks_per_day}</dd></div>
                                <div><dt>"Checks in period"</dt><dd>{p.checks_total}</dd></div>
                                <div><dt>"Price/check"</dt><dd>{usd(p.price_per_check_units)}</dd></div>
                                <div><dt>"Estimated cost"</dt><dd>{usd(p.estimated_cost_units)}</dd></div>
                            </dl>
                            <p class="form-help">"Actual charges remain execution-based and can differ if monitors are disabled or schedules change."</p>
                        </div>
                    }.into_any(),
                    None => view! {
                        <div class="form-help">
                            {move || preview_error.get().unwrap_or_else(|| "Enter valid forecast values to calculate.".into())}
                        </div>
                    }.into_any(),
                }}
            </section>

            <section class="card">
                <div class="overview-cards">
                    {move || prices.get().into_iter().map(|p| view! {
                        <div>
                            <span class="card-label">{p.operation}</span>
                            <span class="card-value">{usd(p.unit_price)}</span>
                            <span class="muted">{format!("per {} · v{}", p.unit_name, p.version)}</span>
                        </div>
                    }).collect_view()}
                </div>
            </section>

            <section class="card">
                <h2>"Free entitlement"</h2>
                {move || match free_tier.get() {
                    Some(ft) => view! {
                        <p class="muted">{format!(
                            "Up to {} monitors at a {} second minimum interval, with {} days of history retention. \
                             A funded wallet unlocks PAYG capacity without a product monitor-count ceiling.",
                            ft.max_monitors,
                            ft.min_check_interval_seconds,
                            ft.retention_days,
                        )}</p>
                    }.into_any(),
                    None => view! {
                        <p class="muted">"Loading entitlement…"</p>
                    }.into_any(),
                }}
            </section>
        </div>
    }
}
