use crate::api::billing;
use crate::auth::context::{use_auth, AuthState, Role};
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Retention tiers exposed by the add-on UI. Kept in sync with the
/// server's `RetentionTier` enum: `"90" | "180" | "365"`.
const RETENTION_TIERS: &[(&str, &str)] = &[
    ("90", "90 days"),
    ("180", "180 days"),
    ("365", "365 days"),
];

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

    let can = move || match auth.state.get() {
        AuthState::Authenticated(u) => matches!(u.role, Role::Owner),
        _ => false,
    };
    let is_retention = move || kind.get() == "retention";
    let is_sms = move || kind.get() == "sms";

    let add = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();

        // Retention is a tier, not a quantity: the server rejects
        // `quantity != 1` and requires `tier`. Force those values here
        // so the buyer never sees the quantity/country inputs.
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
                Ok(v) => result.set(Some(v)),
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
                let summary = format!(
                    "Created {} × {} for {} {}",
                    r.quantity, r.addon_type, r.total_price, r.currency
                );
                let checkout_url = r.checkout_url.clone();
                view! {
                    <div class="success">
                        <p>{summary}</p>
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
                    </div>
                }
            })}

            {move || if can() {
                view! {
                    <section class="card">
                        <form on:submit=add class="stack-form">
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
                        <p class="muted">
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
