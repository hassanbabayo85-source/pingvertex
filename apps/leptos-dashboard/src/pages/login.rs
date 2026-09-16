use crate::auth::api::{self, LoginResult};
use crate::auth::context::{use_auth, AuthState};
use crate::auth::oauth::OAuthButtons;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

// ---------------------------------------------------------------------
// Icons (self-contained per page, matches the convention already used
// in this codebase where each auth page owns its small icon helpers).
// ---------------------------------------------------------------------

fn pv2_logo_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="1.8">
            <circle cx="12" cy="12" r="7.5"></circle>
            <circle cx="12" cy="12" r="2.4" fill="white" stroke="none"></circle>
        </svg>
    }
}

fn pv2_mail_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <rect x="2.5" y="4.5" width="19" height="15" rx="2.5"></rect>
            <path d="M3 6.5l9 6.5 9-6.5"></path>
        </svg>
    }
}

fn pv2_lock_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <rect x="4.5" y="10.5" width="15" height="10" rx="2.5"></rect>
            <path d="M7.5 10.5V7.5a4.5 4.5 0 0 1 9 0v3"></path>
        </svg>
    }
}

fn pv2_eye_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M2 12s3.8-7 10-7 10 7 10 7-3.8 7-10 7-10-7-10-7z"></path>
            <circle cx="12" cy="12" r="3"></circle>
        </svg>
    }
}

fn pv2_eye_off_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 3l18 18"></path>
            <path d="M10.6 5.2A10.6 10.6 0 0 1 12 5c6.2 0 10 7 10 7a17.9 17.9 0 0 1-3.4 4.3M6.6 6.6C4 8.3 2 12 2 12s3.8 7 10 7c1.4 0 2.7-.3 3.9-.8"></path>
            <path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"></path>
        </svg>
    }
}

fn pv2_check_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 12.5l5 5L20 6"></path>
        </svg>
    }
}

fn pv2_arrow_right_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 12h14"></path>
            <path d="M13 6l6 6-6 6"></path>
        </svg>
    }
}

fn pv2_shield_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 3l7 3v6c0 5-3.5 7.5-7 9-3.5-1.5-7-4-7-9V6l7-3z"></path>
            <path d="M9 12l2 2 4-4"></path>
        </svg>
    }
}

fn pv2_pulse_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M2 12h4l2 7 4-14 2 7h8"></path>
        </svg>
    }
}

fn pv2_bolt_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M13 2 3 14h6l-1 8 10-13h-6l1-7z"></path>
        </svg>
    }
}

fn pv2_chart_icon() -> impl IntoView {
    view! {
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 20V10"></path>
            <path d="M12 20V4"></path>
            <path d="M20 20v-7"></path>
        </svg>
    }
}

/// Sunan alama "PingVertex" + tagline, ana amfani dashi a kan sassa uku
/// (login, forgot password, create account) domin daidaito na gani.
fn pv2_brand_header() -> impl IntoView {
    view! {
        <div class="pv2-logo-row">
            <span class="pv2-logo-icon">{pv2_logo_icon()}</span>
            <div class="pv2-logo-text">
                <span class="pv2-wordmark">
                    <span class="pv2-wordmark-dark">"Ping"</span>
                    <span class="pv2-wordmark-accent">"Vertex"</span>
                </span>
                <span class="pv2-tagline">
                    "Uptime" <span class="pv2-dot">"·"</span> "Monitoring" <span class="pv2-dot">"·"</span> "Peace of Mind"
                </span>
            </div>
        </div>
    }
}

