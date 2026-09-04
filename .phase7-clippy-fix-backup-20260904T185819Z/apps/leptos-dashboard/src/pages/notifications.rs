use crate::api::monitoring;
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn NotificationsPage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let channels = RwSignal::new(Vec::<monitoring::NotificationChannel>::new());
    let selected = RwSignal::new(String::new());
    let kind = RwSignal::new(String::from("email"));
    let target = RwSignal::new(String::new());
    let chat_id = RwSignal::new(String::new());
    let secret = RwSignal::new(None::<String>);
    let pending_delete = RwSignal::new(None::<String>);
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
        });
    });

    let add = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        error.set(None);
        let id = selected.get();
        let target_value = target.get().trim().to_string();
        if id.is_empty() || target_value.is_empty() {
            error.set(Some("Select a monitor and provide a target.".into()));
            return;
        }
        let kind_value = kind.get();
        let req = monitoring::ChannelPayload {
            kind: kind_value.clone(),
            target: target_value,
            chat_id: if kind_value == "telegram" {
                Some(chat_id.get().trim().to_string())
            } else {
                None
            },
        };
        busy.set(true);
        spawn_local(async move {
            match monitoring::create_notification_channel(&id, &req).await {
                Ok(v) => {
                    secret.set(v.signing_secret);
                    channels.update(|c| c.push(v.channel));
                    target.set(String::new());
                    chat_id.set(String::new());
                    notice.set(Some("Notification channel created. Copy the signing secret now if you need it for webhook verification.".into()));
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Alerting"</span><h1>"Notifications"</h1></div></header>
            <p class="muted">"Channels are configured per monitor. Secrets are returned only at creation time."</p>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || secret.get().map(|s| view! {
                <div class="secret-box"><strong>"Signing secret — save it now"</strong><code>{s}</code><span class="muted">"It is not stored in this UI after you hide it."</span><button class="btn" type="button" on:click=move |_| secret.set(None)>"Hide secret"</button></div>
            })}
            <section class="card">
                <label>"Monitor"<select prop:value=selected on:change=move|e|{selected.set(event_target_value(&e));secret.set(None);} >
                    <option value="">"Select monitor"</option>
                    {move || monitors.get().into_iter().map(|m| view! { <option value={m.id}>{m.name}</option> }).collect_view()}
                </select></label>
                {move || if selected.get().is_empty() {
                    view! { <p class="muted">"Choose a monitor to configure its channels."</p> }.into_any()
                } else {
                    view! { <>
                        <form on:submit=add class="inline-form">
                            <select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))>
                                <option value="email">"Email"</option><option value="webhook">"Webhook"</option><option value="telegram">"Telegram"</option>
                            </select>
                            {move || if kind.get()=="telegram" {
                                view! { <input placeholder="Telegram chat ID" autocomplete="off" prop:value=chat_id on:input=move|e|chat_id.set(event_target_value(&e)) required/> }.into_any()
                            } else { ().into_any() }}
                            <input placeholder="Target" autocomplete="off" prop:value=target on:input=move|e|target.set(event_target_value(&e)) required/>
                            <button class="btn btn-primary" disabled=move || busy.get()>{move || if busy.get() {"Adding…"} else {"Add channel"}}</button>
                        </form>
                        {move || if channels.get().is_empty() {
                            view! { <p class="muted">"No channels configured for this monitor."</p> }.into_any()
                        } else {
                            view! { <div class="list-stack">{move || channels.get().into_iter().map(|c| {
                                let id=c.id.clone();
                                view! { <div class="list-row"><div><strong>{format!("{}",c.kind)}</strong><div class="muted">{c.target}</div></div><div class="row-actions"><span class="muted">{if c.enabled {"Enabled"} else {"Disabled"}}</span><button class="btn btn-danger" disabled=move || busy.get() on:click=move |_| pending_delete.set(Some(id.clone()))>"Delete"</button></div></div> }
                            }).collect_view()}</div> }.into_any()
                        }}
                    </> }.into_any()
                }}
            </section>
            {move || pending_delete.get().map(|id| {
                let confirm_id=id.clone();
                view! { <ConfirmDialog
                    title="Delete notification channel"
                    body="This stops notifications through this channel. The action is permanent."
                    on_confirm=Callback::new(move |_| {
                        let id=confirm_id.clone();
                        busy.set(true);
                        spawn_local(async move {
                            match monitoring::delete_notification_channel(&id).await {
                                Ok(()) => { channels.update(|v|v.retain(|x|x.id!=id)); notice.set(Some("Notification channel deleted.".into())); }
                                Err(e) => error.set(Some(e)),
                            }
                            pending_delete.set(None); busy.set(false);
                        });
                    })
                    on_cancel=Callback::new(move |_| pending_delete.set(None))
                /> }
            })}
        </div>
    }
}
