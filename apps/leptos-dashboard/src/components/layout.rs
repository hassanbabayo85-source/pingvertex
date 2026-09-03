use crate::auth::context::{use_auth, AuthState};
use leptos::prelude::*;
use leptos_router::components::{Outlet, A};

#[component]
pub fn Layout() -> impl IntoView {
    let auth = use_auth();

    view! {
        <div class="app-shell">
            <aside class="sidebar">
                <div class="sidebar-brand">
                    <A href="/">"PingVertex"</A>
                </div>

                <nav class="sidebar-nav">
                    <div class="sidebar-group">
                        <span class="sidebar-group-label">"OVERVIEW"</span>
                        <A href="/" exact=true>"Dashboard"</A>
                    </div>
                    <div class="sidebar-group">
                        <span class="sidebar-group-label">"MONITORING"</span>
                        <A href="/monitors">"Monitors"</A>
                    </div>
                    <div class="sidebar-group">
                        <span class="sidebar-group-label">"USER EXPERIENCE"</span>
                        <A href="/rum">"Real User Monitoring"</A>
                    </div>
                </nav>

                <div class="sidebar-footer">
                    <div class="sidebar-user">
                        <span class="sidebar-user-email">
                            {move || match auth.state.get() {
                                AuthState::Authenticated(u) => u.email,
                                _ => String::new(),
                            }}
                        </span>
                    </div>
                    <A href="/logout" attr:class="sidebar-signout">"Sign out"</A>
                </div>
            </aside>

            <div class="app-content">
                <main class="app-main">
                    <Outlet/>
                </main>
                <footer class="app-footer">
                    "PingVertex — Infrastructure Monitoring"
                </footer>
            </div>
        </div>
    }
}
