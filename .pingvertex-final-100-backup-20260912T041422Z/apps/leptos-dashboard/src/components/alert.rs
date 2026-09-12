use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Alert(
    #[prop(into)] message: String,
    #[prop(optional, into)] variant: Option<String>,
) -> impl IntoView {
    let variant = variant.unwrap_or_else(|| "info".into());
    let role = if variant == "error" {
        "alert"
    } else {
        "status"
    };
    let class = format!("alert alert-{variant}");
    view! { <div class=class role=role aria-live="polite">{message}</div> }
}
