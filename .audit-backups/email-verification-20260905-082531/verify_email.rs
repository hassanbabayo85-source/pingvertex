use crate::api::client;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;

#[component]
pub fn VerifyEmailPage() -> impl IntoView {
    let query = use_query_map();
    let initial_email = query.get().get("email").unwrap_or_default();
    let email = RwSignal::new(initial_email);
    let code = RwSignal::new(String::new());
    let status = RwSignal::new(None::<String>);
    let verified = RwSignal::new(false);
    let busy = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get() {
            return;
        }
        busy.set(true);
        status.set(None);
        let email_val = email.get();
        let code_val = code.get();
        spawn_local(async move {
            let body = serde_json::json!({ "email": email_val, "code": code_val });
            match client::post_json::<serde_json::Value, _>("/auth/verify-email-otp", &body).await {
                Ok(_) => verified.set(true),
                Err(e) => status.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="auth-shell">
            <div class="auth-card">
                <div class="auth-brand">
                    <span class="auth-brand-mark"><span class="pulse-dot"></span></span>
                    <span class="auth-brand-name">"PingVertex"</span>
                </div>
                <h1>"Verify your email"</h1>
                {move || if verified.get() {
                    view! { <p class="auth-success">"Email verified. You can now " <a href="/login">"sign in"</a> "."</p> }.into_any()
                } else {
                    view! {
                        <form on:submit=on_submit class="auth-form">
                            <div class="field">
                                <label for="verify-email">"Email"</label>
                                <input id="verify-email" type="email" autocomplete="email"
                                    prop:value=email on:input=move |ev| email.set(event_target_value(&ev)) required/>
                            </div>
                            <div class="field">
                                <label for="verify-code">"6-digit code"</label>
                                <input id="verify-code" type="text" inputmode="numeric" autocomplete="one-time-code"
                                    maxlength="6" pattern="[0-9]{6}"
                                    prop:value=code on:input=move |ev| code.set(event_target_value(&ev)) required/>
                            </div>
                            <button type="submit" class="btn btn-primary btn-block" disabled=move || busy.get()>
                                {move || if busy.get() { "Verifying..." } else { "Verify email" }}
                            </button>
                        </form>
                    }.into_any()
                }}
                {move || status.get().map(|e| view! { <div class="form-error">{e}</div> })}
                <div class="auth-links">
                    <a href="/login">"Back to sign in"</a>
                </div>
            </div>
        </div>
    }
}
