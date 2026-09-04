use crate::api::team;
use crate::auth::context::{use_auth, AuthState, Role};
use crate::components::confirm_dialog::ConfirmDialog;
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
    let notice = RwSignal::new(None::<String>);
    let loading = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let pending_remove = RwSignal::new(None::<String>);
    let new_user = RwSignal::new(String::new());
    let new_role = RwSignal::new("Member".to_string());

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => can_manage(&u.role),
        _ => false,
    };

    let reload = move || {
        loading.set(true);
        spawn_local(async move {
            match team::list_members().await {
                Ok(v) => {
                    members.set(v);
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };
    reload();

    let add = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
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
            <header class="page-header">
                <div>
                    <span class="eyebrow">"Access control"</span>
                    <h1>"Team members"</h1>
                </div>
                <a class="btn" href="/invitations">"Invitations"</a>
            </header>
            {move || notice.get().map(|n| view! { <div class="success">{n}</div> })}
            {move || error.get().map(|e| view! { <div class="error">{e}</div> })}
            <section class="card">
                <div class="section-heading">
                    <h2>"Members"</h2>
                    <span class="muted">"Roles are enforced by the server."</span>
                </div>
                {move || if loading.get() {
                    view! { <div class="loading">"Loading members…"</div> }.into_any()
                } else if members.get().is_empty() {
                    view! { <div class="empty-state"><h3>"No members"</h3><p>"Add a member or send an invitation to get started."</p></div> }.into_any()
                } else {
                    view! {
                        <div class="table-scroll">
                            <table>
                                <thead><tr><th>"User"</th><th>"Role"</th><th>"Joined"</th><th></th></tr></thead>
                                <tbody>
                                    {move || members.get().into_iter().map(|m| {
                                        let id = m.user_id.clone();
                                        let initial_role = m.role.clone();
                                        view! {
                                            <tr>
                                                <td><code>{m.user_id}</code></td>
                                                <td>
                                                    <select
                                                        disabled=move || !can() || busy.get()
                                                        prop:value=initial_role
                                                        on:change={
                                                            let id = id.clone();
                                                            move |e| {
                                                                let role = event_target_value(&e);
                                                                let id = id.clone();
                                                                busy.set(true);
                                                                spawn_local(async move {
                                                                    match team::change_role(&id, &role).await {
                                                                        Ok(()) => notice.set(Some("Role updated.".into())),
                                                                        Err(e) => error.set(Some(e.to_string())),
                                                                    }
                                                                    busy.set(false);
                                                                });
                                                            }
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
                                                        view! { <button class="btn btn-danger" disabled=move || busy.get() on:click=move |_| pending_remove.set(Some(remove_id.clone()))>"Remove"</button> }.into_any()
                                                    } else { ().into_any() }}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </section>
            {move || if can() {
                view! {
                    <section class="card">
                        <h2>"Add existing user"</h2>
                        <p class="muted">"Use an existing user UUID. The API remains the authorization boundary."</p>
                        <form on:submit=add class="inline-form">
                            <input placeholder="User UUID" autocomplete="off" prop:value=new_user on:input=move |e| new_user.set(event_target_value(&e)) required/>
                            <select prop:value=new_role on:change=move |e| new_role.set(event_target_value(&e))>
                                <option value="Member">"Member"</option>
                                <option value="Admin">"Admin"</option>
                                <option value="Auditor">"Auditor"</option>
                                <option value="Viewer">"Viewer"</option>
                            </select>
                            <button class="btn btn-primary" disabled=move || busy.get()>"Add"</button>
                        </form>
                    </section>
                }.into_any()
            } else {
                ().into_any()
            }}
            {move || pending_remove.get().map(|id| {
                let confirm_id = id.clone();
                view! {
                    <ConfirmDialog
                        title="Remove team member"
                        body="This removes the member from the tenant. This action cannot be undone here."
                        on_confirm=Callback::new(move |_| {
                            let id = confirm_id.clone();
                            busy.set(true);
                            spawn_local(async move {
                                match team::remove_member(&id).await {
                                    Ok(()) => {
                                        members.update(|v| v.retain(|x| x.user_id != id));
                                        notice.set(Some("Member removed.".into()));
                                    }
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                pending_remove.set(None);
                                busy.set(false);
                            });
                        })
                        on_cancel=Callback::new(move |_| pending_remove.set(None))
                    />
                }
            })}
        </div>
    }
}
