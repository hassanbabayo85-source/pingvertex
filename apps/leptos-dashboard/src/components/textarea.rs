use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Textarea(
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional, into)] value: Option<String>,
    #[prop(optional)] required: bool,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] rows: Option<u32>,
    #[prop(optional)] on_input: Option<Callback<String>>,
) -> impl IntoView {
    view! {
        <textarea
            id=id
            name=name
            placeholder=placeholder
            prop:value=value.unwrap_or_default()
            required=required
            disabled=disabled
            rows=rows
            on:input=move |ev| {
                if let Some(cb) = on_input.as_ref() {
                    cb.run(event_target_value(&ev));
                }
            }
        ></textarea>
    }
}
