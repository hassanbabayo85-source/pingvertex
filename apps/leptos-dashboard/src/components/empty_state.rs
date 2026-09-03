use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn EmptyState(
    #[prop(into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let desc = description.unwrap_or_default();
    let actions = match children {
        Some(c) => view! { <div class="empty-state-actions">{c()}</div> }.into_any(),
        None => ().into_any(),
    };

    view! {
        <div class="empty-state">
            <h3 class="empty-state-title">{title}</h3>
            {move || {
                if desc.is_empty() {
                    ().into_any()
                } else {
                    view! { <p class="empty-state-description">{desc.clone()}</p> }.into_any()
                }
            }}
            {actions}
        </div>
    }
}
