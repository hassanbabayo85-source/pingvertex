use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Button(
    #[prop(optional, into)] class: Option<String>,
    #[prop(optional, into)] disabled: Option<bool>,
    #[prop(optional, into)] button_type: Option<String>,
    #[prop(optional, into)] aria_label: Option<String>,
    children: Children,
) -> impl IntoView {
    let class = class.unwrap_or_else(|| "btn".to_string());
    let disabled = disabled.unwrap_or(false);
    let button_type = button_type.unwrap_or_else(|| "button".into());
    view! {
        <button class=class type=button_type disabled=disabled aria-label=aria_label>
            {children()}
        </button>
    }
}
