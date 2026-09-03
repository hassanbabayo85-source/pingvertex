use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn FormField(
    #[prop(into)] label: String,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional)] required: bool,
    #[prop(optional, into)] help: Option<String>,
    #[prop(optional, into)] error: Option<String>,
    children: Children,
) -> impl IntoView {
    let help_id = id.as_ref().map(|v| format!("{v}-help"));
    let error_id = id.as_ref().map(|v| format!("{v}-error"));

    view! {
        <div class="form-field">
            <label class="form-field-label" for=id.clone()>
                {label}
                {required.then(|| view! {
                    <span class="required-indicator" aria-hidden="true">"*"</span>
                })}
            </label>
            <div class="form-field-control">
                {children()}
            </div>
            {help.map(|text| view! {
                <p class="form-help" id=help_id.clone()>{text}</p>
            })}
            {error.map(|text| view! {
                <p class="form-error-message" id=error_id.clone() role="alert">{text}</p>
            })}
        </div>
    }
}
