use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn ErrorState(
    #[prop(into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional)] on_retry: Option<Callback<()>>,
) -> impl IntoView {
    let desc = description.unwrap_or_default();
    let retry_button = match on_retry {
        Some(cb) => view! {
            <button class="btn" on:click=move |_| cb.run(())>
                "Retry"
            </button>
        }
        .into_any(),
        None => ().into_any(),
    };

    view! {
        <div class="error-state">
            <h3 class="error-state-title">{title}</h3>
            {move || {
                if desc.is_empty() {
                    ().into_any()
                } else {
                    view! { <p class="error-state-description">{desc.clone()}</p> }.into_any()
                }
            }}
            {retry_button}
        </div>
    }
}
