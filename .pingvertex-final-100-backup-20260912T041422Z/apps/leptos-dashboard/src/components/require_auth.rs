use crate::auth::api;
use crate::auth::context::{use_auth, AuthState};
use crate::components::layout::Layout;
use crate::components::loading::LoadingSpinner;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::Redirect;

#[component]
pub fn RequireAuth() -> impl IntoView {
    let auth = use_auth();
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    let check_session = move || {
        loading.set(true);
        error.set(None);
        spawn_local(async move {
            match api::fetch_current_user_typed().await {
                Ok(user) => auth.state.set(AuthState::Authenticated(user)),
                Err(message) => {
                    // Only a real 401 is allowed to transition the global auth
                    // state to logged-out. Infrastructure failures remain retryable.
                    if matches!(message, crate::api::client::ApiError::Unauthorized) {
                        auth.state.set(AuthState::Unauthenticated);
                    } else {
                        error.set(Some(
                            if matches!(message, crate::api::client::ApiError::Network(_)) {
                                "Unable to reach PingVertex. Check your connection and try again."
                                    .to_string()
                            } else {
                                "We could not verify your session right now. Please try again."
                                    .to_string()
                            },
                        ));
                    }
                }
            }
            loading.set(false);
        });
    };

    Effect::new(move |_| check_session());

    view! {
        {move || {
            if loading.get() {
                view! { <div class="auth-loading"><LoadingSpinner text="Checking your session..."/></div> }.into_any()
            } else if let Some(message) = error.get() {
                view! {
                    <div class="auth-loading">
                        <div class="error-state">
                            <h2>"Session check failed"</h2>
                            <p>{message}</p>
                            <button class="btn btn-primary" on:click=move |_| check_session()>"Try again"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                match auth.state.get() {
                    AuthState::Authenticated(_) => view! { <Layout/> }.into_any(),
                    AuthState::Unauthenticated => view! { <Redirect path="/login"/> }.into_any(),
                    _ => view! { <div class="auth-loading"><LoadingSpinner text="Checking your session..."/></div> }.into_any(),
                }
            }
        }}
    }
}
