use crate::api::account;
use crate::auth::api;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn SecurityPage() -> impl IntoView {
    let profile = RwSignal::new(None::<account::AccountProfile>);
    let profile_error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            match account::fetch_account_profile().await {
                Ok(user) => profile.set(Some(user)),
                Err(e) => profile_error.set(Some(e)),
            }
        });
    });

    view! {
        <div class="page-header">
            <h1>"Security"</h1>
        </div>

        {move || profile_error.get().map(|e| view! { <div class="form-error">{e}</div> })}

        <div class="data-section">
            <EmailVerificationCard profile=profile/>
        </div>
        <div class="data-section">
            <PasswordChangeCard/>
        </div>
        <div class="data-section">
            <TotpCard profile=profile/>
        </div>
        <div class="data-section">
            <div class="card">
                <h2>"Social sign-in"</h2>
                <p class="muted">"Google and Apple token-exchange endpoints exist on the backend, but this frontend has no provider client IDs or browser SDK configuration. No fake OAuth button is exposed until those deployment settings are supplied."</p>
            </div>
        </div>
    }
}

#[component]
fn EmailVerificationCard(profile: RwSignal<Option<account::AccountProfile>>) -> impl IntoView {
    let sending = RwSignal::new(false);
    let message = RwSignal::new(None::<String>);

    let on_resend = move |_| {
        let Some(user) = profile.get() else { return };
        sending.set(true);
        message.set(None);
        spawn_local(async move {
            // The backend returns the same neutral response regardless of
            // outcome (anti-enumeration) — show one neutral message here too,
            // rather than branching on Ok/Err in a way that could leak
            // account-existence info back through the UI.
            let _ = api::resend_verification(&user.email).await;
            sending.set(false);
            message.set(Some(
                "If this address needs verifying, a new email is on its way.".to_string(),
            ));
        });
    };

    view! {
        <div class="card">
            <h2>"Email verification"</h2>
            {move || match profile.get() {
                None => view! { <p class="muted">"Loading..."</p> }.into_any(),
                Some(user) if user.email_verified => view! {
                    <p>"Your email (" {user.email} ") is verified."</p>
                }.into_any(),
                Some(user) => view! {
                    <p>"Your email (" {user.email} ") is not verified yet."</p>
                    <button
                        class="btn btn-primary"
                        disabled=move || sending.get()
                        on:click=on_resend
                    >
                        {move || if sending.get() { "Sending..." } else { "Resend verification email" }}
                    </button>
                    {move || message.get().map(|m| view! { <p class="muted">{m}</p> })}
                }.into_any(),
            }}
        </div>
    }
}

