abubakar-hassan-babayo@AHBYPC:~/uptime-monitor$ cd ~/uptime-monitor

for f in \
  apps/leptos-dashboard/src/pages/status_pages.rs \
  apps/leptos-dashboard/src/pages/api_flows.rs \
  apps/leptos-dashboard/src/pages/escalation.rs \
  apps/leptos-dashboard/src/pages/monitor_form.rs \
  apps/leptos-dashboard/src/pages/api_keys.rs \
  apps/leptos-dashboard/src/pages/billing.rs \
  apps/leptos-dashboard/src/pages/pricing.rs \
  apps/leptos-dashboard/src/pages/support.rs \
  apps/leptos-dashboard/src/pages/team.rs
do
  echo
  echo "==================== $f ===================="
  cat "$f"
done

==================== apps/leptos-dashboard/src/pages/status_pages.rs ====================
use crate::api::{monitoring, status_pages as api};
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;

#[component]
pub fn StatusPagesPage() -> impl IntoView {
    let pages = RwSignal::new(Vec::<api::StatusPage>::new());
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let selected = RwSignal::new(None::<String>);
    let pending_delete = RwSignal::new(None::<String>);

    let reload = move || {
        spawn_local(async move {
            let p = api::list().await;
            let m = api::monitors().await;
            match (p, m) {
                (Ok(p), Ok(m)) => {
                    pages.set(p);
                    monitors.set(m);
                    error.set(None);
                }
                (Err(e), _) | (_, Err(e)) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };
    reload();

    view! {
        <div class="feature-page">
            <header class="page-header"><h1>"Status pages"</h1></header>
            {move || notice.get().map(|v| view!{<div class="success">{v}</div>})}
            {move || error.get().map(|v| view!{<div class="error">{v}</div>})}
            {move || if loading.get() { view!{<div class="loading">"Loading status pages…"</div>}.into_any() } else {
                view! {
                    <div class="feature-grid">
                        <section class="card"><h2>"Create status page"</h2><StatusPageForm monitors=monitors on_created=Callback::new(move |p| { pages.update(|v| v.push(p)); notice.set(Some("Status page created.".into())); }) /></section>
                        <section class="card"><h2>"Your status pages"</h2>
                            {move || pages.get().into_iter().map(|p| {
                                let id=p.id.clone(); let id2=id.clone(); let id3=id.clone(); let nav=navigate.clone();
                                view!{<article class="list-row"><div><strong>{p.name.clone()}</strong><div class="muted">{"/status/"}{p.subdomain.clone()}</div><div class="muted">{format!("{} monitor(s)", p.monitor_ids.len())}</div></div><div class="row-actions"><button class="btn" on:click=move |_| {selected.set(Some(id.clone()));}>"Manage"</button><button class="btn btn-danger" on:click=move |_| pending_delete.set(Some(id2.clone()))>"Delete"</button></div></article>}
                            }).collect_view()}
                            {move || if pages.get().is_empty() { view!{<p class="muted">"No status pages yet."</p>}.into_any() } else { ().into_any() }}
                        </section>
                    </div>
                    {move || selected.get().and_then(|id| pages.get().into_iter().find(|p| p.id==id)).map(|p| view!{<StatusPageEditor page=p monitors=monitors on_saved=Callback::new(move |p| { pages.update(|v| { if let Some(old)=v.iter_mut().find(|x| x.id==p.id) {*old=p;} }); notice.set(Some("Status page updated.".into())); }) on_close=Callback::new(move |_| selected.set(None)) />})}
                }.into_any()
            }}
            {move || pending_delete.get().map(|id| { let id2=id.clone(); view!{<ConfirmDialog title="Delete status page" body="This permanently removes the status page." on_confirm=Callback::new(move |_| { let id=id2.clone(); spawn_local(async move { match api::delete(&id).await { Ok(())=>{pages.update(|v|v.retain(|p|p.id!=id)); notice.set(Some("Status page deleted.".into()));}, Err(e)=>error.set(Some(e.to_string())) } pending_delete.set(None); }); }) on_cancel=Callback::new(move |_| pending_delete.set(None))/>}})}
        </div>
    }
}

#[component]
fn StatusPageForm(
    monitors: RwSignal<Vec<monitoring::Monitor>>,
    on_created: Callback<api::StatusPage>,
) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let subdomain = RwSignal::new(String::new());
    let selected = RwSignal::new(Vec::<String>::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let req = api::CreateStatusPageRequest {
            name: name.get(),
            subdomain: subdomain.get(),
            monitor_ids: selected.get(),
        };
        busy.set(true);
        spawn_local(async move {
            match api::create(&req).await {
                Ok(p) => {
                    on_created.run(p);
                    name.set(String::new());
                    subdomain.set(String::new());
                    selected.set(Vec::new());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    view! {<form on:submit=submit class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Subdomain"<input placeholder="status.example" prop:value=subdomain on:input=move|e|subdomain.set(event_target_value(&e)) required/></label><fieldset><legend>"Monitors"</legend>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<label class="check-row"><input type="checkbox" on:change=move|e|{let checked=event_target::<web_sys::HtmlInputElement>(&e).checked();selected.update(|v|if checked{if !v.contains(&id){v.push(id.clone())}}else{v.retain(|x|x!=&id)});}/>{m.name}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get() type="submit">"Create"</button>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</form>}
}

#[component]
fn StatusPageEditor(
    page: api::StatusPage,
    monitors: RwSignal<Vec<monitoring::Monitor>>,
    on_saved: Callback<api::StatusPage>,
    on_close: Callback<()>,
) -> impl IntoView {
    let domain_initial = page.custom_domain.clone();
    let domain_verified = page.custom_domain_verified;
    let name = RwSignal::new(page.name.clone());
    let color = RwSignal::new(page.primary_color.clone().unwrap_or_default());
    let selected = RwSignal::new(page.monitor_ids.clone());
    let domain = RwSignal::new(domain_initial.clone().unwrap_or_default());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = page.id.clone();
    let id2 = id.clone();
    let id3 = id.clone();
    let logo_url = page.logo_url.clone();
    let save_callback = on_saved.clone();
    let domain_callback = on_saved.clone();
    let remove_callback = on_saved.clone();
    let upload_callback = on_saved.clone();
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let req = api::UpdateStatusPageRequest {
            name: name.get(),
            monitor_ids: selected.get(),
            logo_url: logo_url.clone(),
            primary_color: if color.get().trim().is_empty() {
                None
            } else {
                Some(color.get())
            },
        };
        busy.set(true);
        spawn_local(async move {
            match api::update(&id, &req).await {
                Ok(p) => save_callback.run(p),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let request_domain = move || {
        let d = domain.get();
        if d.trim().is_empty() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match api::request_custom_domain(&id2, &d).await {
                Ok(p) => domain_callback.run(p),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let remove_domain = move || {
        busy.set(true);
        spawn_local(async move {
            match api::remove_custom_domain(&id3).await {
                Ok(p) => remove_callback.run(p),
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let upload_id = page.id.clone();
    let upload = move |ev: leptos::ev::Event| {
        let input = event_target::<HtmlInputElement>(&ev);
        if let Some(file) = input.files().and_then(|f| f.get(0)) {
            let file_name = file.name();
            let mime = file.type_();
            let id = upload_id.clone();
            busy.set(true);
            spawn_local(async move {
                match JsFuture::from(file.array_buffer()).await {
                    Ok(buf) => {
                        let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                        match api::upload_logo(&id, bytes, &file_name, &mime).await {
                            Ok(p) => upload_callback.run(p),
                            Err(e) => error.set(Some(e.to_string())),
                        }
                    }
                    Err(_) => error.set(Some("Could not read the selected image.".into())),
                }
                busy.set(false);
            });
        }
    };
    view! {<section class="card feature-editor"><header class="page-header"><h2>"Manage status page"</h2><button class="btn" on:click=move |_|on_close.run(())>"Close"</button></header><form on:submit=save class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Primary color"<input type="text" placeholder="#111827" prop:value=color on:input=move|e|color.set(event_target_value(&e))/></label><fieldset><legend>"Monitors"</legend>{move||monitors.get().into_iter().map(|m|{let mid=m.id.clone();let checked=selected.get().contains(&mid);view!{<label class="check-row"><input type="checkbox" prop:checked=checked on:change=move|e|{let c=event_target::<HtmlInputElement>(&e).checked();selected.update(|v|if c{if !v.contains(&mid){v.push(mid.clone())}}else{v.retain(|x|x!=&mid)});}/>{m.name}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get() type="submit">"Save"</button></form><div class="subsection"><h3>"Logo"</h3><input type="file" accept="image/*" on:change=upload/><p class="muted">"The server validates the image and enforces its upload limit."</p></div><div class="subsection"><h3>"Custom domain"</h3><p>{move||if domain_verified{"Verified".to_string()}else if page.custom_domain.is_some(){"Pending verification".to_string()}else{"Not configured".to_string()}}</p><div class="inline-form"><input placeholder="status.example.com" prop:value=domain on:input=move|e|domain.set(event_target_value(&e))/><button class="btn" type="button" disabled=move||busy.get() on:click=move|_|request_domain()>"Request"</button><button class="btn btn-danger" type="button" disabled=move||busy.get() on:click=move|_|remove_domain()>"Remove"</button></div></div>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</section>}
}

==================== apps/leptos-dashboard/src/pages/api_flows.rs ====================
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
    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
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
    view! {<div class="feature-page"><header class="page-header"><h1>"API flows"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|{let v=event_target_value(&e);selected.set((!v.is_empty()).then_some(v));steps.set(Vec::new())}><option value="">"Select a monitor"</option>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<option value={id}>{m.name}</option>}}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Build flow"</h2><form on:submit=add class="inline-form"><input placeholder="Step name" prop:value=step_name on:input=move|e|step_name.set(event_target_value(&e)) required/><select prop:value=step_method on:change=move|e|step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select><input placeholder="https://example.com/api" prop:value=step_url on:input=move|e|step_url.set(event_target_value(&e)) required/><button class="btn">"Add step"</button></form><ol>{move||steps.get().into_iter().enumerate().map(|(i,s)|view!{<li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_|steps.update(|v|{if i<v.len(){v.remove(i)}})>"Remove"</button></li>}).collect_view()}</ol><button class="btn btn-primary" on:click=save disabled=move||steps.get().is_empty()>{move||if editing.get().is_some(){"Save flow"}else{"Create flow"}}</button></section>})}<section class="card"><h2>"Existing flows"</h2>{move||flows.get().into_iter().map(|f|{let id=f.id.clone();let loaded=f.steps.clone();view!{<article class="list-row"><div><strong>{format!("Flow {}",f.id)}</strong><div class="muted">{format!("{} step(s)",f.steps.len())}</div></div><button class="btn" on:click=move |_|{editing.set(Some(id.clone()));steps.set(loaded.iter().map(|s|StepDraft{name:s.name.clone(),method:s.method.clone(),url:s.url_template.clone(),headers:s.headers.clone(),body:s.body.clone(),extract:s.extract.iter().map(|x|match x{developer::Extraction::StatusCode=>developer::ExtractionRequest::StatusCode,developer::Extraction::Header{name}=>developer::ExtractionRequest::Header{name:name.clone()},developer::Extraction::JsonPath{path}=>developer::ExtractionRequest::JsonPath{path:path.clone()},developer::Extraction::Body=>developer::ExtractionRequest::Body}).collect(),assertions:s.assertions.iter().map(|x|developer::AssertionRequest{response_part:x.response_part.clone(),operator:x.operator.clone(),target:x.target.clone(),expected_value:x.expected_value.clone()}).collect()}).collect());}>"Edit"</button></article>}}).collect_view()}</section><p class="muted">"Flow URLs, headers and assertions are validated by the backend. This UI intentionally sends only fields supported by the current contract."</p></div>}
}

==================== apps/leptos-dashboard/src/pages/escalation.rs ====================
use crate::api::{advanced_monitoring as api, monitoring};
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn EscalationPage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let channels = RwSignal::new(Vec::<monitoring::NotificationChannel>::new());
    let policies = RwSignal::new(Vec::<api::EscalationPolicy>::new());
    let selected_monitor = RwSignal::new(String::new());
    let selected_channel = RwSignal::new(String::new());
    let delay = RwSignal::new(String::from("5"));
    let steps = RwSignal::new(Vec::<api::EscalationStepPayload>::new());
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
        spawn_local(async move {
            match api::list_escalations().await {
                Ok(v) => policies.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });
    Effect::new(move |_| {
        let id = selected_monitor.get();
        if id.is_empty() {
            channels.set(Vec::new());
            return;
        }
        spawn_local(async move {
            match monitoring::list_notification_channels(&id).await {
                Ok(v) => channels.set(v),
                Err(e) => error.set(Some(e)),
            }
        })
    });
    let add_step = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let c = selected_channel.get();
        if c.is_empty() {
            error.set(Some("Select a notification channel.".into()));
            return;
        }
        steps.update(|v| {
            v.push(api::EscalationStepPayload {
                delay_minutes: delay.get().parse().unwrap_or(0),
                channel_id: c,
            })
        });
    };
    let save = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let req = api::EscalationPolicyPayload {
            monitor_id: if selected_monitor.get().is_empty() {
                None
            } else {
                Some(selected_monitor.get())
            },
            steps: steps.get(),
        };
        let current = editing.get();
        spawn_local(async move {
            let r = match current {
                Some(id) => api::update_escalation(&id, &req).await,
                None => api::create_escalation(&req).await,
            };
            match r {
                Ok(v) => {
                    policies.update(|p| {
                        if let Some(i) = p.iter().position(|x| x.id == v.id) {
                            p[i] = v
                        } else {
                            p.insert(0, v)
                        }
                    });
                    notice.set(Some("Escalation policy saved.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Escalation policies"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<section class="card"><h2>"Create / edit policy"</h2><form on:submit=save class="stack-form"><label>"Monitor (optional)"<select prop:value=selected_monitor on:change=move|e|{selected_monitor.set(event_target_value(&e));steps.set(Vec::new())}><option value="">"Global"</option>{move||monitors.get().into_iter().map(|m|view!{<option value={m.id}>{m.name}</option>}).collect_view()}</select></label><div class="inline-form"><select prop:value=selected_channel on:change=move|e|selected_channel.set(event_target_value(&e))><option value="">"Channel"</option>{move||channels.get().into_iter().map(|c|view!{<option value={c.id}>{format!("{} — {}",c.kind,c.target)}</option>}).collect_view()}</select><input type="number" min="0" prop:value=delay on:input=move|e|delay.set(event_target_value(&e))/><button class="btn" on:click=add_step>"Add step"</button></div><ol>{move||steps.get().into_iter().map(|s|view!{<li>{format!("{} min → {}",s.delay_minutes,s.channel_id)}</li>}).collect_view()}</ol><button class="btn btn-primary" disabled=move||steps.get().is_empty()>"Save policy"</button></form></section><section class="card"><h2>"Existing policies"</h2>{move||policies.get().into_iter().map(|p|{let id=p.id.clone();view!{<div class="list-row"><div><strong>{p.monitor_id.clone().unwrap_or_else(||"Global".into())}</strong><div class="muted">{format!("{} step(s)",p.steps.len())}</div></div><button class="btn" on:click=move |_|{editing.set(Some(id.clone()));selected_monitor.set(p.monitor_id.clone().unwrap_or_default());steps.set(p.steps.clone());}>"Edit"</button></div>}}).collect_view()}</section></div>}
}

==================== apps/leptos-dashboard/src/pages/monitor_form.rs ====================
use crate::api::monitoring::{self, MonitorPayload};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};
#[component]
pub fn MonitorFormPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.get().get("id").unwrap_or_default();
    let edit = move || !id().is_empty();
    let name = RwSignal::new(String::new());
    let kind = RwSignal::new("http".into());
    let url = RwSignal::new(String::new());
    let method = RwSignal::new("GET".into());
    let expected = RwSignal::new("200".into());
    let tcp_port = RwSignal::new(String::new());
    let dns_type = RwSignal::new("A".into());
    let dns_value = RwSignal::new(String::new());
    let hb_interval = RwSignal::new("60".into());
    let hb_grace = RwSignal::new("60".into());
    let interval = RwSignal::new("60".into());
    let timeout = RwSignal::new("10".into());
    let public = RwSignal::new(false);
    let ssl = RwSignal::new(true);
    let ssl_days = RwSignal::new("14".into());
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();
    Effect::new(move |_| {
        if edit() {
            let mid = id();
            spawn_local(async move {
                match monitoring::get_monitor(&mid).await {
                    Ok(m) => {
                        name.set(m.name);
                        kind.set(m.kind);
                        url.set(m.url.unwrap_or_default());
                        method.set("GET".into());
                        expected.set(m.expected_status.to_string());
                        tcp_port.set(m.tcp_port.map(|v| v.to_string()).unwrap_or_default());
                        dns_type.set(m.dns_record_type.unwrap_or_else(|| "A".into()));
                        dns_value.set(m.dns_expected_value.unwrap_or_default());
                        hb_interval.set(m.heartbeat_interval_seconds.unwrap_or(60).to_string());
                        hb_grace.set(m.heartbeat_grace_period_seconds.unwrap_or(60).to_string());
                        interval.set(m.interval_seconds.to_string());
                        timeout.set(m.timeout_seconds.to_string());
                        public.set(m.is_public);
                        ssl.set(m.ssl_check_enabled);
                        ssl_days.set(m.ssl_expiry_warning_days.to_string());
                    }
                    Err(e) => error.set(Some(e)),
                }
            })
        }
    });
    let submit = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let payload = MonitorPayload {
            name: name.get(),
            kind: kind.get(),
            url: url.get(),
            method: method.get(),
            expected_status: expected.get().parse().unwrap_or(200),
            interval_seconds: interval.get().parse().unwrap_or(60),
            timeout_seconds: timeout.get().parse().unwrap_or(10),
            assertions: Vec::new(),
            is_public: public.get(),
            ssl_check_enabled: ssl.get(),
            ssl_expiry_warning_days: ssl_days.get().parse().unwrap_or(14),
            tcp_port: tcp_port.get().parse().ok(),
            dns_record_type: if kind.get() == "dns" {
                Some(dns_type.get())
            } else {
                None
            },
            dns_expected_value: if kind.get() == "dns" && !dns_value.get().is_empty() {
                Some(dns_value.get())
            } else {
                None
            },
            heartbeat_interval_seconds: if kind.get() == "heartbeat" {
                hb_interval.get().parse().ok()
            } else {
                None
            },
            heartbeat_grace_period_seconds: if kind.get() == "heartbeat" {
                hb_grace.get().parse().ok()
            } else {
                None
            },
        };
        let mid = id();
        spawn_local(async move {
            let r = if edit() {
                monitoring::update_monitor(&mid, &payload).await
            } else {
                monitoring::create_monitor(&payload).await
            };
            match r {
                Ok(m) => navigate(&format!("/monitors/{}", m.id), Default::default()),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>{move||if edit(){"Edit monitor"}else{"New monitor"}}</h1></header><form on:submit=submit class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Kind"<select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))><option value="http">"HTTP"</option><option value="tcp">"TCP"</option><option value="dns">"DNS"</option><option value="heartbeat">"Heartbeat"</option><option value="flow">"API flow"</option></select></label>{move||match kind.get().as_str(){"http"|"flow"=>view!{<><label>"URL"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"Method"<select prop:value=method on:change=move|e|method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select></label><label>"Expected status"<input type="number" min="100" max="599" prop:value=expected on:input=move|e|expected.set(event_target_value(&e))/></label></>.into_any()},"tcp"=>view!{<><label>"Host"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"TCP port"<input type="number" min="1" max="65535" prop:value=tcp_port on:input=move|e|tcp_port.set(event_target_value(&e)) required/></label></>.into_any()},"dns"=>view!{<><label>"DNS host"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"Record type"<input prop:value=dns_type on:input=move|e|dns_type.set(event_target_value(&e)) required/></label><label>"Expected value (optional)"<input prop:value=dns_value on:input=move|e|dns_value.set(event_target_value(&e))/></label></>.into_any()},"heartbeat"=>view!{<><label>"Heartbeat endpoint"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"Heartbeat interval (seconds)"<input type="number" min="1" prop:value=hb_interval on:input=move|e|hb_interval.set(event_target_value(&e)) required/></label><label>"Grace period (seconds)"<input type="number" min="1" prop:value=hb_grace on:input=move|e|hb_grace.set(event_target_value(&e)) required/></label></>.into_any()},_=>().into_any()}}<label>"Check interval (seconds)"<input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e)) required/></label><label>"Timeout (seconds)"<input type="number" min="1" prop:value=timeout on:input=move|e|timeout.set(event_target_value(&e)) required/></label><label class="check-row"><input type="checkbox" prop:checked=public on:change=move|e|public.set(event_target::<web_sys::HtmlInputElement>(&e).checked())/>"Show on public status pages"</label><label class="check-row"><input type="checkbox" prop:checked=ssl on:change=move|e|ssl.set(event_target::<web_sys::HtmlInputElement>(&e).checked())/>"Check TLS certificate expiry"</label>{move||if ssl.get(){view!{<label>"Expiry warning (days)"<input type="number" min="1" prop:value=ssl_days on:input=move|e|ssl_days.set(event_target_value(&e))/></label>}.into_any()}else{().into_any()}}<button class="btn btn-primary" type="submit">{move||if edit(){"Save changes"}else{"Create monitor"}}</button>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</form></div>}
}

==================== apps/leptos-dashboard/src/pages/api_keys.rs ====================
use crate::api::developer;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role: &Role) -> bool {
    matches!(role, Role::Admin | Role::Owner)
}
#[component]
pub fn ApiKeysPage() -> impl IntoView {
    let auth = use_auth();
    let keys = RwSignal::new(Vec::<developer::ApiKey>::new());
    let name = RwSignal::new(String::new());
    let scopes = RwSignal::new(Vec::<String>::new());
    let error = RwSignal::new(None::<String>);
    let secret = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_manage(&u.role),
        _ => false,
    };
    let reload = move || {
        spawn_local(async move {
            match developer::list_keys().await {
                Ok(v) => keys.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    };
    reload();
    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let n = name.get();
        let s = scopes.get();
        busy.set(true);
        spawn_local(async move {
            match developer::create_key(&developer::CreateApiKeyRequest {
                name: n,
                scopes: s,
                expires_at: None,
            })
            .await
            {
                Ok(k) => {
                    secret.set(Some(k.key));
                    reload();
                    name.set(String::new());
                    scopes.set(Vec::new());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"API keys"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||secret.get().map(|s|view!{<div class="secret-box"><strong>"API key — copy it now"</strong><code>{s}</code><span class="muted">"It will not be shown again."</span></div>})}<section class="card"><h2>"Keys"</h2>{move||keys.get().into_iter().map(|k|{let id=k.id.clone();view!{<div class="list-row"><div><strong>{k.name}</strong><div class="muted"><code>{k.key_prefix}</code>  -  {k.scopes.join(", ")}</div></div>{move||if can(){view!{<button class="btn btn-danger" on:click=move |_|{let id=id.clone();spawn_local(async move{match developer::delete_key(&id).await{Ok(())=>keys.update(|v|v.retain(|x|x.id!=id)),Err(e)=>error.set(Some(e.to_string()))}})}>"Revoke"</button>}.into_any()}else{().into_any()}}</div>}}).collect_view()}</section>{move||if can(){view!{<section class="card"><h2>"Create key"</h2><form on:submit=create class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><fieldset><legend>"Scopes"</legend>{developer::API_KEY_SCOPES.iter().map(|scope|{let s=(*scope).to_string();view!{<label class="check-row"><input type="checkbox" on:change=move|e|{let checked=event_target::<web_sys::HtmlInputElement>(&e).checked();scopes.update(|v|if checked{if !v.contains(&s){v.push(s.clone())}}else{v.retain(|x|x!=&s)});}/>{*scope}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get()>"Create key"</button></form></section>}.into_any()}else{().into_any()}}<p class="muted">"The server enforces scope permissions. Secrets are never stored in browser storage."</p></div>}
}

==================== apps/leptos-dashboard/src/pages/billing.rs ====================
use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_bill(role: &Role) -> bool {
    matches!(role, Role::Owner)
}
#[component]
pub fn BillingPage() -> impl IntoView {
    let auth = use_auth();
    let summary = RwSignal::new(None::<billing::BillingSummary>);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let plan = RwSignal::new("tier_1".into());
    let session = RwSignal::new(String::new());
    let checkout_status = RwSignal::new(None::<String>);
    let quote = RwSignal::new(None::<billing::Quote>);
    let monitors = RwSignal::new(String::from("1"));
    let interval = RwSignal::new("60".into());
    let retention = RwSignal::new("30".into());
    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_bill(&u.role),
        _ => false,
    };
    Effect::new(move |_| {
        spawn_local(async move {
            match billing::summary().await {
                Ok(v) => summary.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });
    let get_quote = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let req = billing::QuoteRequest {
            monitors: monitors.get().parse().unwrap_or(1),
            interval_seconds: interval.get().parse().unwrap_or(60),
            retention_days: retention.get().parse().unwrap_or(30),
        };
        spawn_local(async move {
            match billing::quote(&req).await {
                Ok(v) => quote.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    let checkout = move || {
        let p = plan.get();
        spawn_local(async move {
            match billing::checkout(&p).await {
                Ok(v) => {
                    notice.set(Some(
                        "Checkout created. Continue in the payment provider.".into(),
                    ));
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href(&v.checkout_url);
                    }
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Billing"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><h2>"Current billing"</h2>{move||summary.get().map(|s|view!{<div class="overview-cards"><div class="card"><span class="card-label">"Base"</span><span class="card-value">{format!("{} {}",s.base_price,s.currency)}</span></div><div class="card"><span class="card-label">"Add-ons"</span><span class="card-value">{format!("{} {}",s.add_ons_total,s.currency)}</span></div><div class="card"><span class="card-label">"Total"</span><span class="card-value">{format!("{} {}",s.total,s.currency)}</span></div></div>})}</section><section class="card"><h2>"Pricing quote"</h2><p class="muted">"The server is the only pricing authority; these values are sent to the quote endpoint and never calculated in the browser."</p><form on:submit=get_quote class="inline-form"><label>"Monitors"<input type="number" min="1" prop:value=monitors on:input=move|e|monitors.set(event_target_value(&e))/></label><label>"Interval (s)"<input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e))/></label><label>"Retention (days)"<input type="number" min="1" prop:value=retention on:input=move|e|retention.set(event_target_value(&e))/></label><button class="btn">"Get quote"</button></form>{move||quote.get().map(|q|view!{<div class="quote-box"><strong>{format!("{} {} / month",q.monthly_price,q.currency)}</strong><span>{q.interval_label}</span><span>{q.retention_label}</span></div>})}</section><section class="card"><h2>"Verify checkout"</h2><p class="muted">"After the provider completes payment, enter the checkout session ID returned by the server. Payment state is verified server-side."</p><form on:submit=move|e|{e.prevent_default();let id=session.get();spawn_local(async move{match billing::verify_checkout(&id).await{Ok(v)=>checkout_status.set(Some(v.status)),Err(e)=>error.set(Some(e.to_string()))}})} class="inline-form"><input placeholder="Checkout session ID" prop:value=session on:input=move|e|session.set(event_target_value(&e)) required/><button class="btn">"Verify"</button></form>{move||checkout_status.get().map(|v|view!{<p><strong>{v}</strong></p>})}</section>{move||if can(){view!{<section class="card"><h2>"Change plan"</h2><div class="inline-form"><select prop:value=plan on:change=move|e|plan.set(event_target_value(&e))><option value="free">"Free"</option><option value="tier_1">"Tier 1"</option><option value="tier_5">"Tier 5"</option><option value="tier_10">"Tier 10"</option><option value="tier_20">"Tier 20"</option></select><button class="btn btn-primary" on:click=move |_|checkout()>"Continue to checkout"</button></div></section>}.into_any()}else{view!{<p class="muted">"Only the tenant owner can change the subscription."</p>}.into_any()}}</div>}
}

==================== apps/leptos-dashboard/src/pages/pricing.rs ====================
use crate::api::billing;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn PricingPage() -> impl IntoView {
    let monitors = RwSignal::new(String::from("1"));
    let interval = RwSignal::new("60".into());
    let retention = RwSignal::new("30".into());
    let quote = RwSignal::new(None::<billing::Quote>);
    let error = RwSignal::new(None::<String>);
    let refresh = move || {
        let req = billing::QuoteRequest {
            monitors: monitors.get().parse().unwrap_or(1),
            interval_seconds: interval.get().parse().unwrap_or(60),
            retention_days: retention.get().parse().unwrap_or(30),
        };
        spawn_local(async move {
            match billing::quote(&req).await {
                Ok(v) => quote.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Pricing"</h1></header><p class="muted">"Live prices below come directly from the backend pricing configuration."</p><form on:submit=move|e|{e.prevent_default();refresh()} class="inline-form"><input type="number" min="1" prop:value=monitors on:input=move|e|monitors.set(event_target_value(&e))/><input type="number" min="1" prop:value=interval on:input=move|e|interval.set(event_target_value(&e))/><input type="number" min="1" prop:value=retention on:input=move|e|retention.set(event_target_value(&e))/><button class="btn btn-primary">"Calculate"</button></form>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||quote.get().map(|q|view!{<section class="card"><h2>{format!("{} {} / month",q.monthly_price,q.currency)}</h2><p>{q.interval_label}  -  {q.retention_label}</p><p class="muted">{format!("{} checks per period",q.checks_per_period)}</p></section>})}</div>}
}

==================== apps/leptos-dashboard/src/pages/support.rs ====================
use crate::api::support;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn SupportPage() -> impl IntoView {
    let auth = use_auth();
    let tickets = RwSignal::new(Vec::<support::Ticket>::new());
    let selected = RwSignal::new(None::<(support::Ticket, Vec<support::Message>)>);
    let subject = RwSignal::new(String::new());
    let category = RwSignal::new("technical_issue".into());
    let message = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let can_status = move || match auth.state.get() {
        AuthState::Authenticated(u) => matches!(u.role, Role::Admin | Role::Owner),
        _ => false,
    };
    let reload = move || {
        spawn_local(async move {
            match support::tickets().await {
                Ok(v) => tickets.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    };
    reload();
    let create = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let req = support::CreateTicket {
            subject: subject.get(),
            category: category.get(),
        };
        spawn_local(async move {
            match support::create_ticket(&req).await {
                Ok(v) => {
                    tickets.update(|t| t.insert(0, v));
                    subject.set(String::new());
                    notice.set(Some("Ticket created.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Support"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<div class="feature-grid"><section class="card"><h2>"New ticket"</h2><form on:submit=create class="stack-form"><label>"Subject"<input prop:value=subject on:input=move|e|subject.set(event_target_value(&e)) required/></label><label>"Category"<select prop:value=category on:change=move|e|category.set(event_target_value(&e))><option value="account">"Account"</option><option value="billing">"Billing"</option><option value="monitor">"Monitor"</option><option value="notifications">"Notifications"</option><option value="api">"API"</option><option value="security">"Security"</option><option value="technical_issue">"Technical issue"</option><option value="other">"Other"</option></select></label><button class="btn btn-primary">"Create ticket"</button></form></section><section class="card"><h2>"Your tickets"</h2>{move||tickets.get().into_iter().map(|t|{let id=t.id.clone();view!{<button class="list-row link-button" on:click=move |_|{let id=id.clone();spawn_local(async move{match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}})}><span><strong>{t.ticket_number}</strong> {t.subject}</span><span>{t.status}</span></button>}}).collect_view()}</section></div>{move||selected.get().map(|(t,msgs)|{let id=t.id.clone();view!{<section class="card"><h2>{format!("{} — {}",t.ticket_number,t.subject)}</h2><p class="muted">{t.status}</p>{move||if can_status(){let id=t.id.clone();view!{<select on:change=move|e|{let status=event_target_value(&e);let id=id.clone();spawn_local(async move{match support::set_status(&id,&status).await{Ok(())=>match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))},Err(e)=>error.set(Some(e.to_string()))}})}><option value="open">"Open"</option><option value="in_progress">"In progress"</option><option value="waiting_for_customer">"Waiting for customer"</option><option value="resolved">"Resolved"</option><option value="closed">"Closed"</option></select>}.into_any()}else{().into_any()}}{msgs.into_iter().map(|m|view!{<div class="message"><div class="muted">{m.created_at}</div><p>{m.message}</p></div>}).collect_view()}<form on:submit={let id=id.clone();move|e:leptos::ev::SubmitEvent|{e.prevent_default();let text=message.get();spawn_local(async move{match support::reply(&id,&text).await{Ok(_)=>{message.set(String::new());match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}},Err(e)=>error.set(Some(e.to_string()))}})}} class="inline-form"><input placeholder="Reply" prop:value=message on:input=move|e|message.set(event_target_value(&e)) required/><button class="btn">"Reply"</button></form></section>}})}</div>}
}

==================== apps/leptos-dashboard/src/pages/team.rs ====================
use crate::api::team;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role: &Role) -> bool {
    matches!(role, Role::Admin | Role::Owner)
}

#[component]
pub fn TeamPage() -> impl IntoView {
    let auth = use_auth();
    let members = RwSignal::new(Vec::<team::Member>::new());
    let error = RwSignal::new(None::<String>);
    let loading = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let new_user = RwSignal::new(String::new());
    let new_role = RwSignal::new("Member".to_string());
    let notice = RwSignal::new(None::<String>);

    let reload = move || {
        spawn_local(async move {
            match team::list_members().await {
                Ok(v) => members.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };
    reload();

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_manage(&u.role),
        _ => false,
    };

    let add = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let uid = new_user.get();
        let role = new_role.get();
        if uid.trim().is_empty() {
            error.set(Some("User ID is required.".into()));
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match team::add_member(&team::AddMemberRequest { user_id: uid, role }).await {
                Ok(v) => {
                    members.update(|m| m.push(v));
                    new_user.set(String::new());
                    notice.set(Some("Member added.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><h1>"Team members"</h1></header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            <section class="card">
                <h2>"Members"</h2>
                {move || if loading.get() {
                    view! { <div class="loading">"Loading…"</div> }.into_any()
                } else {
                    view! {
                        <table>
                            <thead><tr><th>"User ID"</th><th>"Role"</th><th>"Joined"</th><th></th></tr></thead>
                            <tbody>
                                {move || members.get().into_iter().map(|m| {
                                    let id = m.user_id.clone();
                                    let role_value = m.role.clone();
                                    view! {
                                        <tr>
                                            <td><code>{m.user_id}</code></td>
                                            <td>
                                                <select
                                                    disabled=move || !can()
                                                    prop:value=role_value
                                                    on:change=move |e| {
                                                        let role = event_target_value(&e);
                                                        let id = id.clone();
                                                        spawn_local(async move {
                                                            if let Err(e) = team::change_role(&id, &role).await {
                                                                error.set(Some(e.to_string()));
                                                            }
                                                        });
                                                    }
                                                >
                                                    <option value="Member">"Member"</option>
                                                    <option value="Admin">"Admin"</option>
                                                    <option value="Owner">"Owner"</option>
                                                    <option value="Auditor">"Auditor"</option>
                                                    <option value="Viewer">"Viewer"</option>
                                                </select>
                                            </td>
                                            <td>{m.joined_at.unwrap_or_else(|| "Pending".into())}</td>
                                            <td>
                                                {move || if can() {
                                                    let remove_id = id.clone();
                                                    view! {
                                                        <button class="btn btn-danger" on:click=move |_| {
                                                            let remove_id = remove_id.clone();
                                                            spawn_local(async move {
                                                                match team::remove_member(&remove_id).await {
                                                                    Ok(()) => members.update(|v| v.retain(|x| x.user_id != remove_id)),
                                                                    Err(e) => error.set(Some(e.to_string())),
                                                                }
                                                            });
                                                        }>{"Remove"}</button>
                                                    }.into_any()
                                                } else { ().into_any() }}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any()
                }}
            </section>
            {move || if can() {
                view! {
                    <section class="card">
                        <h2>"Add existing user"</h2>
                        <p class="muted">"The backend requires the existing user's UUID; the server remains the authorization boundary."</p>
                        <form on:submit=add class="inline-form">
                            <input placeholder="User UUID" prop:value=new_user on:input=move |e| new_user.set(event_target_value(&e)) required/>
                            <select prop:value=new_role on:change=move |e| new_role.set(event_target_value(&e))>
                                <option value="Member">"Member"</option><option value="Admin">"Admin"</option><option value="Auditor">"Auditor"</option><option value="Viewer">"Viewer"</option>
                            </select>
                            <button class="btn btn-primary" disabled=move || busy.get()>"Add"</button>
                        </form>
                    </section>
                }.into_any()
            } else { ().into_any() }}
        </div>
    }
}
abubakar-hassan-babayo@AHBYPC:~/uptime-monitor$ cargo check --manifest-path apps/leptos-dashboard/Cargo.toml 2>&1
    Checking leptos-dashboard v0.1.0 (/home/abubakar-hassan-babayo/uptime-monitor/apps/leptos-dashboard)
error[E0425]: cannot find value `navigate` in this scope
  --> apps/leptos-dashboard/src/pages/status_pages.rs:46:102
   |
46 | ...lone(); let nav=navigate.clone();
   |                    ^^^^^^^^ not found in this scope

warning: variable does not need to be mutable
   --> apps/leptos-dashboard/src/api/client.rs:228:9
    |
228 |     let mut options = web_sys::BlobPropertyBag::new();
    |         ----^^^^^^^
    |         |
    |         help: remove this `mut`
    |
    = note: `#[warn(unused_mut)]` (part of `#[warn(unused)]`) on by default

error[E0308]: mismatched types
   --> apps/leptos-dashboard/src/pages/api_flows.rs:106:1395
    |
106 | ...eps.update(|v|{if i<v.len(){v.remove(i)}})>"Remove"</button></l...
    |                   -------------^^^^^^^^^^^-
    |                   |            |
    |                   |            expected `()`, found `StepDraft`
    |                   `if` expressions without `else` arms expect their inner expression to be `()`
    |
help: consider using a semicolon here
    |
106 |     view! {<div class="feature-page"><header class="page-header"><h1>"API flows"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|{let v=event_target_value(&e);selected.set((!v.is_empty()).then_some(v));steps.set(Vec::new())}><option value="">"Select a monitor"</option>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<option value={id}>{m.name}</option>}}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Build flow"</h2><form on:submit=add class="inline-form"><input placeholder="Step name" prop:value=step_name on:input=move|e|step_name.set(event_target_value(&e)) required/><select prop:value=step_method on:change=move|e|step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select><input placeholder="https://example.com/api" prop:value=step_url on:input=move|e|step_url.set(event_target_value(&e)) required/><button class="btn">"Add step"</button></form><ol>{move||steps.get().into_iter().enumerate().map(|(i,s)|view!{<li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_|steps.update(|v|{if i<v.len(){v.remove(i);}})>"Remove"</button></li>}).collect_view()}</ol><button class="btn btn-primary" on:click=save disabled=move||steps.get().is_empty()>{move||if editing.get().is_some(){"Save flow"}else{"Create flow"}}</button></section>})}<section class="card"><h2>"Existing flows"</h2>{move||flows.get().into_iter().map(|f|{let id=f.id.clone();let loaded=f.steps.clone();view!{<article class="list-row"><div><strong>{format!("Flow {}",f.id)}</strong><div class="muted">{format!("{} step(s)",f.steps.len())}</div></div><button class="btn" on:click=move |_|{editing.set(Some(id.clone()));steps.set(loaded.iter().map(|s|StepDraft{name:s.name.clone(),method:s.method.clone(),url:s.url_template.clone(),headers:s.headers.clone(),body:s.body.clone(),extract:s.extract.iter().map(|x|match x{developer::Extraction::StatusCode=>developer::ExtractionRequest::StatusCode,developer::Extraction::Header{name}=>developer::ExtractionRequest::Header{name:name.clone()},developer::Extraction::JsonPath{path}=>developer::ExtractionRequest::JsonPath{path:path.clone()},developer::Extraction::Body=>developer::ExtractionRequest::Body}).collect(),assertions:s.assertions.iter().map(|x|developer::AssertionRequest{response_part:x.response_part.clone(),operator:x.operator.clone(),target:x.target.clone(),expected_value:x.expected_value.clone()}).collect()}).collect());}>"Edit"</button></article>}}).collect_view()}</section><p class="muted">"Flow URLs, headers and assertions are validated by the backend. This UI intentionally sends only fields supported by the current contract."</p></div>}
    |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              +
help: consider using a semicolon here
    |
106 |     view! {<div class="feature-page"><header class="page-header"><h1>"API flows"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|{let v=event_target_value(&e);selected.set((!v.is_empty()).then_some(v));steps.set(Vec::new())}><option value="">"Select a monitor"</option>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<option value={id}>{m.name}</option>}}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Build flow"</h2><form on:submit=add class="inline-form"><input placeholder="Step name" prop:value=step_name on:input=move|e|step_name.set(event_target_value(&e)) required/><select prop:value=step_method on:change=move|e|step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select><input placeholder="https://example.com/api" prop:value=step_url on:input=move|e|step_url.set(event_target_value(&e)) required/><button class="btn">"Add step"</button></form><ol>{move||steps.get().into_iter().enumerate().map(|(i,s)|view!{<li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_|steps.update(|v|{if i<v.len(){v.remove(i)};})>"Remove"</button></li>}).collect_view()}</ol><button class="btn btn-primary" on:click=save disabled=move||steps.get().is_empty()>{move||if editing.get().is_some(){"Save flow"}else{"Create flow"}}</button></section>})}<section class="card"><h2>"Existing flows"</h2>{move||flows.get().into_iter().map(|f|{let id=f.id.clone();let loaded=f.steps.clone();view!{<article class="list-row"><div><strong>{format!("Flow {}",f.id)}</strong><div class="muted">{format!("{} step(s)",f.steps.len())}</div></div><button class="btn" on:click=move |_|{editing.set(Some(id.clone()));steps.set(loaded.iter().map(|s|StepDraft{name:s.name.clone(),method:s.method.clone(),url:s.url_template.clone(),headers:s.headers.clone(),body:s.body.clone(),extract:s.extract.iter().map(|x|match x{developer::Extraction::StatusCode=>developer::ExtractionRequest::StatusCode,developer::Extraction::Header{name}=>developer::ExtractionRequest::Header{name:name.clone()},developer::Extraction::JsonPath{path}=>developer::ExtractionRequest::JsonPath{path:path.clone()},developer::Extraction::Body=>developer::ExtractionRequest::Body}).collect(),assertions:s.assertions.iter().map(|x|developer::AssertionRequest{response_part:x.response_part.clone(),operator:x.operator.clone(),target:x.target.clone(),expected_value:x.expected_value.clone()}).collect()}).collect());}>"Edit"</button></article>}}).collect_view()}</section><p class="muted">"Flow URLs, headers and assertions are validated by the backend. This UI intentionally sends only fields supported by the current contract."</p></div>}
    |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               +

error[E0631]: type mismatch in closure arguments
   --> apps/leptos-dashboard/src/pages/api_flows.rs:106:1487
    |
 68 | ... = move |ev: leptos::ev::SubmitEvent| {
    |       ---------------------------------- found signature defined here
...
106 | ...div class="feature-page"><header class="page-header"><h1>"API flows"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|{let v=event_target_value(&e);selected.set((!v.is_empty()).then_some(v));steps.set(Vec::new())}><option value="">"Select a monitor"</option>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<option value={id}>{m.name}</option>}}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Build flow"</h2><form on:submit=add class="inline-form"><input placeholder="Step name" prop:value=step_name on:input=move|e|step_name.set(event_target_value(&e)) required/><select prop:value=step_method on:change=move|e|step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select><input placeholder="https://example.com/api" prop:value=step_url on:input=move|e|step_url.set(event_target_value(&e)) required/><button class="btn">"Add step"</button></form><ol>{move||steps.get().into_iter().enumerate().map(|(i,s)|view!{<li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_|steps.update(|v|{if i<v.len(){v.remove(i)}})>"Remove"</button></li>}).collect_view()}</ol><button class="btn btn-primary" on:cl...
    |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      ^^ expected due to this
    |
    = note: expected closure signature `fn(MouseEvent) -> _`
               found closure signature `fn(SubmitEvent) -> _`
    = note: required for `HtmlElement<Button, (...,), ...>` to implement `OnAttribute<click, {closure@...}>`
    = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-6793555415621294438.txt'
    = note: consider using `--verbose` to print the full type name to the console

error[E0599]: the method `disabled` exists for struct `HtmlElement<Button, (..., ...), ...>`, but its trait bounds were not satisfied
   --> apps/leptos-dashboard/src/pages/api_flows.rs:106:1501
    |
 68 | ... = move |ev: leptos::ev::SubmitEvent| {
    |       ---------------------------------- doesn't satisfy `_: FnOnce<(MouseEvent,)>`
...
106 | ...div class="feature-page"><header class="page-header"><h1>"API flows"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}<section class="card"><label>"Monitor"<select on:change=move|e|{let v=event_target_value(&e);selected.set((!v.is_empty()).then_some(v));steps.set(Vec::new())}><option value="">"Select a monitor"</option>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<option value={id}>{m.name}</option>}}).collect_view()}</select></label></section>{move||selected.get().map(|_|view!{<section class="card"><h2>"Build flow"</h2><form on:submit=add class="inline-form"><input placeholder="Step name" prop:value=step_name on:input=move|e|step_name.set(event_target_value(&e)) required/><select prop:value=step_method on:change=move|e|step_method.set(event_target_value(&e))><option value="GET">"GET"</option><option value="HEAD">"HEAD"</option></select><input placeholder="https://example.com/api" prop:value=step_url on:input=move|e|step_url.set(event_target_value(&e)) required/><button class="btn">"Add step"</button></form><ol>{move||steps.get().into_iter().enumerate().map(|(i,s)|view!{<li class="list-row"><span>{format!("{}. {} — {} {}",i+1,s.name,s.method,s.url)}</span><button class="btn btn-danger" type="button" on:click=move |_|steps.update(|v|{if i<v.len(){v.remove(i)}})>"Remove"</button></li>}).collect_view()}</ol><button class="btn btn-primary" on:click=save disabled=mo...
    |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    ^^^^^^^^ method cannot be called on `HtmlElement<Button, (..., ...), ...>` due to unsatisfied trait bounds
    |
   ::: /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tachys-0.1.9/src/html/event.rs:136:1
    |
136 | pub struct On<E, F> {
    | ------------------- doesn't satisfy `_: Attribute`
    |
    = note: the following trait bounds were not satisfied:
            `{closure@apps/leptos-dashboard/src/pages/api_flows.rs:68:16: 68:50}: FnOnce<(MouseEvent,)>`
            which is required by `On<click, {closure@apps/leptos-dashboard/src/pages/api_flows.rs:68:16: 68:50}>: Attribute`
    = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-851115321079697408.txt'
    = note: consider using `--verbose` to print the full type name to the console

error[E0525]: expected a closure that implements the `FnMut` trait, but this closure only implements `FnOnce`
  --> apps/leptos-dashboard/src/pages/api_keys.rs:55:573
   |
55 | ...();view!{<div class="list-row"><div><strong>{k.name}</strong><div class="muted"><code>{k.key_prefix}</code>  -  {k.scopes.join(", ")}</div></div>{move||if can(){view!{<button class="btn btn-danger" on:click=move |_|{let id=id.clone();spawn_local(async move{match developer::delete_key(&id).await{Ok(())=>keys.update(|v|v.retain(|x|x.id!=id)),Err(e)=>error.set(Some(e.to_string()))}})}>"Revoke"</button>}.into_any()}else{().into_any()}}</div>}})....
   |       -----------------------------------------------------------------------------------------------------------------------------------------------^^^^^^--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
   |       |                                                                                                                                             ||                                                                            |
   |       |                                                                                                                                             ||                                                                            closure is `FnOnce` because it moves the variable `id` out of its environment
   |       |                                                                                                                                             |this closure implements `FnOnce`, not `FnMut`
   |       |                                                                                                                                             |this tail expression is of type `{closure@api_keys.rs:55:573}`
   |       |                                                                                                                                             the requirement to implement `FnMut` derives from here
   |       required by a bound introduced by this call
   |
   = note: required for `{closure@api_keys.rs:55:573}` to implement `ReactiveFunction`
   = note: required for `{closure@api_keys.rs:55:573}` to implement `leptos::prelude::Render`
   = note: required for `{closure@api_keys.rs:55:573}` to implement `leptos::prelude::IntoRender`
   = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-15780295803421866624.txt'
   = note: consider using `--verbose` to print the full type name to the console
help: you might have meant to create the closure instead of a block
   |
55 |     view! {<div class="feature-page"><header class="page-header"><h1>"API keys"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||secret.get().map(|s|view!{<div class="secret-box"><strong>"API key — copy it now"</strong><code>{s}</code><span class="muted">"It will not be shown again."</span></div>})}<section class="card"><h2>"Keys"</h2>{move||keys.get().into_iter().map(|k|{let id=k.id.clone();view!{<div class="list-row"><div><strong>{k.name}</strong><div class="muted"><code>{k.key_prefix}</code>  -  {k.scopes.join(", ")}</div></div>|_| {move||if can(){view!{<button class="btn btn-danger" on:click=move |_|{let id=id.clone();spawn_local(async move{match developer::delete_key(&id).await{Ok(())=>keys.update(|v|v.retain(|x|x.id!=id)),Err(e)=>error.set(Some(e.to_string()))}})}>"Revoke"</button>}.into_any()}else{().into_any()}}</div>}}).collect_view()}</section>{move||if can(){view!{<section class="card"><h2>"Create key"</h2><form on:submit=create class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><fieldset><legend>"Scopes"</legend>{developer::API_KEY_SCOPES.iter().map(|scope|{let s=(*scope).to_string();view!{<label class="check-row"><input type="checkbox" on:change=move|e|{let checked=event_target::<web_sys::HtmlInputElement>(&e).checked();scopes.update(|v|if checked{if !v.contains(&s){v.push(s.clone())}}else{v.retain(|x|x!=&s)});}/>{*scope}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get()>"Create key"</button></form></section>}.into_any()}else{().into_any()}}<p class="muted">"The server enforces scope permissions. Secrets are never stored in browser storage."</p></div>}
   |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            +++

error[E0277]: the trait bound `Vec<View<...>>: Render` is not satisfied
   --> apps/leptos-dashboard/src/pages/api_keys.rs:55:5
    |
 55 |     view! {<div class="feat...er storage."</p></div>}
    |     ^^^^^^^^^^^^^^^^^^^^^^^...^^^^^^^^^^^^^^^^^^^^^^^ unsatisfied trait bound
    |
    = help: the trait `leptos::prelude::Render` is not implemented for `Vec<View<HtmlElement<Div, ..., ...>>>`
help: the trait `leptos::prelude::Render` is implemented for `Vec<T>`
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tachys-0.1.9/src/view/iterators.rs:133:1
    |
133 | / impl<T> Render for Vec<T>
134 | | where
135 | |     T: Render,
    | |______________^
    = note: required for `{closure@api_keys.rs:55:373}` to implement `leptos::prelude::Render`
    = note: required for `{closure@api_keys.rs:55:373}` to implement `leptos::prelude::IntoRender`
    = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-3907193232583981569.txt'
    = note: consider using `--verbose` to print the full type name to the console
    = note: this error originates in the macro `view` (in Nightly builds, run with -Z macro-backtrace for more info)

error[E0282]: type annotations needed for `leptos::prelude::RwSignal<_>`
  --> apps/leptos-dashboard/src/pages/billing.rs:20:9
   |
20 |     let interval = RwSignal::new("60".into());
   |         ^^^^^^^^
...
38 |             interval_seconds: interval.get().parse().unwrap_or(60),
   |                               -------------- type must be known at this point
   |
help: consider giving `interval` an explicit type, where the placeholder `_` is specified
   |
20 |     let interval: leptos::prelude::RwSignal<T> = RwSignal::new("60".into());
   |                 ++++++++++++++++++++++++++++++

error[E0277]: the trait bound `str: Clone` is not satisfied
  --> apps/leptos-dashboard/src/pages/billing.rs:49:22
   |
49 |         let p = plan.get();
   |                      ^^^ the trait `Clone` is not implemented for `str`
   |
help: the trait `Clone` is implemented for `std::string::String`
  --> /rustc/8bab26f4f68e0e26f0bb7960be334d5b520ea452/library/alloc/src/string.rs:2363:0
   = note: required for `leptos::prelude::RwSignal<str>` to implement `leptos::prelude::Get`

error[E0277]: the trait bound `&str: Into<str>` is not satisfied
  --> apps/leptos-dashboard/src/pages/billing.rs:15:39
   |
15 |     let plan = RwSignal::new("tier_1".into());
   |                                       ^^^^ the trait `From<&str>` is not implemented for `str`
   |
   = help: `&str` implements trait `From<T>`:
             From<&Subtag>
             From<&Key>
             From<&Attribute>
             From<&Key>
             From<&SubdivisionSuffix>
             From<&CurrencyType>
             From<&icu_locale_core::subtags::Subtag>
             From<&Language>
           and 4 others
   = note: required for `&str` to implement `Into<str>`
   = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-15494918796377704226.txt'
   = note: consider using `--verbose` to print the full type name to the console

error[E0277]: the size for values of type `str` cannot be known at compilation time
  --> apps/leptos-dashboard/src/pages/billing.rs:49:13
   |
49 |         let p = plan.get();
   |             ^ doesn't have a size known at compile-time
   |
   = help: the trait `Sized` is not implemented for `str`
   = note: all local variables must have a statically known size

error[E0277]: the size for values of type `str` cannot be known at compilation time
   --> apps/leptos-dashboard/src/pages/billing.rs:15:30
    |
 15 |     let plan = RwSignal::new("tier_1".into());
    |                ------------- ^^^^^^^^^^^^^^^ doesn't have a size known at compile-time
    |                |
    |                required by a bound introduced by this call
    |
    = help: the trait `Sized` is not implemented for `str`
note: required by a bound in `leptos::prelude::RwSignal::<T>::new`
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reactive_graph-0.1.8/src/signal/rw.rs:114:6
    |
114 | impl<T> RwSignal<T>
    |      ^ required by this bound in `RwSignal::<T>::new`
...
124 |     pub fn new(value: T) -> Self {
    |            --- required by a bound in this associated function
help: consider removing this method call, as the receiver has type `&'static str` and `&'static str: Sized` trivially holds
    |
 15 -     let plan = RwSignal::new("tier_1".into());
 15 +     let plan = RwSignal::new("tier_1");
    |

error[E0277]: the size for values of type `str` cannot be known at compilation time
   --> apps/leptos-dashboard/src/pages/billing.rs:49:22
    |
 49 |         let p = plan.get();
    |                      ^^^ doesn't have a size known at compile-time
    |
    = help: the trait `Sized` is not implemented for `str`
help: the trait `leptos::prelude::DefinedAt` is implemented for `leptos::prelude::RwSignal<T, S>`
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reactive_graph-0.1.8/src/signal/rw.rs:297:1
    |
297 | impl<T, S> DefinedAt for RwSignal<T, S> {
    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    = note: required for `leptos::prelude::RwSignal<str>` to implement `leptos::prelude::DefinedAt`
note: required by a bound in `leptos::prelude::Get::get`
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reactive_graph-0.1.8/src/traits.rs:373:16
    |
373 | pub trait Get: DefinedAt {
    |                ^^^^^^^^^ required by this bound in `Get::get`
...
387 |     fn get(&self) -> Self::Value {
    |        --- required by a bound in this associated function

error[E0277]: the trait bound `leptos::prelude::RwSignal<str>: IntoProperty` is not satisfied
   --> apps/leptos-dashboard/src/pages/billing.rs:64:2445
    |
 64 | ...select prop:value=plan on:change=move|e|plan.set(event_target_v...
    |           ----       ^^^^ unsatisfied trait bound
    |           |
    |           required by a bound introduced by this call
    |
    = help: the trait `IntoProperty` is not implemented for `leptos::prelude::RwSignal<str>`
help: the trait `IntoProperty` is implemented for `leptos::prelude::RwSignal<V, S>`
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tachys-0.1.9/src/reactive_graph/property.rs:142:13
    |
142 | /             impl<V, S> IntoProperty for $sig<V, S>
143 | |             where
144 | |                 $sig<V, S>: Get<Value = V>,
145 | |                 S: Send + Sync + 'static,
146 | |                 S: Storage<V> + Storage<Option<V>>,
147 | |                 V: IntoProperty + Send + Sync + Clone + 'static,
148 | |                 V::State: 'static,
    | |__________________________________^
...
185 |       property_signal_arena!(RwSignal);
    |       -------------------------------- in this macro invocation
note: required by a bound in `leptos::prelude::PropAttribute::prop`
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tachys-0.1.9/src/html/attribute/global.rs:45:8
    |
 45 |     P: IntoProperty,
    |        ^^^^^^^^^^^^ required by this bound in `PropAttribute::prop`
...
 51 |     fn prop(self, key: K, value: P) -> Self::Output;
    |        ---- required by a bound in this associated function
    = note: this error originates in the macro `property_signal_arena` (in Nightly builds, run with -Z macro-backtrace for more info)

error[E0277]: the trait bound `leptos::prelude::RwSignal<str>: IntoProperty` is not satisfied
   --> apps/leptos-dashboard/src/pages/billing.rs:64:2351
    |
 64 | ...(){view!{<section class="c...utton></div></section>}.in...
    |       ^^^^^^^^^^^^^^^^^^^^^^^...^^^^^^^^^^^^^^^^^^^^^^^
    |       |                                                                                  |
    |       |                                                                                  required by a bound introduced by this call
    |       unsatisfied trait bound
    |
    = help: the trait `IntoProperty` is not implemented for `leptos::prelude::RwSignal<str>`
help: the trait `IntoProperty` is implemented for `leptos::prelude::RwSignal<V, S>`
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tachys-0.1.9/src/reactive_graph/property.rs:142:13
    |
142 | /             impl<V, S> IntoProperty for $sig<V, S>
143 | |             where
144 | |                 $sig<V, S>: Get<Value = V>,
145 | |                 S: Send + Sync + 'static,
146 | |                 S: Storage<V> + Storage<Option<V>>,
147 | |                 V: IntoProperty + Send + Sync + Clone + 'static,
148 | |                 V::State: 'static,
    | |__________________________________^
...
185 |       property_signal_arena!(RwSignal);
    |       -------------------------------- in this macro invocation
    = note: required for `HtmlElement<Select, (), (...,)>` to implement `PropAttribute<&str, RwSignal<str>>`
    = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-17711905371406301175.txt'
    = note: consider using `--verbose` to print the full type name to the console
    = note: this error originates in the macro `property_signal_arena` (in Nightly builds, run with -Z macro-backtrace for more info)

error[E0277]: the size for values of type `str` cannot be known at compilation time
   --> apps/leptos-dashboard/src/pages/billing.rs:64:2351
    |
 64 | ...(){view!{<section class="c...utton></div></section>}.in...
    |       ^^^^^^^^^^^^^^^^^^^^^^^...^^^^^^^^^^^^^^^^^^^^^^^ doesn't have a size known at compile-time
    |
    = help: the trait `Sized` is not implemented for `str`
note: required by an implicit `Sized` bound in `leptos::prelude::RwSignal`
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reactive_graph-0.1.8/src/signal/rw.rs:102:21
    |
102 | pub struct RwSignal<T, S = SyncStorage> {
    |                     ^ required by the implicit `Sized` requirement on this type parameter in `RwSignal`
    = note: this error originates in the macro `view` (in Nightly builds, run with -Z macro-backtrace for more info)

error[E0599]: the method `on` exists for struct `HtmlElement<Select, (...,), ...>`, but its trait bounds were not satisfied
  --> apps/leptos-dashboard/src/pages/billing.rs:64:2450
   |
64 | ...><select prop:value=plan on:change=move|e|plan.set(event_target_...
   |                             ^^ method cannot be called on `HtmlElement<Select, (...,), ...>` due to unsatisfied trait bounds
   |
  ::: /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tachys-0.1.9/src/html/element/mod.rs:34:1
   |
34 | pub struct HtmlElement<E, At, Ch> {
   | --------------------------------- doesn't satisfy `_: AsRef<Element>`, `_: ElementExt` or `_: OnAttribute<_, _>`
   |
   = note: the following trait bounds were not satisfied:
           `(Property<&str, leptos::prelude::RwSignal<str>>,): Attribute`
           which is required by `leptos::html::HtmlElement<leptos::html::Select, (Property<&str, leptos::prelude::RwSignal<str>>,), ((InertElement, InertElement, InertElement, InertElement, InertElement),)>: leptos::prelude::OnAttribute<_, _>`
           `leptos::html::HtmlElement<leptos::html::Select, (Property<&str, leptos::prelude::RwSignal<str>>,), ((InertElement, InertElement, InertElement, InertElement, InertElement),)>: AsRef<web_sys::Element>`
           which is required by `leptos::html::HtmlElement<leptos::html::Select, (Property<&str, leptos::prelude::RwSignal<str>>,), ((InertElement, InertElement, InertElement, InertElement, InertElement),)>: leptos::prelude::ElementExt`
   = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-14095595023828720362.txt'
   = note: consider using `--verbose` to print the full type name to the console

error[E0599]: the method `set` exists for struct `leptos::prelude::RwSignal<str>`, but its trait bounds were not satisfied
   --> apps/leptos-dashboard/src/pages/billing.rs:64:2472
    |
 64 | ... on:change=move|e|plan.set(event_target_value(&e))><option valu...
    |                           ^^^ method cannot be called on `leptos::prelude::RwSignal<str>` due to unsatisfied trait bounds
    |
   ::: /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reactive_graph-0.1.8/src/signal/rw.rs:102:1
    |
102 | pub struct RwSignal<T, S = SyncStorage> {
    | --------------------------------------- doesn't satisfy `_: IsDisposed`, `_: Set` or `_: Update`
    |
    = note: the following trait bounds were not satisfied:
            `leptos::prelude::RwSignal<str>: leptos::prelude::Update`
            which is required by `leptos::prelude::RwSignal<str>: leptos::prelude::Set`
            `leptos::prelude::RwSignal<str>: leptos::prelude::IsDisposed`
            which is required by `leptos::prelude::RwSignal<str>: leptos::prelude::Set`

error[E0631]: type mismatch in closure arguments
  --> apps/leptos-dashboard/src/pages/escalation.rs:90:1045
   |
45 | ... = move |e: leptos::ev::SubmitEvent| {
   |       --------------------------------- found signature defined here
...
90 | ...class="feature-page"><header class="page-header"><h1>"Escalation policies"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<section class="card"><h2>"Create / edit policy"</h2><form on:submit=save class="stack-form"><label>"Monitor (optional)"<select prop:value=selected_monitor on:change=move|e|{selected_monitor.set(event_target_value(&e));steps.set(Vec::new())}><option value="">"Global"</option>{move||monitors.get().into_iter().map(|m|view!{<option value={m.id}>{m.name}</option>}).collect_view()}</select></label><div class="inline-form"><select prop:value=selected_channel on:change=move|e|selected_channel.set(event_target_value(&e))><option value="">"Channel"</option>{move||channels.get().into_iter().map(|c|view!{<option value={c.id}>{format!("{} — {}",c.kind,c.target)}</option>}).collect_view()}</select><input type="number" min="0" prop:value=delay on:input=move|e|delay.set(event_target_value(&e))/><button class="btn" on:cl...
   |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        ^^ expected due to this
   |
   = note: expected closure signature `fn(MouseEvent) -> _`
              found closure signature `fn(SubmitEvent) -> _`
   = note: required for `HtmlElement<Button, (...,), ...>` to implement `OnAttribute<click, {closure@...}>`
   = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-2919449082587240324.txt'
   = note: consider using `--verbose` to print the full type name to the console

error[E0277]: the trait bound `(..., ..., ...): Render` is not satisfied
  --> apps/leptos-dashboard/src/pages/escalation.rs:90:5
   |
90 |     view! {<div class="feat...iew()}</section></div>}
   |     ^^^^^^^^^^^^^^^^^^^^^^^...^^^^^^^^^^^^^^^^^^^^^^^ unsatisfied trait bound
   |
   = help: the trait `leptos::prelude::Render` is not implemented for `(..., ..., ...)`
   = help: the following other types implement trait `leptos::prelude::Render`:
             ()
             (A, B)
             (A, B, C)
             (A, B, C, D)
             (A, B, C, D, E)
             (A, B, C, D, E, F)
             (A, B, C, D, E, F, G)
             (A, B, C, D, E, F, G, H)
           and 19 others
   = note: required for `((..., ..., ...),)` to implement `leptos::prelude::Render`
   = note: required for `HtmlElement<Div, (), ()>` to implement `ElementChild<(..., ..., ...)>`
   = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-3903389636587059202.txt'
   = note: consider using `--verbose` to print the full type name to the console
   = note: this error originates in the macro `view` (in Nightly builds, run with -Z macro-backtrace for more info)

error[E0599]: the method `class` exists for struct `HtmlElement<Div, (), (...,)>`, but its trait bounds were not satisfied
  --> apps/leptos-dashboard/src/pages/escalation.rs:90:635
   |
90 | ...}</select></label><div class="inline-form"><select prop:value=se...
   |                           ^^^^^ method cannot be called on `HtmlElement<Div, (), (...,)>` due to unsatisfied trait bounds
   |
  ::: /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tachys-0.1.9/src/html/element/mod.rs:34:1
   |
34 | pub struct HtmlElement<E, At, Ch> {
   | --------------------------------- doesn't satisfy `_: AsRef<Element>`, `_: ClassAttribute<_>` or `_: ElementExt`
   |
   = note: the following trait bounds were not satisfied:
           `((leptos::html::HtmlElement<leptos::html::Select, (Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<change, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:701: 90:708}>), ((InertElement, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:789: 90:795}),)>, leptos::html::HtmlElement<Input, (Attr<leptos::attr::Type, &str>, Attr<Min, &str>, Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<input, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:983: 90:990}>), ()>, leptos::html::HtmlElement<Button, (Class<&str>, On<click, {closure@apps/leptos-dashboard/src/pages/escalation.rs:45:20: 45:53}>), (&str,)>),): leptos::prelude::RenderHtml`
           which is required by `leptos::html::HtmlElement<leptos::html::Div, (), ((leptos::html::HtmlElement<leptos::html::Select, (Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<change, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:701: 90:708}>), ((InertElement, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:789: 90:795}),)>, leptos::html::HtmlElement<Input, (Attr<leptos::attr::Type, &str>, Attr<Min, &str>, Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<input, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:983: 90:990}>), ()>, leptos::html::HtmlElement<Button, (Class<&str>, On<click, {closure@apps/leptos-dashboard/src/pages/escalation.rs:45:20: 45:53}>), (&str,)>),)>: leptos::prelude::ClassAttribute<_>`
           `leptos::html::HtmlElement<leptos::html::Div, (), ((leptos::html::HtmlElement<leptos::html::Select, (Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<change, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:701: 90:708}>), ((InertElement, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:789: 90:795}),)>, leptos::html::HtmlElement<Input, (Attr<leptos::attr::Type, &str>, Attr<Min, &str>, Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<input, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:983: 90:990}>), ()>, leptos::html::HtmlElement<Button, (Class<&str>, On<click, {closure@apps/leptos-dashboard/src/pages/escalation.rs:45:20: 45:53}>), (&str,)>),)>: AsRef<web_sys::Element>`
           which is required by `leptos::html::HtmlElement<leptos::html::Div, (), ((leptos::html::HtmlElement<leptos::html::Select, (Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<change, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:701: 90:708}>), ((InertElement, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:789: 90:795}),)>, leptos::html::HtmlElement<Input, (Attr<leptos::attr::Type, &str>, Attr<Min, &str>, Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<input, {closure@apps/leptos-dashboard/src/pages/escalation.rs:90:983: 90:990}>), ()>, leptos::html::HtmlElement<Button, (Class<&str>, On<click, {closure@apps/leptos-dashboard/src/pages/escalation.rs:45:20: 45:53}>), (&str,)>),)>: leptos::prelude::ElementExt`
   = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-10233445044423918578.txt'
   = note: consider using `--verbose` to print the full type name to the console

error[E0308]: mismatched types
   --> apps/leptos-dashboard/src/pages/escalation.rs:90:1757
    |
 90 | ...default());steps.set(p.steps.clone());}>"Edit"</button></div>}}...
    |                     --- ^^^^^^^^^^^^^^^ expected `Vec<EscalationStepPayload>`, found `Vec<EscalationStep>`
    |                     |
    |                     arguments to this method are incorrect
    |
    = note: expected struct `Vec<advanced_monitoring::EscalationStepPayload>`
               found struct `Vec<EscalationStep>`
note: method defined here
   --> /home/abubakar-hassan-babayo/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reactive_graph-0.1.8/src/traits.rs:525:8
    |
525 |     fn set(&self, value: Self::Value);
    |        ^^^

error[E0308]: `match` arms have incompatible types
   --> apps/leptos-dashboard/src/pages/monitor_form.rs:103:1081
    |
103 | ...e||match kind.get().as_str(){"http"|"flow"=>view!{<><label>"URL"<in...</label></>.into_any()},"tcp"=>view!{<><label>"Host"<input prop:value=url on:input=move|e|url.set(event_target_value(&e)) required/></label><label>"TCP port"<input type="number" min="1" max="65535" prop:value=tcp_port on:input=move|e|tcp_port.set(event_target_value(&e)) required/></label></>.into_any()},"d...
    |       -------------------------                -----------------------...-----------------------        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected a tuple with 3 elements, found one with 2 elements
    |       |                                        |                                                  |                                                                                                           |                                                                                                                                                                                                                              |
    |       |                                        |                                                  |                                                                                                           |                                                                                                                                                                                                                              one of the expected closures
    |       |                                        |                                                  |                                                                                                           one of the expected closures
    |       |                                        |                                                  one of the expected closures
    |       |                                        this is found to be of type `leptos::prelude::View<((leptos::html::HtmlElement<leptos::html::Label, (), ((&str, leptos::html::HtmlElement<Input, (Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<input, {closure@apps/leptos-dashboard/src/pages/monitor_form.rs:103:674: 103:681}>, Attr<Required, bool>), ()>),)>, leptos::html::HtmlElement<leptos::html::Label, (), ((&str, leptos::html::HtmlElement<leptos::html::Select, (Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<change, {closure@apps/leptos-dashboard/src/pages/monitor_form.rs:103:782: 103:789}>), ((InertElement, InertElement),)>),)>, leptos::html::HtmlElement<leptos::html::Label, (), ((&str, leptos::html::HtmlElement<Input, (Attr<leptos::attr::Type, &str>, Attr<Min, &str>, Attr<Max, &str>, Property<&str, leptos::prelude::RwSignal<std::string::String>>, On<input, {closure@apps/leptos-dashboard/src/pages/monitor_form.rs:103:1005: 103:1012}>), ()>),)>), &str)>`
    |       `match` arms have incompatible types
    |
    = note: expected struct `View<((..., ..., ...), ...)>`
               found struct `View<((..., ...), ...)>`
    = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-13857435634235828254.txt'
    = note: consider using `--verbose` to print the full type name to the console
    = note: this error originates in the macro `view` (in Nightly builds, run with -Z macro-backtrace for more info)

error[E0282]: type annotations needed for `leptos::prelude::RwSignal<_>`
  --> apps/leptos-dashboard/src/pages/pricing.rs:7:9
   |
 7 |     let interval = RwSignal::new("60".into());
   |         ^^^^^^^^
...
14 |             interval_seconds: interval.get().parse().unwrap_or(60),
   |                               -------------- type must be known at this point
   |
help: consider giving `interval` an explicit type, where the placeholder `_` is specified
   |
 7 |     let interval: leptos::prelude::RwSignal<T> = RwSignal::new("60".into());
   |                 ++++++++++++++++++++++++++++++

error[E0525]: expected a closure that implements the `FnMut` trait, but this closure only implements `FnOnce`
   --> apps/leptos-dashboard/src/pages/status_pages.rs:117:16
    |
117 |     let save = move |ev: leptos::ev::SubmitEvent| {
    |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ this closure implements `FnOnce`, not `FnMut`
...
131 |             match api::update(&id, &req).await {
    |                                -- closure is `FnOnce` because it moves the variable `id` out of its environment
...
185 |     view! {<section class="...{e}</div>})}</section>}
    |     -----------------------...-----------------------
    |     |                                                                                                                                                                                         |
    |     |                                                                                                                                                                                         required by a bound introduced by this call
    |     the requirement to implement `FnMut` derives from here
    |
    = note: required for `HtmlElement<Form, (Class<...>,), ...>` to implement `OnAttribute<submit, {closure@...}>`
    = note: the full name for the type has been written to '/run/media/abubakar-hassan-babayo/2e9125ba-a356-4262-9af0-959cb5e15607/cargo-target/debug/deps/leptos_dashboard-e7bb615a6c640513.long-type-15103569044886578012.txt'
    = note: consider using `--verbose` to print the full type name to the console

error[E0507]: cannot move out of `id`, a captured variable in an `FnMut` closure
  --> apps/leptos-dashboard/src/pages/support.rs:45:2481
   |
45 | ...et id=id.clone();move|e:leptos::ev::SubmitEvent|{e.prevent_default();let text=message.get();spawn_local(async move{match support::reply(&id,&t...
   |       -- ---------- -------------------------------                                                        ^^^^^^^^^^ `id` is moved here    -- variable moved due to use in coroutine
   |       |  |          |
   |       |  |          captured by this `FnMut` closure
   |       |  move occurs because `id` has type `std::string::String`, which does not implement the `Copy` trait
   |       captured outer variable
   |
   = help: `Fn` and `FnMut` closures require captured values to be able to be consumed multiple times, but `FnOnce` closures may consume them only once
help: consider cloning the value before moving it into the closure
   |
45 ~     view! {<div class="feature-page"><header class="page-header"><h1>"Support"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<div class="feature-grid"><section class="card"><h2>"New ticket"</h2><form on:submit=create class="stack-form"><label>"Subject"<input prop:value=subject on:input=move|e|subject.set(event_target_value(&e)) required/></label><label>"Category"<select prop:value=category on:change=move|e|category.set(event_target_value(&e))><option value="account">"Account"</option><option value="billing">"Billing"</option><option value="monitor">"Monitor"</option><option value="notifications">"Notifications"</option><option value="api">"API"</option><option value="security">"Security"</option><option value="technical_issue">"Technical issue"</option><option value="other">"Other"</option></select></label><button class="btn btn-primary">"Create ticket"</button></form></section><section class="card"><h2>"Your tickets"</h2>{move||tickets.get().into_iter().map(|t|{let id=t.id.clone();view!{<button class="list-row link-button" on:click=move |_|{let id=id.clone();spawn_local(async move{match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}})}><span><strong>{t.ticket_number}</strong> {t.subject}</span><span>{t.status}</span></button>}}).collect_view()}</section></div>{move||selected.get().map(|(t,msgs)|{let id=t.id.clone();view!{<section class="card"><h2>{format!("{} — {}",t.ticket_number,t.subject)}</h2><p class="muted">{t.status}</p>{move||if can_status(){let id=t.id.clone();view!{<select on:change=move|e|{let status=event_target_value(&e);let id=id.clone();spawn_local(async move{match support::set_status(&id,&status).await{Ok(())=>match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))},Err(e)=>error.set(Some(e.to_string()))}})}><option value="open">"Open"</option><option value="in_progress">"In progress"</option><option value="waiting_for_customer">"Waiting for customer"</option><option value="resolved">"Resolved"</option><option value="closed">"Closed"</option></select>}.into_any()}else{().into_any()}}{msgs.into_iter().map(|m|view!{<div class="message"><div class="muted">{m.created_at}</div><p>{m.message}</p></div>}).collect_view()}<form on:submit={let id=id.clone();move|e:leptos::ev::SubmitEvent|{e.prevent_default();let text=message.get();spawn_local({
46 +     let value = id.clone();
47 ~     async move{match support::reply(&value,&text).await{Ok(_)=>{message.set(String::new());match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}},Err(e)=>error.set(Some(e.to_string()))}}
48 ~     })}} class="inline-form"><input placeholder="Reply" prop:value=message on:input=move|e|message.set(event_target_value(&e)) required/><button class="btn">"Reply"</button></form></section>}})}</div>}
   |

error[E0382]: use of moved value: `id`
   --> apps/leptos-dashboard/src/pages/team.rs:102:50
    |
 74 | ...                   let id = m.user_id.clone();
    |                           -- move occurs because `id` has type `std::string::String`, which does not implement the `Copy` trait
...
 83 | ...                                   on:change=move |e| {
    |                                                 -------- value moved into closure here
 84 | ...                                       let role = event_tar...
 85 | ...                                       let id = id.clone();
    |                                                    -- variable moved due to use in closure
...
102 | ...                               {move || if can() {
    |                                    ^^^^^^^ value used here after move
103 | ...                                   let remove_id = id.clone();
    |                                                       -- use occurs due to use in closure
    |
help: consider cloning the value before moving it into the closure
    |
 83 ~                                                     on:change={
 84 +                                                     let value = id.clone();
 85 ~                                                     move |e| {
 86 |                                                         let role = event_target_value(&e);
 87 ~                                                         let id = value.clone();
 88 |                                                         spawn_local(async move {
...
 92 |                                                         });
 93 ~                                                     }
 94 +                                                     }
    |

Some errors have detailed explanations: E0277, E0282, E0308, E0382, E0425, E0507, E0525, E0599, E0631.
For more information about an error, try `rustc --explain E0277`.
warning: `leptos-dashboard` (lib) generated 1 warning
error: could not compile `leptos-dashboard` (lib) due to 26 previous errors; 1 warning emitted
abubakar-hassan-babayo@AHBYPC:~/uptime-monitor$ 
