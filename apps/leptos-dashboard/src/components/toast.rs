use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Toast(
    #[prop(into)] message: String,
    #[prop(optional, into)] variant: Option<String>,
) -> impl IntoView {
    let variant = variant.unwrap_or_else(|| "info".into());
    let class = format!("toast toast-{variant}");
    view! { <div class=class role="status" aria-live="polite">{message}</div> }
}
