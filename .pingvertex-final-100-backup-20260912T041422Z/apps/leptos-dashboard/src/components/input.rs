use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Input(
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] name: Option<String>,
    #[prop(optional, into)] input_type: Option<String>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional, into)] value: Option<String>,
    #[prop(optional)] required: bool,
    #[prop(optional)] disabled: bool,
    #[prop(optional, into)] autocomplete: Option<String>,
    #[prop(optional, into)] aria_invalid: Option<String>,
    #[prop(optional)] on_input: Option<Callback<String>>,
) -> impl IntoView {
    let input_type = input_type.unwrap_or_else(|| "text".into());
    view! {
        <input
            id=id
            name=name
            type=input_type
            placeholder=placeholder
            prop:value=value.unwrap_or_default()
            required=required
            disabled=disabled
            autocomplete=autocomplete
            aria-invalid=aria_invalid
            on:input=move |ev| {
                if let Some(cb) = on_input.as_ref() {
                    cb.run(event_target_value(&ev));
                }
            }
        />
    }
}
