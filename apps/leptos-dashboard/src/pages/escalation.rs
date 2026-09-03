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
    let add_step = move |_| {
        let c = selected_channel.get();
        if c.is_empty() {
            error.set(Some("Select a notification channel.".into()));
            return;
        }
        let delay_minutes = match delay.get().trim().parse::<u32>() {
            Ok(value) => value,
            Err(_) => {
                error.set(Some("Delay must be a valid non-negative number.".into()));
                return;
            }
        };
        steps.update(|v| {
            v.push(api::EscalationStepPayload {
                delay_minutes,
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
    view! {<div class="feature-page"><header class="page-header"><h1>"Escalation policies"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<section class="card"><h2>"Create / edit policy"</h2><form on:submit=save class="stack-form"><label>"Monitor (optional)"<select prop:value=selected_monitor on:change=move|e|{selected_monitor.set(event_target_value(&e));steps.set(Vec::new())}><option value="">"Global"</option>{move||monitors.get().into_iter().map(|m|view!{<option value={m.id}>{m.name}</option>}).collect_view()}</select></label><div class="inline-form"><select prop:value=selected_channel on:change=move|e|selected_channel.set(event_target_value(&e))><option value="">"Channel"</option>{move||channels.get().into_iter().map(|c|view!{<option value={c.id}>{format!("{} — {}",c.kind,c.target)}</option>}).collect_view()}</select><input type="number" min="0" prop:value=delay on:input=move|e|delay.set(event_target_value(&e))/><button class="btn" on:click=add_step>"Add step"</button></div><ol>{move||steps.get().into_iter().map(|s|view!{<li>{format!("{} min → {}",s.delay_minutes,s.channel_id)}</li>}).collect_view()}</ol><button class="btn btn-primary" disabled=move||steps.get().is_empty()>"Save policy"</button></form></section><section class="card"><h2>"Existing policies"</h2>{move||policies.get().into_iter().map(|p|{let id=p.id.clone();view!{<div class="list-row"><div><strong>{p.monitor_id.clone().unwrap_or_else(||"Global".into())}</strong><div class="muted">{format!("{} step(s)",p.steps.len())}</div></div><button class="btn" on:click=move |_|{editing.set(Some(id.clone()));selected_monitor.set(p.monitor_id.clone().unwrap_or_default());steps.set(p.steps.iter().map(|s|api::EscalationStepPayload{delay_minutes:s.delay_minutes,channel_id:s.channel_id.clone()}).collect());}>"Edit"</button></div>}}).collect_view()}</section></div>}
}
