use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn StatusBadge(
    #[prop(into)] status: String,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let normalized = status.trim().to_lowercase();
    let (variant, default_label) = match normalized.as_str() {
        "up" | "operational" => ("up", "Operational"),
        "down" | "outage" => ("down", "Down"),
        "degraded" | "warning" | "degraded_performance" => ("warning", "Warning"),
        "disabled" | "paused" => ("disabled", "Disabled"),
        _ => ("unknown", "Unknown"),
    };
    let display_label = label.unwrap_or_else(|| default_label.to_string());
    let class = format!("status-badge status-badge-{}", variant);
    let aria = format!("Status: {display_label}");

    view! {
        <span class=class role="status" aria-label=aria>
            <span class="status-badge-dot" aria-hidden="true"></span>
            <span class="status-badge-label">{display_label}</span>
        </span>
    }
}
