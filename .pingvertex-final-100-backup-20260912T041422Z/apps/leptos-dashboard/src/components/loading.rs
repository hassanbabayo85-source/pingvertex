use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn LoadingSpinner(#[prop(optional, into)] text: Option<String>) -> impl IntoView {
    let text = text.unwrap_or_else(|| "Loading...".to_string());
    view! {
        <div class="loading-container" role="status" aria-live="polite">
            <div class="loading-spinner" aria-hidden="true"></div>
            <span>{text}</span>
        </div>
    }
}

#[allow(dead_code)]
#[component]
pub fn LoadingSkeleton() -> impl IntoView {
    view! {
        <div class="skeleton skeleton-card" aria-hidden="true">
            <div class="skeleton-line"></div>
            <div class="skeleton-line short"></div>
        </div>
    }
}
