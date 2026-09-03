use crate::auth::api;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn ForgotPasswordPage() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let success = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let email_val = email.get();
        spawn_local(async move {
            match api::forgot_password(&email_val).await {
                Ok(()) => {
                    success.set(true);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="auth-shell">
            <div class="auth-card">
                <div class="auth-brand">
                    <span class="auth-brand-mark"><span class="pulse-dot"></span></span>
                    <span class="auth-brand-name">"PingVertex"</span>
                </div>
                <h1>"Reset your password"</h1>
                <p class="auth-subtitle">"We'll email you a link to choose a new one."</p>

                {move || {
                    if success.get() {
                        view! {
                            <p class="auth-success">"If an account exists for that email, you will receive a password reset link."</p>
                        }.into_any()
                    } else {
                        view! {
                            <form on:submit=on_submit class="auth-form">
                                <div class="field">
                                    <label for="email">"Email"</label>
                                    <input type="email" id="email" prop:value=email on:input=move |ev| email.set(event_target_value(&ev)) required/>
                                </div>
                                <button type="submit" class="btn btn-primary btn-block">"Send reset link"</button>
                            </form>
                        }.into_any()
                    }
                }}

                {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}

                <div class="auth-links">
                    <a href="/login">"Back to sign in"</a>
                </div>
            </div>
        </div>
    }
}
