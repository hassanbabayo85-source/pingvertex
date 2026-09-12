use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn CommandPalette(
    #[prop(optional, into)] placeholder: Option<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="command-palette" role="dialog" aria-label="Command palette">
            <input type="search" placeholder=placeholder.unwrap_or_else(|| "Search commands…".into()) aria-label="Search commands"/>
            <div class="command-palette-results">{children()}</div>
        </div>
    }
}
