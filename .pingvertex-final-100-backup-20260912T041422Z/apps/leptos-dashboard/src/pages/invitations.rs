use crate::api::team;
use crate::auth::context::{use_auth, AuthState, Role};
use crate::components::confirm_dialog::ConfirmDialog;
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
    let notice = RwSignal::new(None::<String>);
    let email = RwSignal::new(String::new());
    let role = RwSignal::new("Member".into());
    let token = RwSignal::new(String::new());
    let created_token = RwSignal::new(None::<String>);
    let pending_revoke = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

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
        error.set(None);
        let email_value = email.get().trim().to_string();
        if email_value.is_empty() {
            error.set(Some("Email is required.".into()));
            return;
        }
        let req = team::CreateInvitationRequest {
            email: email_value,
            role: role.get(),
        };
        busy.set(true);
        spawn_local(async move {
            match team::create_invitation(&req).await {
                Ok(v) => {
                    created_token.set(Some(v.invitation_token));
                    email.set(String::new());
                    notice.set(Some(
                        "Invitation created. Save the token now; it is returned only once.".into(),
                    ));
                    reload();
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    let accept = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let t = token.get().trim().to_string();
        if t.is_empty() {
            error.set(Some("Invitation token is required.".into()));
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match team::accept_invitation(&t).await {
                Ok(_) => {
                    token.set(String::new());
                    notice.set(Some("Invitation accepted.".into()));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"Access control"</span><h1>"Invitations"</h1></div><a class="btn" href="/team">"Team"</a></header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            {move || created_token.get().map(|t| view! {
                <div class="secret-box">
                    <strong>"Invitation token — save it now"</strong>
                    <code>{t}</code>
                    <span class="muted">"This token is shown only in this session. Do not share it publicly."</span>
                    <button class="btn" type="button" on:click=move |_| created_token.set(None)>"Hide token"</button>
                </div>
            })}
            <div class="feature-grid">
                {move || if can() {
                    view! {
                        <section class="card">
                            <h2>"Create invitation"</h2>
                            <form on:submit=create class="stack-form">
                                <label>"Email"<input type="email" autocomplete="email" maxlength="254" prop:value=email on:input=move |e| email.set(event_target_value(&e)) required/></label>
                                <label>"Role"<select prop:value=role on:change=move |e| role.set(event_target_value(&e))>
                                    <option value="Member">"Member"</option><option value="Admin">"Admin"</option><option value="Auditor">"Auditor"</option><option value="Viewer">"Viewer"</option>
                                </select></label>
                                <button class="btn btn-primary" disabled=move || busy.get()>{move || if busy.get() {"Creating…"} else {"Create invitation"}}</button>
                            </form>
                        </section>
                    }.into_any()
                } else {
                    view! { <section class="card"><h2>"Invitation management"</h2><p class="muted">"You do not have permission to create or revoke tenant invitations."</p></section> }.into_any()
                }}
                <section class="card">
                    <div class="section-heading"><h2>"Pending"</h2><span class="muted">"Expired invitations are handled by the server."</span></div>
                    {move || if invitations.get().is_empty() {
                        view! { <div class="empty-state"><p>"No pending invitations."</p></div> }.into_any()
                    } else {
                        view! { <div class="list-stack">{move || invitations.get().into_iter().map(|i| {
                            let id = i.id.clone();
                            view! { <div class="list-row"><div><strong>{i.email}</strong><div class="muted">{i.role}{" · expires "}{i.expires_at}</div></div>
                                {move || if can() {
                                    let revoke_id = id.clone();
                                    view! { <button class="btn btn-danger" disabled=move || busy.get() on:click=move |_| pending_revoke.set(Some(revoke_id.clone()))>"Revoke"</button> }.into_any()
                                } else { ().into_any() }}
                            </div> }
                        }).collect_view()}</div> }.into_any()
                    }}
                </section>
            </div>
            <section class="card">
                <h2>"Accept invitation"</h2>
                <p class="muted">"Paste a valid invitation token. The server validates ownership, expiry and tenant membership."</p>
                <form on:submit=accept class="inline-form">
                    <input type="password" autocomplete="off" placeholder="Invitation token" prop:value=token on:input=move |e| token.set(event_target_value(&e)) required/>
                    <button class="btn" disabled=move || busy.get()>"Accept"</button>
                </form>
            </section>
            {move || pending_revoke.get().map(|id| {
                let confirm_id = id.clone();
                view! {
                    <ConfirmDialog
                        title="Revoke invitation"
                        body="The invitation token will stop working immediately."
                        on_confirm=Callback::new(move |_| {
                            let id = confirm_id.clone();
                            busy.set(true);
                            spawn_local(async move {
                                match team::revoke_invitation(&id).await {
                                    Ok(()) => {
                                        invitations.update(|v| v.retain(|x| x.id != id));
                                        notice.set(Some("Invitation revoked.".into()));
                                    }
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                pending_revoke.set(None);
                                busy.set(false);
                            });
                        })
                        on_cancel=Callback::new(move |_| pending_revoke.set(None))
                    />
                }
            })}
        </div>
    }
}
