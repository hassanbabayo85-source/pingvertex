use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn SearchInput(
    #[prop(into)] value: String,
    on_input: Callback<String>,
    #[prop(optional, into)] placeholder: Option<String>,
) -> impl IntoView {
    let placeholder = placeholder.unwrap_or_else(|| "Search...".to_string());

    view! {
        <div class="search-input">
            <input
                type="search"
                value=value.clone()
                placeholder=placeholder
                on:input=move |ev| {
                    let input_val = event_target_value(&ev);
                    on_input.run(input_val);
                }
            />
        </div>
    }
}
