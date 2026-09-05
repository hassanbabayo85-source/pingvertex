use crate::auth::api;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;

#[derive(Clone, PartialEq)]
enum VerifyStatus {
    Verifying,
    Success,
    Failed(String),
    MissingToken,
}

/// Public route — `/verify-email?token=...`. The link in the verification
/// email points here. No session/auth is required or checked: the token
/// itself is the proof of ownership (see
/// `crates/api/src/handlers/verify_email.rs`), matching the backend's own
/// doc comment that this endpoint is "public by design".
#[component]
pub fn VerifyEmailPage() -> impl IntoView {
    let query = use_query_map();
    let token = move || query.get().get("token").unwrap_or_default();
    let status = RwSignal::new(VerifyStatus::Verifying);

    Effect::new(move |_| {
        let token_val = token();
        if token_val.is_empty() {
            status.set(VerifyStatus::MissingToken);
            return;
        }
        spawn_local(async move {
            match api::verify_email(&token_val).await {
                Ok(()) => status.set(VerifyStatus::Success),
                Err(e) => status.set(VerifyStatus::Failed(e)),
            }
        });
    });

    view! {
        <div class="auth-shell">
            <div class="auth-card">
                <div class="auth-brand">
                    <span class="auth-brand-mark"><span class="pulse-dot"></span></span>
                    <span class="auth-brand-name">"PingVertex"</span>
                </div>
                <h1>"Verify your email"</h1>

                {move || match status.get() {
                    VerifyStatus::Verifying => view! {
                        <p class="muted">"Verifying your email address..."</p>
                    }.into_any(),
                    VerifyStatus::Success => view! {
                        <p>"Your email is verified. You can now sign in."</p>
                    }.into_any(),
                    VerifyStatus::MissingToken => view! {
                        <div class="form-error">
                            "This link is missing its verification token. Please use the \
                            link from your email exactly as sent, or request a new one from \
                            the Security page after signing in."
                        </div>
                    }.into_any(),
                    VerifyStatus::Failed(e) => view! {
                        <div class="form-error">
                            "We couldn't verify your email (" {e} "). The link may have \
                            expired — request a new one from the Security page after signing in."
                        </div>
                    }.into_any(),
                }}

                <div class="auth-links">
                    <a href="/login">"Back to sign in"</a>
                </div>
            </div>
        </div>
    }
}
