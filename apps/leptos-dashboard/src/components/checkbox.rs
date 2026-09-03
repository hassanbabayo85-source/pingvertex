use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Checkbox(
    #[prop(into)] label: String,
    #[prop(optional)] checked: bool,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] required: bool,
    #[prop(optional)]
    on_change: Option<Callback<bool>>,
) -> impl IntoView {
    view! {
        <label class="check-row">
            <input
                type="checkbox"
                prop:checked=checked
                disabled=disabled
                required=required
                on:change=move |ev| {
                    if let Some(cb) = on_change.as_ref() {
                        cb.run(event_target::<web_sys::HtmlInputElement>(&ev).checked());
                    }
                }
            />
            <span>{label}</span>
        </label>
    }
}
