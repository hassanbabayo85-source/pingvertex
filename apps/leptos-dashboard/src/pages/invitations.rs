use crate::api::team;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn can_manage(role: &Role) -> bool {
    matches!(role, Role::Admin | Role::Owner)
}

#[component]
pub fn InvitationsPage() -> impl IntoView {
    let auth = use_auth();
    let invitations = RwSignal::new(Vec::<team::Invitation>::new());
    let error = RwSignal::new(None::<String>);
    let token = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let role = RwSignal::new("Member".into());
    let notice = RwSignal::new(None::<String>);
    let created_token = RwSignal::new(None::<String>);
    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_manage(&u.role),
        _ => false,
    };
    let reload = move || {
        spawn_local(async move {
            match team::list_invitations().await {
                Ok(v) => invitations.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    };
    reload();
    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let req = team::CreateInvitationRequest {
            email: email.get(),
            role: role.get(),
        };
        spawn_local(async move {
            match team::create_invitation(&req).await {
                Ok(v) => {
                    created_token.set(Some(v.invitation_token));
                    notice.set(Some(
                        "Invitation created. Save the token now; it is returned only once.".into(),
                    ));
                    reload();
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    let accept = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let t = token.get();
        spawn_local(async move {
            match team::accept_invitation(&t).await {
                Ok(_) => notice.set(Some("Invitation accepted.".into())),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {
        <div class="feature-page">
            <header class="page-header"><h1>"Invitations"</h1></header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || created_token.get().map(|t| view! { <div class="secret-box"><strong>"Invitation token — save it now"</strong><code>{t}</code></div> })}
            <div class="feature-grid">
                {move || if can() { view! { <section class="card"><h2>"Create invitation"</h2><form on:submit=create class="stack-form"><label>"Email"<input type="email" prop:value=email on:input=move |e| email.set(event_target_value(&e)) required/></label><label>"Role"<select prop:value=role on:change=move |e| role.set(event_target_value(&e))><option value="Member">"Member"</option><option value="Admin">"Admin"</option><option value="Auditor">"Auditor"</option><option value="Viewer">"Viewer"</option></select></label><button class="btn btn-primary">"Create"</button></form></section> }.into_any() } else { ().into_any() }}
                <section class="card"><h2>"Pending"</h2>{move || invitations.get().into_iter().map(|i| { let id=i.id.clone(); view! { <div class="list-row"><div><strong>{i.email}</strong><div class="muted">{i.role}  -  expires {i.expires_at}</div></div>{move || if can(){ let revoke_id=id.clone(); view!{<button class="btn btn-danger" on:click=move |_|{let revoke_id=revoke_id.clone();spawn_local(async move{match team::revoke_invitation(&revoke_id).await{Ok(())=>invitations.update(|v|v.retain(|x|x.id!=revoke_id)),Err(e)=>error.set(Some(e.to_string()))}})}>"Revoke"</button>}.into_any()}else{().into_any()}}</div> } }).collect_view()}</section>
            </div>
            <section class="card"><h2>"Accept invitation"</h2><form on:submit=accept class="inline-form"><input placeholder="Invitation token" prop:value=token on:input=move |e| token.set(event_target_value(&e)) required/><button class="btn">"Accept"</button></form></section>
        </div>
    }
}
