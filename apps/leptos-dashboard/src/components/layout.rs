use crate::auth::context::{use_auth, AuthState};
use crate::components::navigation::{NavStatus, NAV_GROUPS};
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
                    {NAV_GROUPS
                        .iter()
                        .map(|group| {
                            view! {
                                <div class="sidebar-group">
                                    <span class="sidebar-group-label">{group.label}</span>
                                    {group
                                        .items
                                        .iter()
                                        .map(|item| match item.status {
                                            NavStatus::Available => view! {
                                                <A href=item.href>{item.label}</A>
                                            }.into_any(),
                                            NavStatus::ComingSoon => view! {
                                                <span
                                                    class="sidebar-group-disabled"
                                                    title="Coming soon"
                                                    aria-disabled="true"
                                                >
                                                    {item.label}
                                                    <span class="sidebar-soon-badge">"soon"</span>
                                                </span>
                                            }.into_any(),
                                        })
                                        .collect_view()}
                                </div>
                            }
                        })
                        .collect_view()}
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
