use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Dropdown(#[prop(into)] label: String, children: Children) -> impl IntoView {
    view! {
        <details class="dropdown">
            <summary class="dropdown-trigger">{label}</summary>
            <div class="dropdown-menu">{children()}</div>
        </details>
    }
}
