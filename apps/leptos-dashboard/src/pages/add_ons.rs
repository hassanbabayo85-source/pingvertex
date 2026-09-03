use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn AddOnsPage() -> impl IntoView {
    let auth = use_auth();
    let kind = RwSignal::new("enterprise".into());
    let qty = RwSignal::new(String::from("1"));
    let country = RwSignal::new(String::new());
    let result = RwSignal::new(None::<billing::AddOn>);
    let error = RwSignal::new(None::<String>);
    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => matches!(u.role, Role::Owner),
        _ => false,
    };
    let add = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let req = billing::AddOnRequest {
            addon_type: kind.get(),
            quantity: qty.get().parse().unwrap_or(0),
            country_code: if country.get().is_empty() {
                None
            } else {
                Some(country.get())
            },
            tier: None,
        };
        spawn_local(async move {
            match billing::add_on(&req).await {
                Ok(v) => result.set(Some(v)),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Add-ons"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}{move||result.get().map(|r|view!{<div class="success">{format!("Created {} × {} for {} {}",r.quantity,r.addon_type,r.total_price,r.currency)}</div>})}{move||if can(){view!{<section class="card"><form on:submit=add class="stack-form"><label>"Type"<select prop:value=kind on:change=move|e|kind.set(event_target_value(&e))><option value="enterprise">"Enterprise SLA"</option><option value="sms">"SMS credits"</option></select></label><label>"Quantity"<input type="number" min="1" prop:value=qty on:input=move|e|qty.set(event_target_value(&e))/></label>{move||if kind.get()=="sms"{view!{<label>"Country code"<input placeholder="NG" prop:value=country on:input=move|e|country.set(event_target_value(&e)) required/></label>}.into_any()}else{().into_any()}}<button class="btn btn-primary">"Create add-on"</button></form></section>}.into_any()}else{view!{<p class="muted">"Only the tenant owner can manage add-ons."</p>}.into_any()}}</div>}
}
