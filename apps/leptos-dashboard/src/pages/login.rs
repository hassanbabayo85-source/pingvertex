use crate::auth::api::{self, LoginResult};
use crate::auth::context::{use_auth, AuthState};
use crate::auth::oauth::OAuthButtons;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

#[component]
pub fn LoginPage() -> impl IntoView {
    let auth = use_auth();
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let two_factor_required = RwSignal::new(false);
    let pending_token = RwSignal::new(String::new());
    let two_factor_code = RwSignal::new(String::new());
    let navigate = use_navigate();
    let navigate_2fa = navigate.clone();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let email_val = email.get();
        let password_val = password.get();
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::login(&email_val, &password_val).await {
                Ok(LoginResult::Success) => match api::fetch_current_user().await {
                    Ok(user) => {
                        auth.state.set(AuthState::Authenticated(user));
                        navigate("/", Default::default());
                    }
                    Err(e) => error.set(Some(e)),
                },
                Ok(LoginResult::TwoFactorRequired {
                    pending_token: token,
                }) => {
                    pending_token.set(token);
                    two_factor_required.set(true);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let on_2fa_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let code = two_factor_code.get();
        let token = pending_token.get();
        let navigate = navigate_2fa.clone();
        spawn_local(async move {
            match api::complete_2fa(&token, &code).await {
                Ok(()) => match api::fetch_current_user().await {
                    Ok(user) => {
                        auth.state.set(AuthState::Authenticated(user));
                        navigate("/", Default::default());
                    }
                    Err(e) => error.set(Some(e)),
                },
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
                <h1>"Sign in"</h1>
                <p class="auth-subtitle">"Monitor your infrastructure without the noise."</p>

                {move || {
                    if two_factor_required.get() {
                        view! {
                            <form on:submit=on_2fa_submit.clone() class="auth-form">
                                <p class="auth-hint">"Enter the code from your authenticator app."</p>
                                <div class="field">
                                    <label for="two-factor-code">"Two-factor code"</label>
                                    <input
                                        type="text"
                                        id="two-factor-code"
                                        autocomplete="one-time-code"
                                        prop:value=two_factor_code
                                        on:input=move |ev| two_factor_code.set(event_target_value(&ev))
                                        required
                                    />
                                </div>
                                <button type="submit" class="btn btn-primary btn-block">"Verify"</button>
                            </form>
                        }.into_any()
                    } else {
                        view! {
                            <form on:submit=on_submit.clone() class="auth-form">
                                <div class="field">
                                    <label for="email">"Email"</label>
                                    <input
                                        type="email"
                                        id="email"
                                        autocomplete="username"
                                        prop:value=email
                                        on:input=move |ev| email.set(event_target_value(&ev))
                                        required
                                    />
                                </div>
                                <div class="field">
                                    <label for="password">"Password"</label>
                                    <input
                                        type="password"
                                        id="password"
                                        autocomplete="current-password"
                                        prop:value=password
                                        on:input=move |ev| password.set(event_target_value(&ev))
                                        required
                                    />
                                </div>
                                <button type="submit" class="btn btn-primary btn-block">"Sign in"</button>
                            </form>
                        }.into_any()
                    }
                }}

                {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}

                <OAuthButtons/>

                <div class="auth-links">
                    <a href="/forgot-password">"Forgot password?"</a>
                    <span class="auth-links-sep">"·"</span>
                    <a href="/register">"Create an account"</a>
                </div>
            </div>
        </div>
    }
}
