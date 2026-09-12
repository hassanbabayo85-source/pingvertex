use crate::auth::api;
use crate::auth::context::{use_auth, AuthState};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

#[component]
pub fn LogoutPage() -> impl IntoView {
    let auth = use_auth();
    let navigate = use_navigate();
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::logout().await {
                Ok(()) => {
                    auth.state.set(AuthState::Unauthenticated);
                    navigate("/login", Default::default());
                }
                Err(e) => error.set(Some(e)),
            }
        });
    });

    view! {
        <div class="auth-shell">
            <div class="auth-card auth-card-center">
                <div class="auth-brand">
                    <span class="auth-brand-mark"><span class="pulse-dot"></span></span>
                    <span class="auth-brand-name">"PingVertex"</span>
                </div>
                <h1>"Signing you out..."</h1>
                {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}
            </div>
        </div>
    }
}
