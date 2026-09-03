use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn FilterBar(children: Children) -> impl IntoView {
    view! {
        <div class="filter-bar">
            {children()}
        </div>
    }
}
