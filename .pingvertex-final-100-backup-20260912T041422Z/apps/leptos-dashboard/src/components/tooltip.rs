use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Tooltip(#[prop(into)] text: String, children: Children) -> impl IntoView {
    view! {
        <span class="tooltip" tabindex="0" aria-describedby="tooltip-content">
            {children()}
            <span class="tooltip-content" role="tooltip" id="tooltip-content">{text}</span>
        </span>
    }
}
