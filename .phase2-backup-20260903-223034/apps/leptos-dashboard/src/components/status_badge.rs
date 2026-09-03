use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn StatusBadge(
    #[prop(into)] status: String,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let normalized = status.to_lowercase();
    let (variant, default_label) = match normalized.as_str() {
        "up" | "operational" => ("up", "Operational"),
        "down" | "outage" => ("down", "Down"),
        "degraded" => ("degraded", "Degraded"),
        "paused" => ("paused", "Paused"),
        _ => ("neutral", "Unknown"),
    };
    let display_label = label.unwrap_or_else(|| default_label.to_string());
    let class = format!("status-badge status-badge-{}", variant);

    view! {
        <span class=class>
            <span class="status-badge-dot"></span>
            <span class="status-badge-label">{display_label}</span>
        </span>
    }
}
