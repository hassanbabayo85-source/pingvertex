use crate::api::monitoring;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn NotificationsPage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let channels = RwSignal::new(Vec::<monitoring::NotificationChannel>::new());
    let selected = RwSignal::new(String::new());
    let kind = RwSignal::new("email".into());
    let target = RwSignal::new(String::new());
    let chat_id = RwSignal::new(String::new());
    let secret = RwSignal::new(None::<String>);
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
        let id = selected.get();
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
    let add = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let id = selected.get();
        if id.is_empty() {
            error.set(Some("Select a monitor.".into()));
            return;
        }
        let req = monitoring::ChannelPayload {
            kind: kind.get(),
            target: target.get(),
            chat_id: if kind.get() == "telegram" {
                Some(chat_id.get())
            } else {
                None
            },
        };
        spawn_local(async move {
            match monitoring::create_notification_channel(&id, &req).await {
                Ok(v) => {
                    secret.set(v.signing_secret);
                    channels.update(|c| c.push(v.channel));
                    target.set(String::new());
                    notice.set(Some("Notification channel created.".into()));
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Notifications"</h1></header><p class="muted">"Notification channels are configured per monitor. This page provides a single place to manage the channels already supported by the backend."</p>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<section class="card"><label>"Monitor"<select prop:value=selected on:change=move|e|selected.set(event_target_value(&e))><option value="">"Select monitor"</option>{move||monitors.get().into_iter().map(|m|view!{<option value={m.id}>{m.name}</option>}).collect_view()}</select></label>{move||if !selected.get().is_empty(){view!{<><form on:submit=add class="inline-form"><select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))><option value="email">"Email"</option><option value="webhook">"Webhook"</option><option value="telegram">"Telegram"</option></select>{move||if kind.get()=="telegram"{view!{<input placeholder="Telegram chat ID" prop:value=chat_id on:input=move|e|chat_id.set(event_target_value(&e)) required/>}.into_any()}else{().into_any()}}<input placeholder="Target" prop:value=target on:input=move|e|target.set(event_target_value(&e)) required/><button class="btn btn-primary">"Add"</button></form><div>{move||channels.get().into_iter().map(|c|view!{<div class="list-row"><span>{format!("{} — {}",c.kind,c.target)}</span><span>{if c.enabled{"Enabled"}else{"Disabled"}}</span></div>}).collect_view()}</div></>}.into_any()}else{().into_any()}}</section></div>}
}
