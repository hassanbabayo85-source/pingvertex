use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn LoadingSpinner(#[prop(optional, into)] text: Option<String>) -> impl IntoView {
    let text = text.unwrap_or_else(|| "Loading...".to_string());
    view! {
        <div class="loading-container">
            <div class="loading-spinner" aria-label="loading"></div>
            <span>{text}</span>
        </div>
    }
}

#[allow(dead_code)]
#[component]
pub fn LoadingSkeleton() -> impl IntoView {
    view! {
        <div class="skeleton">
            <div class="skeleton-line"></div>
            <div class="skeleton-line short"></div>
        </div>
    }
}