#[component]
fn PasswordChangeCard() -> impl IntoView {
    let current_password = RwSignal::new(String::new());
    let new_password = RwSignal::new(String::new());
    let confirm_password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let success = RwSignal::new(false);
    let submitting = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        success.set(false);

        let current_val = current_password.get();
        let new_val = new_password.get();
        let confirm_val = confirm_password.get();

        if new_val != confirm_val {
            error.set(Some("New passwords do not match.".to_string()));
            return;
        }

        submitting.set(true);
        spawn_local(async move {
            match api::change_password(&current_val, &new_val).await {
                Ok(()) => {
                    success.set(true);
                    current_password.set(String::new());
                    new_password.set(String::new());
                    confirm_password.set(String::new());
                }
                Err(e) => error.set(Some(e)),
            }
            submitting.set(false);
        });
    };

    view! {
        <div class="card">
            <h2>"Change password"</h2>
            <form on:submit=on_submit class="auth-form">
                <div class="field">
                    <label for="current-password">"Current password"</label>
                    <input
                        type="password"
                        id="current-password"
                        prop:value=current_password
                        on:input=move |ev| current_password.set(event_target_value(&ev))
                        required
                    />
                </div>
                <div class="field">
                    <label for="new-password-security">"New password"</label>
                    <input
                        type="password"
                        id="new-password-security"
                        prop:value=new_password
                        on:input=move |ev| new_password.set(event_target_value(&ev))
                        required
                    />
                </div>
                <div class="field">
                    <label for="confirm-new-password">"Confirm new password"</label>
                    <input
                        type="password"
                        id="confirm-new-password"
                        prop:value=confirm_password
                        on:input=move |ev| confirm_password.set(event_target_value(&ev))
                        required
                    />
                </div>
                <button type="submit" class="btn btn-primary" disabled=move || submitting.get()>
                    {move || if submitting.get() { "Updating..." } else { "Update password" }}
                </button>
            </form>
            {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}
            {move || success.get().then(|| view! { <p class="muted">"Password updated."</p> })}
        </div>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TotpStep {
    Idle,
    AwaitingCode,
    ShowingBackupCodes,
    Disabling,
}

#[component]
fn TotpCard(profile: RwSignal<Option<account::AccountProfile>>) -> impl IntoView {
    let step = RwSignal::new(TotpStep::Idle);
    let otpauth_uri = RwSignal::new(String::new());
    let totp_code = RwSignal::new(String::new());
    let backup_codes = RwSignal::new(Vec::<String>::new());
    let disable_password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let on_start_enable = move |_| {
        error.set(None);
        busy.set(true);
        spawn_local(async move {
            match api::enable_totp().await {
                Ok(uri) => {
                    otpauth_uri.set(uri);
                    step.set(TotpStep::AwaitingCode);
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let on_confirm = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let code = totp_code.get();
        busy.set(true);
        spawn_local(async move {
            match api::confirm_totp(&code).await {
                Ok(codes) => {
                    backup_codes.set(codes);
                    profile.update(|p| {
                        if let Some(u) = p {
                            u.totp_enabled = true;
                        }
                    });
                    step.set(TotpStep::ShowingBackupCodes);
                    totp_code.set(String::new());
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let on_disable = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let password = disable_password.get();
        busy.set(true);
        spawn_local(async move {
            match api::disable_totp(&password).await {
                Ok(()) => {
                    profile.update(|p| {
                        if let Some(u) = p {
                            u.totp_enabled = false;
                        }
                    });
                    step.set(TotpStep::Idle);
                    disable_password.set(String::new());
                }
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="card">
            <h2>"Two-factor authentication"</h2>

            {move || match profile.get() {
                Some(user) if user.totp_enabled => view! { <p class="muted">"2FA is enabled for this account."</p> }.into_any(),
                Some(_) => view! { <p class="muted">"2FA is currently disabled."</p> }.into_any(),
                None => view! { <p class="muted">"Loading security status…"</p> }.into_any(),
            }}

            {move || match step.get() {
                TotpStep::Idle => view! {
                    <div class="totp-actions">
                        <button class="btn btn-primary" disabled=move || busy.get() on:click=on_start_enable>
                            "Set up 2FA"
                        </button>
                        <button
                            class="btn btn-danger"
                            disabled=move || busy.get()
                            on:click=move |_| step.set(TotpStep::Disabling)
                        >
                            "Turn off 2FA"
                        </button>
                    </div>
                }.into_any(),

                TotpStep::AwaitingCode => view! {
                    <div class="totp-setup">
                        <p>"Add this to your authenticator app, then enter the 6-digit code it shows:"</p>
                        <code class="totp-uri">{otpauth_uri}</code>
                        <p class="muted">
                            "We show this as text rather than a QR code so your 2FA secret is \
                            never sent to a third-party image service — most authenticator \
                            apps accept manual/URI entry."
                        </p>
                        <form on:submit=on_confirm class="auth-form">
                            <div class="field">
                                <label for="totp-code">"6-digit code"</label>
                                <input
                                    type="text"
                                    id="totp-code"
                                    inputmode="numeric"
                                    prop:value=totp_code
                                    on:input=move |ev| totp_code.set(event_target_value(&ev))
                                    required
                                />
                            </div>
                            <button type="submit" class="btn btn-primary" disabled=move || busy.get()>
                                "Confirm and enable"
                            </button>
                        </form>
                    </div>
                }.into_any(),

                TotpStep::ShowingBackupCodes => view! {
                    <div class="totp-backup-codes">
                        <p>"Save these backup codes now — they will not be shown again:"</p>
                        <ul class="backup-code-list">
                            {backup_codes.get().into_iter().map(|c| view! { <li><code>{c}</code></li> }).collect_view()}
                        </ul>
                        <button
                            class="btn btn-primary"
                            on:click=move |_| step.set(TotpStep::Idle)
                        >
                            "I've saved these"
                        </button>
                    </div>
                }.into_any(),

                TotpStep::Disabling => view! {
                    <form on:submit=on_disable class="auth-form">
                        <p class="muted">"Confirm your current password to turn off 2FA."</p>
                        <div class="field">
                            <label for="disable-totp-password">"Current password"</label>
                            <input
                                type="password"
                                id="disable-totp-password"
                                prop:value=disable_password
                                on:input=move |ev| disable_password.set(event_target_value(&ev))
                                required
                            />
                        </div>
                        <div class="totp-actions">
                            <button type="submit" class="btn btn-danger" disabled=move || busy.get()>
                                "Turn off 2FA"
                            </button>
                            <button
                                type="button"
                                class="btn"
                                on:click=move |_| step.set(TotpStep::Idle)
                            >
                                "Cancel"
                            </button>
                        </div>
                    </form>
                }.into_any(),
            }}

            {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}
        </div>
    }
}
