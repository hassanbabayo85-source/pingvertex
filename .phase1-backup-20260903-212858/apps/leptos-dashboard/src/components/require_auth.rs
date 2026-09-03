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

    Effect::new(move |_| {
        spawn_local(async move {
            match api::fetch_current_user().await {
                Ok(user) => auth.state.set(AuthState::Authenticated(user)),
                Err(_) => auth.state.set(AuthState::Unauthenticated),
            }
            loading.set(false);
        });
    });

    view! {
        {move || {
            if loading.get() {
                view! {
                    <div class="auth-loading">
                        <LoadingSpinner text="Checking your session..."/>
                    </div>
                }.into_any()
            } else {
                match auth.state.get() {
                    AuthState::Authenticated(_) => view! { <Layout/> }.into_any(),
                    AuthState::Unauthenticated => view! { <Redirect path="/login"/> }.into_any(),
                    _ => view! {
                        <div class="auth-loading">
                            <LoadingSpinner text="Checking your session..."/>
                        </div>
                    }.into_any(),
                }
            }
        }}
    }
}
