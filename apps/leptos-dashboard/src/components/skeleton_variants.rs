use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Skeleton(#[prop(optional, into)] variant: Option<String>) -> impl IntoView {
    let variant = variant.unwrap_or_else(|| "line".into());
    let class = format!("skeleton skeleton-{variant}");
    view! { <div class=class aria-hidden="true"></div> }
}
