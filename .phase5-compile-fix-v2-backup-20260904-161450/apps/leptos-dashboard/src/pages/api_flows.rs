use crate::api::{developer, monitoring};
use crate::components::confirm_dialog::ConfirmDialog;
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

fn flow_steps_to_drafts(steps: &[developer::ApiFlowStep]) -> Vec<StepDraft> {
    steps.iter().map(|s| StepDraft {
        name: s.name.clone(),
        method: s.method.clone(),
        url: s.url_template.clone(),
        headers: s.headers.clone(),
        body: s.body.clone(),
        extract: s.extract.iter().map(|x| match x {
            developer::Extraction::StatusCode => developer::ExtractionRequest::StatusCode,
            developer::Extraction::Header { name } => developer::ExtractionRequest::Header { name: name.clone() },
            developer::Extraction::JsonPath { path } => developer::ExtractionRequest::JsonPath { path: path.clone() },
            developer::Extraction::Body => developer::ExtractionRequest::Body,
        }).collect(),
        assertions: s.assertions.iter().map(|x| developer::AssertionRequest {
            response_part: x.response_part.clone(),
            operator: x.operator.clone(),
            target: x.target.clone(),
            expected_value: x.expected_value.clone(),
        }).collect(),
    }).collect()
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
    let pending_delete = RwSignal::new(None::<(String, String)>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

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
        } else {
            flows.set(Vec::new());
        }
    });

    let add = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let n = step_name.get().trim().to_string();
        let u = step_url.get().trim().to_string();
        if n.is_empty() || u.is_empty() {
            error.set(Some("Step name and URL are required.".into()));
            return;
        }
        steps.update(|v| v.push(StepDraft {
            name: n,
            method: step_method.get(),
            url: u,
            headers: Vec::new(),
            body: None,
            extract: Vec::new(),
            assertions: Vec::new(),
        }));
        step_name.set(String::new());
        step_url.set(String::new());
    };

    let save = move |ev: leptos::ev::MouseEvent| {
        ev.prevent_default();
        let Some(mid) = selected.get() else { return };
        if steps.get().is_empty() {
            error.set(Some("Add at least one step.".into()));
            return;
        }
        let payload = developer::CreateApiFlowRequest {
            steps: steps.get().iter().enumerate().map(|(i, s)| developer::ApiFlowStepRequest {
                order: (i + 1) as u16,
                name: s.name.clone(),
                method: s.method.clone(),
                url_template: s.url.clone(),
                headers: s.headers.clone(),
                body: s.body.clone(),
                extract: s.extract.clone(),
                assertions: s.assertions.clone(),
            }).collect(),
        };
        let mid2 = mid.clone();
        busy.set(true);
        spawn_local(async move {
            let result = match editing.get_untracked() {
                Some(flow_id) => developer::update_flow(&mid2, &flow_id, &payload).await,
                None => developer::create_flow(&mid2, &payload).await,
            };
            match result {
                Ok(_) => {
                    editing.set(None);
                    steps.set(Vec::new());
                    notice.set(Some("API flow saved.".into()));
                    match developer::list_flows(&mid2).await {
                        Ok(v) => flows.set(v),
                        Err(e) => error.set(Some(e.to_string())),
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Developer"</span><h1>"API flows"</h1></div></header>
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            <section class="card">
                <label>"Monitor"
                    <select prop:value=move || selected.get().unwrap_or_default() on:change=move |e| {
                        let v = event_target_value(&e);
                        selected.set((!v.is_empty()).then_some(v));
                        steps.set(Vec::new());
                        editing.set(None);
                    }>
                        <option value="">"Select a monitor"</option>
                        {move || monitors.get().into_iter().map(|m| view! { <option value={m.id}>{m.name}</option> }).collect_view()}
                    </select>
                </label>
            </section>
            {move || selected.get().map(|_| view! {
                <section class="card">
                    <div class="section-heading"><h2>{move || if editing.get().is_some() {"Edit flow"} else {"Build flow"}}</h2><span class="muted">"Only backend-supported fields are sent."</span></div>
                    <form on:submit=add class="inline-form">
                        <input placeholder="Step name" maxlength="100" prop:value=step_name on:input=move |e| step_name.set(event_target_value(&e)) required/>
                        <select prop:value=step_method on:change=move |e| step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select>
                        <input type="url" placeholder="https://example.com/api" prop:value=step_url on:input=move |e| step_url.set(event_target_value(&e)) required/>
                        <button class="btn">"Add step"</button>
                    </form>
                    {move || if steps.get().is_empty() {
                        view! { <p class="muted">"No steps yet."</p> }.into_any()
                    } else {
                        view! { <ol>{move || steps.get().into_iter().enumerate().map(|(i,s)| view! {
                            <li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_| steps.update(|v| { if i < v.len() { v.remove(i); } })>"Remove"</button></li>
                        }).collect_view()}</ol> }.into_any()
                    }}
                    <div class="form-actions">
                        {move || editing.get().is_some().then(|| view! { <button class="btn" type="button" on:click=move |_| { editing.set(None); steps.set(Vec::new()); }>"Cancel edit"</button> })}
                        <button class="btn btn-primary" type="button" on:click=save disabled=move || busy.get() || steps.get().is_empty()>{move || if busy.get() {"Saving…"} else if editing.get().is_some() {"Save flow"} else {"Create flow"}}</button>
                    </div>
                </section>
            })}
            <section class="card">
                <div class="section-heading"><h2>"Existing flows"</h2><span class="muted">"Delete is permanent."</span></div>
                {move || if flows.get().is_empty() {
                    view! { <p class="muted">"No API flows for this monitor."</p> }.into_any()
                } else {
                    view! { <div class="list-stack">{move || flows.get().into_iter().map(|f| {
                        let id = f.id.clone();
                        let monitor_id = f.monitor_id.clone();
                        let loaded = flow_steps_to_drafts(&f.steps);
                        view! { <article class="list-row">
                            <div><strong>{format!("Flow {}",f.id)}</strong><div class="muted">{format!("{} step(s)",f.steps.len())}</div></div>
                            <div class="row-actions">
                                <button class="btn" on:click=move |_| { editing.set(Some(id.clone())); steps.set(loaded.clone()); }>"Edit"</button>
                                <button class="btn btn-danger" on:click=move |_| pending_delete.set(Some((monitor_id.clone(), id.clone())))>"Delete"</button>
                            </div>
                        </article> }
                    }).collect_view()}</div> }.into_any()
                }}
            </section>
            {move || pending_delete.get().map(|(mid, fid)| {
                let mid2 = mid.clone();
                let fid2 = fid.clone();
                view! { <ConfirmDialog
                    title="Delete API flow"
                    body="This permanently removes the flow and its steps."
                    on_confirm=Callback::new(move |_| {
                        let mid = mid2.clone(); let fid = fid2.clone();
                        busy.set(true);
                        spawn_local(async move {
                            match developer::delete_flow(&mid, &fid).await {
                                Ok(()) => {
                                    flows.update(|v| v.retain(|x| x.id != fid));
                                    notice.set(Some("API flow deleted.".into()));
                                }
                                Err(e) => error.set(Some(e.to_string())),
                            }
                            pending_delete.set(None);
                            busy.set(false);
                        });
                    })
                    on_cancel=Callback::new(move |_| pending_delete.set(None))
                /> }
            })}
        </div>
    }
}
