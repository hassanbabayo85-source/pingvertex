use leptos::prelude::*;
use leptos::task::spawn_local;
use crate::api::client;

#[component]
#[allow(dead_code)]
pub fn MfaStepUpModal(
    /// Wannan yana nuna ko za a buɗe Modal ɗin ko a'a
    #[prop(into)] show: Signal<bool>,
    /// Function da za a kira idan mutum ya ba da lambar daidai
    on_success: Callback<(), ()>,
    /// Function da za a kira idan mutum ya fasa (Cancel)
    on_cancel: Callback<(), ()>,
) -> impl IntoView {
    let totp_code = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let submit_mfa = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        busy.set(true);
        let code = totp_code.get();

        spawn_local(async move {
            let body = serde_json::json!({ "totp_code": code });
            // Kira API ɗin tabbatarwa (ko wane iri ne backend yake amfani da shi)
            match client::post_json::<serde_json::Value, _>("/auth/totp/verify", &body).await {
                Ok(_) => {
                    totp_code.set(String::new()); // Wanke input ɗin
                    on_success.run(()); // Ci gaba da aikin
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        {move || if show.get() {
            view! {
                <div style="position: fixed; inset: 0; background: rgba(0,0,0,0.5); display: flex; align-items: center; justify-content: center; z-index: 1000;">
                    <div class="card" style="background: var(--bg-color); max-width: 400px; width: 90%; box-shadow: 0 4px 6px rgba(0,0,0,0.1);">
                        <h2 style="margin-top: 0;">"Security Verification Required"</h2>
                        <p class="muted">"Please enter the 6-digit code from your authenticator app to authorize this high-privilege action."</p>
                        
                        <form on:submit=submit_mfa class="auth-form" style="margin-top: 1rem;">
                            <div class="field">
                                <label for="stepup-code">"6-digit code"</label>
                                <input
                                    type="text"
                                    id="stepup-code"
                                    inputmode="numeric"
                                    autocomplete="one-time-code"
                                    prop:value=totp_code
                                    on:input=move |ev| totp_code.set(event_target_value(&ev))
                                    required
                                />
                            </div>
                            {move || error.get().map(|e| view! { <div class="form-error" style="color: var(--danger-color); margin-top: 0.5rem;">{e}</div> })}
                            
                            <div style="display: flex; gap: 1rem; margin-top: 1.5rem; justify-content: flex-end;">
                                <button type="button" class="btn" on:click=move |_| on_cancel.run(()) disabled=move || busy.get()>
                                    "Cancel"
                                </button>
                                <button type="submit" class="btn btn-primary" disabled=move || busy.get()>
                                    {move || if busy.get() { "Verifying..." } else { "Verify & Continue" }}
                                </button>
                            </div>
                        </form>
                    </div>
                </div>
            }.into_any()
        } else {
            ().into_any()
        }}
    }
}
