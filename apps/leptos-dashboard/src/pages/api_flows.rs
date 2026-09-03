use crate::api::{developer, monitoring};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone)]
struct StepDraft {
    name: String,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    extract: Vec<developer::ExtractionRequest>,
    assertions: Vec<developer::AssertionRequest>,
}
#[component]
pub fn ApiFlowsPage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let selected = RwSignal::new(None::<String>);
    let flows = RwSignal::new(Vec::<developer::ApiFlow>::new());
    let steps = RwSignal::new(Vec::<StepDraft>::new());
    let step_name = RwSignal::new(String::new());
    let step_url = RwSignal::new(String::new());
    let step_method = RwSignal::new("GET".into());
    let editing = RwSignal::new(None::<String>);
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
                match developer::list_flows(&id).await {
                    Ok(v) => flows.set(v),
                    Err(e) => error.set(Some(e.to_string())),
                }
            })
        }
    });
    let add = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let n = step_name.get();
        let u = step_url.get();
        if n.trim().is_empty() || u.trim().is_empty() {
            error.set(Some("Step name and URL are required.".into()));
            return;
        }
        steps.update(|v| {
            v.push(StepDraft {
                name: n,
                method: step_method.get(),
                url: u,
                headers: Vec::new(),
                body: None,
                extract: Vec::new(),
                assertions: Vec::new(),
            })
        });
        step_name.set(String::new());
        step_url.set(String::new());
    };
    let save = move |_| {
        let Some(mid) = selected.get() else { return };
        let payload = developer::CreateApiFlowRequest {
            steps: steps
                .get()
                .iter()
                .enumerate()
                .map(|(i, s)| developer::ApiFlowStepRequest {
                    order: (i + 1) as u16,
                    name: s.name.clone(),
                    method: s.method.clone(),
                    url_template: s.url.clone(),
                    headers: s.headers.clone(),
                    body: s.body.clone(),
                    extract: s.extract.clone(),
                    assertions: s.assertions.clone(),
                })
                .collect(),
        };
        let mid2 = mid.clone();
        spawn_local(async move {
            match match editing.get_untracked() {
                Some(flow_id) => developer::update_flow(&mid2, &flow_id, &payload).await,
                None => developer::create_flow(&mid2, &payload).await,
            } {
                Ok(_) => {
                    editing.set(None);
                    notice.set(Some("API flow saved.".into()));
                    match developer::list_flows(&mid2).await {
                        Ok(v) => flows.set(v),
                        Err(e) => error.set(Some(e.to_string())),
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"API flows"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|{let v=event_target_value(&e);selected.set((!v.is_empty()).then_some(v));steps.set(Vec::new())}><option value="">"Select a monitor"</option>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<option value={id}>{m.name}</option>}}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Build flow"</h2><form on:submit=add class="inline-form"><input placeholder="Step name" prop:value=step_name on:input=move|e|step_name.set(event_target_value(&e)) required/><select prop:value=step_method on:change=move|e|step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select><input placeholder="https://example.com/api" prop:value=step_url on:input=move|e|step_url.set(event_target_value(&e)) required/><button class="btn">"Add step"</button></form><ol>{move||steps.get().into_iter().enumerate().map(|(i,s)|view!{<li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_|steps.update(|v|{if i<v.len(){v.remove(i);}})>"Remove"</button></li>}).collect_view()}</ol><button class="btn btn-primary" on:click=save disabled=move||steps.get().is_empty()>{move||if editing.get().is_some(){"Save flow"}else{"Create flow"}}</button></section>})}<section class="card"><h2>"Existing flows"</h2>{move||flows.get().into_iter().map(|f|{let id=f.id.clone();let loaded=f.steps.clone();view!{<article class="list-row"><div><strong>{format!("Flow {}",f.id)}</strong><div class="muted">{format!("{} step(s)",f.steps.len())}</div></div><button class="btn" on:click=move |_|{editing.set(Some(id.clone()));steps.set(loaded.iter().map(|s|StepDraft{name:s.name.clone(),method:s.method.clone(),url:s.url_template.clone(),headers:s.headers.clone(),body:s.body.clone(),extract:s.extract.iter().map(|x|match x{developer::Extraction::StatusCode=>developer::ExtractionRequest::StatusCode,developer::Extraction::Header{name}=>developer::ExtractionRequest::Header{name:name.clone()},developer::Extraction::JsonPath{path}=>developer::ExtractionRequest::JsonPath{path:path.clone()},developer::Extraction::Body=>developer::ExtractionRequest::Body}).collect(),assertions:s.assertions.iter().map(|x|developer::AssertionRequest{response_part:x.response_part.clone(),operator:x.operator.clone(),target:x.target.clone(),expected_value:x.expected_value.clone()}).collect()}).collect());}>"Edit"</button></article>}}).collect_view()}</section><p class="muted">"Flow URLs, headers and assertions are validated by the backend. This UI intentionally sends only fields supported by the current contract."</p></div>}
}
