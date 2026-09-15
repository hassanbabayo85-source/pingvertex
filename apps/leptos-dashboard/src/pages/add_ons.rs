use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

const RETENTION_TIERS: &[(&str, &str)] =
    &[("90", "90 days"), ("180", "180 days"), ("365", "365 days")];

#[component]
pub fn AddOnsPage() -> impl IntoView {
    let auth = use_auth();
    let kind = RwSignal::new("enterprise".to_string());
    let qty = RwSignal::new(String::from("1"));
    let country = RwSignal::new(String::new());
    let tier = RwSignal::new("90".to_string());
    let result = RwSignal::new(None::<billing::AddOn>);
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    
    // Gyara: Amfani da RwSignal don ajiye active add-ons maimakon Resource
    let active_add_ons = RwSignal::new(None::<Vec<billing::AddOn>>);
    let summary_error = RwSignal::new(None::<String>);

    // Function don ɗauko jerin add-ons da ke aiki
    let fetch_summary = move || {
        spawn_local(async move {
            match billing::summary().await {
                Ok(summary) => active_add_ons.set(Some(summary.active_add_ons)),
                Err(e) => summary_error.set(Some(e.to_string())),
            }
        });
    };

    // Kira fetch_summary a farkon buɗe shafin
    Effect::new(move |_| {
        fetch_summary();
    });

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => matches!(u.role, Role::Owner),
        _ => false,
    };
    let is_retention = move || kind.get() == "retention";
    let is_sms = move || kind.get() == "sms";

    let cancel_addon = move |id: String| {
        busy.set(true);
        spawn_local(async move {
            if let Err(e) = billing::cancel_add_on(&id).await {
                error.set(Some(format!("Failed to cancel: {}", e)));
            } else {
                fetch_summary(); // Sake ɗauko jerin bayan an soke
            }
            busy.set(false);
        });
    };

    let verify_addon = move |id: String| {
        busy.set(true);
        spawn_local(async move {
            if let Err(e) = billing::verify_add_on(&id).await {
                error.set(Some(format!("Verification failed: {}", e)));
            } else {
                fetch_summary(); // Sake ɗauko jerin idan ya tabbata
                result.set(None); // Ɓoye sanarwar biyan kuɗi
            }
            busy.set(false);
        });
    };

    let add = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();

        let (quantity, country_code, tier_value) = if is_retention() {
            (1u64, None, Some(tier.get()))
        } else {
            let q = match qty.get().trim().parse::<u64>() {
                Ok(v) if v >= 1 => v,
                _ => {
                    error.set(Some(
                        "Quantity must be a valid number of at least 1.".into(),
                    ));
                    return;
                }
            };
            let c = if country.get().is_empty() {
                None
            } else {
                Some(country.get())
            };
            (q, c, None)
        };

        error.set(None);
        busy.set(true);
        let req = billing::AddOnRequest {
            addon_type: kind.get(),
            quantity,
            country_code,
            tier: tier_value,
        };
        spawn_local(async move {
            match billing::add_on(&req).await {
                Ok(v) => {
                    result.set(Some(v));
                    fetch_summary();
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
                    <span class="eyebrow">"PAYG"</span>
                    <h1>"Add-ons"</h1>
                    <p class="page-subtitle">
                        "SMS credits, Enterprise SLA, and extended history retention."
                    </p>
                </div>
            </header>

            {move || error.get().map(|e| view! { <div class="error" role="alert">{e}</div> })}

            {move || result.get().map(|r| {
                let summary_text = format!(
                    "Created {} × {} for {} {}",
                    r.quantity, r.addon_type, r.total_price, r.currency
                );
                let checkout_url = r.checkout_url.clone();
                let addon_id = r.id.clone();
                view! {
                    <div class="success card" style="margin-bottom: 2rem;">
                        <p>{summary_text}</p>
                        <div style="display: flex; gap: 1rem; margin-top: 1rem;">
                            {checkout_url.map(|url| view! {
                                <a
                                    class="btn btn-primary"
                                    href=url
                                    target="_blank"
                                    rel="noopener noreferrer"
                                >
                                    "Complete payment"
                                </a>
                            })}
                            <button class="btn" on:click=move |_| verify_addon(addon_id.clone()) disabled=move || busy.get()>
                                {move || if busy.get() { "Verifying..." } else { "I've paid, verify now" }}
                            </button>
                        </div>
                    </div>
                }
            })}

            <section class="card" style="margin-bottom: 2rem;">
                <h2>"Active Add-ons"</h2>
                {move || match active_add_ons.get() {
                    Some(addons) => {
                        if addons.is_empty() {
                            view! { <p class="muted">"No active add-ons."</p> }.into_any()
                        } else {
                            view! {
                                <div class="stack">
                                    {addons.into_iter().map(|addon| {
                                        let id = addon.id.clone();
                                        view! {
                                            <div style="display: flex; justify-content: space-between; align-items: center; padding: 1rem 0; border-bottom: 1px solid var(--border-color);">
                                                <div>
                                                    <strong>{addon.quantity} " × " {addon.addon_type}</strong>
                                                    <br/>
                                                    <span class="muted">{addon.status} " - " {addon.total_price} " " {addon.currency}</span>
                                                </div>
                                                <button class="btn btn-danger" on:click=move |_| cancel_addon(id.clone()) disabled=move || busy.get()>
                                                    "Cancel"
                                                </button>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        }
                    },
                    None => {
                        if let Some(e) = summary_error.get() {
                            view! { <div class="error">"Failed to load: " {e}</div> }.into_any()
                        } else {
                            view! { <p class="muted">"Loading..."</p> }.into_any()
                        }
                    }
                }}
            </section>

            {move || if can() {
                view! {
                    <section class="card">
                        <h2>"Purchase New Add-on"</h2>
                        <form on:submit=add class="stack-form" style="margin-top: 1rem;">
                            <label>
                                "Type"
                                <select
                                    prop:value=kind
                                    on:change=move |e| kind.set(event_target_value(&e))
                                >
                                    <option value="enterprise">"Enterprise SLA"</option>
                                    <option value="sms">"SMS credits"</option>
                                    <option value="retention">"Extended Retention"</option>
                                </select>
                            </label>

                            {move || if is_retention() {
                                view! {
                                    <label>
                                        "Retention tier"
                                        <select
                                            prop:value=tier
                                            on:change=move |e| tier.set(event_target_value(&e))
                                        >
                                            {RETENTION_TIERS
                                                .iter()
                                                .map(|(value, label)| view! {
                                                    <option value=*value>{*label}</option>
                                                })
                                                .collect_view()}
                                        </select>
                                    </label>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <label>
                                        "Quantity"
                                        <input
                                            type="number"
                                            min="1"
                                            prop:value=qty
                                            on:input=move |e| qty.set(event_target_value(&e))
                                        />
                                    </label>
                                    {move || if is_sms() {
                                        view! {
                                            <label>
                                                "Country code"
                                                <input
                                                    placeholder="NG"
                                                    prop:value=country
                                                    on:input=move |e| country.set(event_target_value(&e))
                                                    required
                                                />
                                            </label>
                                        }
                                        .into_any()
                                    } else {
                                        ().into_any()
                                    }}
                                }
                                .into_any()
                            }}

                            <button class="btn btn-primary" disabled=move || busy.get()>
                                {move || if busy.get() { "Creating…" } else { "Create add-on" }}
                            </button>
                        </form>
                        <p class="muted" style="margin-top: 1rem;">
                            "Add-ons are purchased through the payment provider, not from the wallet. \
                             Extended retention replaces your tenant's default history retention until cancelled."
                        </p>
                    </section>
                }
                .into_any()
            } else {
                view! { <p class="muted">"Only the tenant owner can manage add-ons."</p> }
                    .into_any()
            }}
        </div>
    }
}
