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
    view! {<div class="feature-page"><header class="page-header"><h1>"Support"</h1></header>{move||notice.get().map(|n|view!{<div class="success">{n}</div>})}{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<div class="feature-grid"><section class="card"><h2>"New ticket"</h2><form on:submit=create class="stack-form"><label>"Subject"<input prop:value=subject on:input=move|e|subject.set(event_target_value(&e)) required/></label><label>"Category"<select prop:value=category on:change=move|e|category.set(event_target_value(&e))><option value="account">"Account"</option><option value="billing">"Billing"</option><option value="monitor">"Monitor"</option><option value="notifications">"Notifications"</option><option value="api">"API"</option><option value="security">"Security"</option><option value="technical_issue">"Technical issue"</option><option value="other">"Other"</option></select></label><button class="btn btn-primary">"Create ticket"</button></form></section><section class="card"><h2>"Your tickets"</h2>{move||tickets.get().into_iter().map(|t|{let id=t.id.clone();view!{<button class="list-row link-button" on:click=move |_|{let id=id.clone();spawn_local(async move{match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}})}><span><strong>{t.ticket_number}</strong> {t.subject}</span><span>{t.status}</span></button>}}).collect_view()}</section></div>{move||selected.get().map(|(t,msgs)|{let id=t.id.clone();view!{<section class="card"><h2>{format!("{} — {}",t.ticket_number,t.subject)}</h2><p class="muted">{t.status}</p>{move||if can_status(){let id=t.id.clone();view!{<select on:change=move|e|{let status=event_target_value(&e);let id=id.clone();spawn_local(async move{match support::set_status(&id,&status).await{Ok(())=>match support::ticket(&id).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))},Err(e)=>error.set(Some(e.to_string()))}})}><option value="open">"Open"</option><option value="in_progress">"In progress"</option><option value="waiting_for_customer">"Waiting for customer"</option><option value="resolved">"Resolved"</option><option value="closed">"Closed"</option></select>}.into_any()}else{().into_any()}}{msgs.into_iter().map(|m|view!{<div class="message"><div class="muted">{m.created_at}</div><p>{m.message}</p></div>}).collect_view()}<form on:submit={ let id = id.clone(); move |e: leptos::ev::SubmitEvent| { e.prevent_default(); let text = message.get(); let id_clone = id.clone(); spawn_local(async move { match support::reply(&id_clone, &text).await { Ok(_) => { message.set(String::new()); match support::ticket(&id_clone).await { Ok(v) => selected.set(Some(v)), Err(e) => error.set(Some(e.to_string())), } } Err(e) => error.set(Some(e.to_string())), } }); } } class="inline-form"><input placeholder="Reply" prop:value=message on:input=move|e|message.set(event_target_value(&e)) required/><button class="btn">"Reply"</button></form></section>}})}</div>}
}
