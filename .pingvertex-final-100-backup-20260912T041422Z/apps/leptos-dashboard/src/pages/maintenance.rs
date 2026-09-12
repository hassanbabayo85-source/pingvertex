use crate::api::{advanced_monitoring as api, monitoring};
use leptos::prelude::*;
use leptos::task::spawn_local;
fn iso(value: String) -> String {
    js_sys::Date::new(&wasm_bindgen::JsValue::from_str(&value))
        .to_iso_string()
        .as_string()
        .unwrap_or(value)
}
#[component]
pub fn MaintenancePage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let selected = RwSignal::new(None::<String>);
    let windows = RwSignal::new(Vec::<api::MaintenanceWindow>::new());
    let start = RwSignal::new(String::new());
    let end = RwSignal::new(String::new());
    let reason = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        spawn_local(async move {
            match monitoring::fetch_monitors().await {
                Ok(v) => monitors.set(v),
                Err(e) => error.set(Some(e)),
            }
        })
    });
    Effect::new(move |_| {
        if let Some(id) = selected.get() {
            let id = id.clone();
            spawn_local(async move {
                match api::list_maintenance(&id).await {
                    Ok(v) => windows.set(v),
                    Err(e) => error.set(Some(e.to_string())),
                }
            })
        }
    });
    let create = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let Some(id) = selected.get() else { return };
        let req = monitoring::MaintenanceWindowPayload {
            starts_at: iso(start.get()),
            ends_at: iso(end.get()),
            reason: reason.get(),
        };
        spawn_local(async move {
            match api::create_maintenance(&id, &req).await {
                Ok(v) => {
                    windows.update(|w| w.insert(0, v));
                    notice.set(Some("Maintenance window created.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Maintenance windows"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|selected.set(event_target_value(&e).into())><option value="">"Select monitor"</option>{move||monitors.get().into_iter().map(|m|view!{<option value={m.id}>{m.name}</option>}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Create window"</h2><form on:submit=create class="stack-form"><label>"Starts"<input type="datetime-local" prop:value=start on:input=move|e|start.set(event_target_value(&e)) required/></label><label>"Ends"<input type="datetime-local" prop:value=end on:input=move|e|end.set(event_target_value(&e)) required/></label><label>"Reason"<textarea prop:value=reason on:input=move|e|reason.set(event_target_value(&e)) maxlength="500"/></label><button class="btn btn-primary">"Create"</button></form><h2>"Configured windows"</h2>{move||windows.get().into_iter().map(|w|view!{<div class="list-row"><div><strong>{w.reason}</strong><div class="muted">{w.starts_at}  ->  {w.ends_at}</div></div></div>}).collect_view()}</section>})}<p class="muted">"Maintenance windows suppress notifications while checks continue to run and incidents remain recorded."</p></div>}
}
