use crate::api::billing;
use leptos::prelude::*;
use leptos::task::spawn_local;

fn usd(u: i64) -> String {
    format!("${}.{:06}", u / 1_000_000, u.abs() % 1_000_000)
}

fn operation_label(operation: &str) -> String {
    match operation {
        "http_check.one_time" => "HTTP — one-time check".into(),
        "tcp_check.one_time" => "TCP — one-time check".into(),
        "dns_check.one_time" => "DNS — one-time check".into(),
        "api_flow_check.one_time" => "API flow — one-time check".into(),
        "proxy_check.one_time" => "Proxy — one-time check".into(),
        "http_check.monitoring" => "HTTP — monitoring".into(),
        "tcp_check.monitoring" => "TCP — monitoring".into(),
        "dns_check.monitoring" => "DNS — monitoring".into(),
        "api_flow_check.monitoring" => "API flow — monitoring".into(),
        "heartbeat_processing" => "Heartbeat processing".into(),
        "rum_event" => "RUM event".into(),
        "sms" => "SMS".into(),
        "storage" => "Storage write".into(),
        "network_egress" => "Network egress".into(),
        _ => operation.to_owned(),
    }
}

// Business limits are server-owned. Until the API response arrives, the
// estimator stays in a loading/disabled state rather than guessing policy
// values in the browser.

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

    let max_regions = move || preview_limits.get().map(|l| l.max_regions);
    let max_forecast_days = move || preview_limits.get().map(|l| l.max_forecast_days);
    let min_interval_for_kind = move || {
        preview_limits.get().map(|l| match kind.get().as_str() {
            "tcp" => l.min_interval_seconds_tcp,
            "dns" => l.min_interval_seconds_dns,
            "heartbeat" => l.min_interval_seconds_heartbeat,
            "flow" => l.min_interval_seconds_flow,
            _ => l.min_interval_seconds_http,
        })
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
        let (Some(min_interval), Some(max_regions), Some(max_forecast_days)) =
            (min_interval_for_kind(), max_regions(), max_forecast_days())
        else {
            preview.set(None);
            preview_error.set(Some("Loading server pricing limits…".into()));
            return;
        };
        if monitors == 0
            || interval_seconds < min_interval
            || interval_seconds > 86_400
            || regions == 0
            || regions > max_regions
            || forecast_days == 0
            || forecast_days > max_forecast_days
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
                            prop:min=move || min_interval_for_kind().unwrap_or(1).to_string()
                            prop:value=interval
                            on:input=move |e| interval.set(event_target_value(&e))/>
                    </label>
                    <label>
                        "Regions"
                        <input
                            type="number"
                            min="1"
                            prop:max=move || max_regions().unwrap_or(u32::MAX).to_string()
                            prop:value=regions
                            on:input=move |e| regions.set(event_target_value(&e))/>
                    </label>
                    <label>
                        "Forecast days"
                        <input
                            type="number"
                            min="1"
                            prop:max=move || max_forecast_days().unwrap_or(u32::MAX).to_string()
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
                            <span class="card-label">{operation_label(&p.operation)}</span>
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
