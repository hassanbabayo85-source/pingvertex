use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn MetricCard(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(optional, into)] trend: Option<String>,
    #[prop(optional, into)] trend_variant: Option<String>,
) -> impl IntoView {
    let trend_view = match (trend, trend_variant) {
        (Some(t), Some(v)) => {
            let class = format!("card-trend card-trend-{}", v);
            view! { <span class=class>{t}</span> }.into_any()
        }
        (Some(t), None) => view! { <span class="card-trend">{t}</span> }.into_any(),
        _ => ().into_any(),
    };

    view! {
        <div class="card metric-card">
            <span class="card-label">{label}</span>
            <span class="card-value">{value}</span>
            {trend_view}
        </div>
    }
}
