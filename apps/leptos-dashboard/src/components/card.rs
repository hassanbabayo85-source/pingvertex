use leptos::prelude::*;

#[component]
pub fn Card(#[prop(optional, into)] class: Option<String>, children: Children) -> impl IntoView {
    let class = class.unwrap_or_else(|| "card".to_string());
    view! {
        <div class=class>
            {children()}
        </div>
    }
}
