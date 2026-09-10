use crate::api::audit;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn AuditPage() -> impl IntoView {
    let page = RwSignal::new(None::<audit::AuditPage>);
    let error = RwSignal::new(None::<String>);
    let action = RwSignal::new(String::new());
    let actor = RwSignal::new(String::new());
    let target = RwSignal::new(String::new());
    let from = RwSignal::new(String::new());
    let to = RwSignal::new(String::new());
    let cursor = RwSignal::new(None::<String>);
    let reload = RwSignal::new(0_u32);

    Effect::new(move |_| {
        let _ = reload.get();
        let cursor_value = cursor.get();
        let action_value = action.get_untracked();
        let actor_value = actor.get_untracked();
        let target_value = target.get_untracked();
        let from_value = from.get();
        let to_value = to.get();
        spawn_local(async move {
            match audit::list(
                cursor_value.as_deref(),
                Some(&action_value),
                Some(&actor_value),
                Some(&target_value),
                Some(&from_value),
                Some(&to_value),
            )
            .await
            {
                Ok(value) => {
                    error.set(None);
                    page.set(Some(value));
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    });

    view! {
        <div class="feature-page">
            <header class="page-header"><div><span class="eyebrow">"SECURITY"</span><h1>"Audit log"</h1><p class="muted">"Tenant-scoped history of security, monitor and billing actions."</p></div></header>
            {move || error.get().map(|e| view!{<div class="error" role="alert">{e}</div>})}
            <section class="card">
                <div class="form-grid">
                    <label>"Action"<input placeholder="e.g. MonitorCreated" prop:value=action on:input=move |e| { action.set(event_target_value(&e)); cursor.set(None); reload.update(|v| *v += 1); } /></label>
                    <label>"Actor"<input placeholder="user UUID" prop:value=actor on:input=move |e| { actor.set(event_target_value(&e)); cursor.set(None); reload.update(|v| *v += 1); } /></label>
                    <label>"Target"<input placeholder="resource id" prop:value=target on:input=move |e| { target.set(event_target_value(&e)); cursor.set(None); reload.update(|v| *v += 1); } /></label>
                    <label>"From"<input type="date" prop:value=from on:change=move |e| { from.set(event_target_value(&e)); cursor.set(None); reload.update(|v| *v += 1); } /></label>
                    <label>"To"<input type="date" prop:value=to on:change=move |e| { to.set(event_target_value(&e)); cursor.set(None); reload.update(|v| *v += 1); } /></label>
                </div>
            </section>
            <section class="card">
                <div class="table-shell"><table class="data-table"><thead><tr><th>"Time"</th><th>"Source"</th><th>"Actor"</th><th>"Action"</th><th>"Target"</th><th>"Details"</th></tr></thead>
                <tbody>{move || match page.get() {
                    Some(p) if p.items.is_empty() => view!{<tr><td colspan="6"><div class="empty-state"><h3>"No audit events"</h3><p>"No events match the selected filters."</p></div></td></tr>}.into_any(),
                    Some(p) => { let next = p.next_cursor.clone(); view!{<>{p.items.into_iter().map(|e| view!{<tr><td>{e.created_at}</td><td>{e.source}</td><td><code>{e.actor_user_id}</code></td><td>{e.action}</td><td>{e.target_id}</td><td><code>{e.metadata.to_string()}</code></td></tr>}).collect_view()}{next.map(|value| view!{<tr><td colspan="6"><button class="button button-secondary" on:click=move |_| { cursor.set(Some(value.clone())); } >"Next page"</button></td></tr>})}</>}.into_any() }
                    None => view!{<tr><td colspan="6">"Loading…"</td></tr>}.into_any(),
                }}</tbody></table></div>
            </section>
        </div>
    }
}
