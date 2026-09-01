use leptos::prelude::*;

#[component]
pub fn Button(
    #[prop(optional, into)] class: Option<String>,
    #[prop(optional, into)] disabled: Option<bool>,
    children: Children,
) -> impl IntoView {
    let class = class.unwrap_or_else(|| "btn".to_string());
    let disabled = disabled.unwrap_or(false);
    view! {
        <button class=class disabled=disabled>
            {children()}
        </button>
    }
}
