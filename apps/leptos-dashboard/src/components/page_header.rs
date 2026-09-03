use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn PageHeader(
    #[prop(into)] title: String,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let actions = match children {
        Some(c) => view! { <div class="page-header-actions">{c()}</div> }.into_any(),
        None => ().into_any(),
    };

    view! {
        <div class="page-header">
            <h1>{title}</h1>
            {actions}
        </div>
    }
}
