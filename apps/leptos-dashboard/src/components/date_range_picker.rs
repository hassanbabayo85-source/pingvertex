use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn DateRangePicker(
    #[prop(optional, into)] start: Option<String>,
    #[prop(optional, into)] end: Option<String>,
    #[prop(optional)] on_start_change: Option<Callback<String>>,
    #[prop(optional)] on_end_change: Option<Callback<String>>,
) -> impl IntoView {
    view! {
        <div class="date-range-picker">
            <label>
                <span>"From"</span>
                <input
                    type="datetime-local"
                    prop:value=start.unwrap_or_default()
                    on:input=move |e| {
                        if let Some(cb) = on_start_change.as_ref() {
                            cb.run(event_target_value(&e));
                        }
                    }
                />
            </label>
            <label>
                <span>"To"</span>
                <input
                    type="datetime-local"
                    prop:value=end.unwrap_or_default()
                    on:input=move |e| {
                        if let Some(cb) = on_end_change.as_ref() {
                            cb.run(event_target_value(&e));
                        }
                    }
                />
            </label>
        </div>
    }
}
