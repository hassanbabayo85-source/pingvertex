use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Select(
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional, into)] value: Option<String>,
    #[prop(optional)] required: bool,
    #[prop(optional)] disabled: bool,
    children: Children,
    #[prop(optional)] on_change: Option<Callback<String>>,
) -> impl IntoView {
    view! {
        <select
            id=id
            name=name
            prop:value=value.unwrap_or_default()
            required=required
            disabled=disabled
            on:change=move |ev| {
                if let Some(cb) = on_change.as_ref() {
                    cb.run(event_target_value(&ev));
                }
            }
        >
            {children()}
        </select>
    }
}
