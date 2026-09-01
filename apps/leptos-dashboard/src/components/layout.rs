use crate::auth::context::{use_auth, AuthState};
use leptos::prelude::*;
use leptos_router::components::{Outlet, A};

#[component]
pub fn Layout() -> impl IntoView {
    let auth = use_auth();

    view! {
        <div class="app-shell">
            <header class="app-header">
                <div class="app-brand">
                    <A href="/">"PingVertex"</A>
                </div>
                <nav class="app-nav">
                    <A href="/">"Overview"</A>
                    <A href="/monitors">"Monitors"</A>
                    <A href="/rum">"Real User Monitoring"</A>
                </nav>
                <div class="app-user">
                    {move || match auth.state.get() {
                        AuthState::Authenticated(u) => u.email,
                        _ => String::new(),
                    }}
                    <A href="/logout">"Sign out"</A>
                </div>
            </header>
            <main class="app-main">
                <Outlet/>
            </main>
            <footer class="app-footer">
                "PingVertex — Infrastructure Monitoring"
            </footer>
        </div>
    }
}
