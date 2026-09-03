use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Switch(
    #[prop(into)] label: String,
    #[prop(optional)] checked: bool,
    #[prop(optional)] disabled: bool,
    #[prop(optional)] on_change: Option<Callback<bool>>,
) -> impl IntoView {
    view! {
        <label class="switch-row">
            <input
                class="switch-input"
                type="checkbox"
                role="switch"
                prop:checked=checked
                disabled=disabled
                on:change=move |ev| {
                    if let Some(cb) = on_change.as_ref() {
                        cb.run(event_target::<web_sys::HtmlInputElement>(&ev).checked());
                    }
                }
            />
            <span class="switch-track" aria-hidden="true"><span class="switch-thumb"></span></span>
            <span>{label}</span>
        </label>
    }
}
