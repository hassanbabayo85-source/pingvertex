use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Tabs(children: Children) -> impl IntoView {
    view! { <div class="tabs" role="tablist">{children()}</div> }
}