/// Zanen kwamfutoci/duniya + jerin fasali huɗu a kasan katin, iri ɗaya a
/// kan sassa uku domin shafukan su yi daidai da juna.
#[allow(dead_code)]
fn pv2_illustration_footer() -> impl IntoView {
    view! {
        <div class="pv2-illustration">
            <div class="pv2-globe-panel">
                <svg viewBox="0 0 700 210" preserveAspectRatio="xMidYMid meet">
                    <defs>
                        <linearGradient id="pv2NodeGrad" x1="0" y1="0" x2="1" y2="1">
                            <stop offset="0" stop-color="#8b7bff"></stop>
                            <stop offset="1" stop-color="#4f8dfc"></stop>
                        </linearGradient>
                    </defs>

                    <path d="M95,110 C210,55 250,150 335,105" stroke="rgba(255,255,255,.28)" stroke-width="1.5" stroke-dasharray="4 6" fill="none"></path>
                    <path d="M605,120 C480,175 440,80 365,105" stroke="rgba(255,255,255,.28)" stroke-width="1.5" stroke-dasharray="4 6" fill="none"></path>

                    <g transform="translate(65,90)">
                        <rect width="42" height="42" rx="10" fill="url(#pv2NodeGrad)"></rect>
                        <rect x="9" y="12" width="24" height="4" rx="2" fill="rgba(255,255,255,.55)"></rect>
                        <rect x="9" y="21" width="16" height="4" rx="2" fill="rgba(255,255,255,.35)"></rect>
                    </g>
                    <g transform="translate(593,100)">
                        <rect width="42" height="42" rx="10" fill="url(#pv2NodeGrad)"></rect>
                        <rect x="9" y="12" width="24" height="4" rx="2" fill="rgba(255,255,255,.55)"></rect>
                        <rect x="9" y="21" width="16" height="4" rx="2" fill="rgba(255,255,255,.35)"></rect>
                    </g>

                    <g transform="translate(300,64)">
                        <rect y="62" width="86" height="28" rx="9" fill="url(#pv2NodeGrad)"></rect>
                        <rect y="34" width="86" height="28" rx="9" fill="url(#pv2NodeGrad)" opacity="0.92"></rect>
                        <rect y="6" width="86" height="28" rx="9" fill="url(#pv2NodeGrad)" opacity="0.82"></rect>
                        <rect x="10" y="14" width="30" height="4" rx="2" fill="rgba(255,255,255,.6)"></rect>
                        <rect x="10" y="42" width="30" height="4" rx="2" fill="rgba(255,255,255,.6)"></rect>
                        <rect x="10" y="70" width="30" height="4" rx="2" fill="rgba(255,255,255,.6)"></rect>
                        <circle cx="43" cy="-16" r="17" fill="#1fdb75"></circle>
                        <path d="M35,-16 l6,6 l13,-14" stroke="white" stroke-width="3.2" fill="none" stroke-linecap="round" stroke-linejoin="round"></path>
                    </g>
                </svg>
            </div>

            <div class="pv2-feature-row">
                <div class="pv2-feature">
                    <span class="pv2-feature-icon pv2-fi-green">{pv2_pulse_icon()}</span>
                    <span>"Real-time" <br/> "Monitoring"</span>
                </div>
                <div class="pv2-feature">
                    <span class="pv2-feature-icon pv2-fi-purple">{pv2_shield_icon()}</span>
                    <span>"Instant" <br/> "Alerts"</span>
                </div>
                <div class="pv2-feature">
                    <span class="pv2-feature-icon pv2-fi-blue">{pv2_chart_icon()}</span>
                    <span>"Detailed" <br/> "Reports"</span>
                </div>
                <div class="pv2-feature">
                    <span class="pv2-feature-icon pv2-fi-orange">{pv2_bolt_icon()}</span>
                    <span>"High" <br/> "Availability"</span>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn LoginPage() -> impl IntoView {
    let auth = use_auth();
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let show_password = RwSignal::new(false);
    let remember_me = RwSignal::new(true);
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
        <div class="pv2-shell">
            <div class="pv2-card">
                {pv2_brand_header()}

                {move || {
                    if two_factor_required.get() {
                        view! {
                            <h1 class="pv2-heading">"Two-Factor Verification"</h1>
                            <p class="pv2-subtitle">"Enter the code from your authenticator app to finish signing in."</p>
                            <form on:submit=on_2fa_submit.clone() class="pv2-form">
                                <div class="pv2-field">
                                    <label class="pv2-label" for="two-factor-code">{pv2_lock_icon()} "Authentication Code"</label>
                                    <div class="pv2-input-wrap">
                                        <span class="pv2-input-icon-box">{pv2_lock_icon()}</span>
                                        <input
                                            type="text"
                                            id="two-factor-code"
                                            autocomplete="one-time-code"
                                            placeholder="000000"
                                            prop:value=two_factor_code
                                            on:input=move |ev| two_factor_code.set(event_target_value(&ev))
                                            required
                                        />
                                    </div>
                                </div>
                                <button type="submit" class="pv2-submit">
                                    {pv2_arrow_right_icon()}
                                    <span>"Verify"</span>
                                </button>
                            </form>
                        }.into_any()
                    } else {
                        view! {
                            <h1 class="pv2-heading">"Welcome Back" <span class="pv2-wave">"👋"</span></h1>
                            <p class="pv2-subtitle">
                                "Monitor your infrastructure without the noise." <br/>
                                "Fast. Reliable. Intelligent."
                            </p>

                            <form on:submit=on_submit.clone() class="pv2-form">
                                <div class="pv2-field">
                                    <label class="pv2-label" for="email">{pv2_mail_icon()} "Email Address"</label>
                                    <div class="pv2-input-wrap">
                                        <span class="pv2-input-icon-box">{pv2_mail_icon()}</span>
                                        <input
                                            type="email"
                                            id="email"
                                            autocomplete="username"
                                            placeholder="you@example.com"
                                            prop:value=email
                                            on:input=move |ev| email.set(event_target_value(&ev))
                                            required
                                        />
                                    </div>
                                </div>

                                <div class="pv2-field">
                                    <label class="pv2-label" for="password">{pv2_lock_icon()} "Password"</label>
                                    <div class="pv2-input-wrap">
                                        <span class="pv2-input-icon-box">{pv2_lock_icon()}</span>
                                        <input
                                            id="password"
                                            autocomplete="current-password"
                                            type=move || if show_password.get() { "text" } else { "password" }
                                            placeholder="Enter your password"
                                            prop:value=password
                                            on:input=move |ev| password.set(event_target_value(&ev))
                                            required
                                        />
                                        <button
                                            type="button"
                                            class="pv2-toggle-btn"
                                            tabindex="-1"
                                            aria-label=move || if show_password.get() { "Hide password" } else { "Show password" }
                                            on:click=move |_| show_password.update(|v| *v = !*v)
                                        >
                                            {move || if show_password.get() { pv2_eye_off_icon().into_any() } else { pv2_eye_icon().into_any() }}
                                        </button>
                                    </div>
                                </div>

                                <div class="pv2-row-between">
                                    <label class="pv2-checkbox">
                                        <input
                                            type="checkbox"
                                            prop:checked=remember_me
                                            on:change=move |ev| remember_me.set(event_target_checked(&ev))
                                        />
                                        <span class="pv2-checkbox-box">{pv2_check_icon()}</span>
                                        "Remember me"
                                    </label>
                                    <a href="/forgot-password" class="pv2-link">"Forgot password?"</a>
                                </div>

                                <button type="submit" class="pv2-submit">
                                    {pv2_arrow_right_icon()}
                                    <span>"Sign in"</span>
                                </button>
                            </form>
                        }.into_any()
                    }
                }}

                {move || error.get().map(|e| view! { <div class="pv2-error">{e}</div> })}

                <OAuthButtons/>

                <p class="pv2-security-note">
                    {pv2_shield_icon()}
                    "Your data is protected with enterprise-grade security."
                </p>

                <p class="pv2-footer-line">
                    "Don't have an account? " <a href="/register" class="pv2-link">"Create an account"</a>
                </p>
            </div>

        </div>
    }
}
