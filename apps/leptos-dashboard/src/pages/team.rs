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
                                                    on:change={
                                                        let id = id.clone();
                                                        move |e| {
                                                            let role = event_target_value(&e);
                                                            let id = id.clone();
                                                            spawn_local(async move {
                                                                if let Err(e) = team::change_role(&id, &role).await {
                                                                    error.set(Some(e.to_string()));
                                                                }
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
