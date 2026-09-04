use crate::auth::context::{use_auth, AuthState};
use crate::components::navigation::{NavStatus, NAV_GROUPS};
use leptos::prelude::*;
use leptos_router::components::{Outlet, A};
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};

fn focus_element_by_id(id: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(element) = document.get_element_by_id(id) else {
        return;
    };
    if let Ok(element) = element.dyn_into::<HtmlElement>() {
        let _ = element.focus();
    }
}

fn trap_mobile_nav_focus(event: KeyboardEvent) {
    if event.key() != "Tab" {
        return;
    }

    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(drawer) = document.get_element_by_id("mobile-navigation") else {
        return;
    };

    let Ok(focusable) = drawer
        .query_selector_all("button:not([disabled]), a[href], [tabindex]:not([tabindex='-1'])")
    else {
        return;
    };

    let length = focusable.length();
    if length == 0 {
        return;
    }

    let Some(active) = document.active_element() else {
        return;
    };

    let first = focusable
        .item(0)
        .and_then(|element| element.dyn_into::<HtmlElement>().ok());
    let last = focusable
        .item(length.saturating_sub(1))
        .and_then(|element| element.dyn_into::<HtmlElement>().ok());

    let Some(first) = first else {
        return;
    };
    let Some(last) = last else {
        return;
    };

    if event.shift_key() && active.is_same_node(Some(&first)) {
        event.prevent_default();
        let _ = last.focus();
    } else if !event.shift_key() && active.is_same_node(Some(&last)) {
        event.prevent_default();
        let _ = first.focus();
    }
}

#[component]
pub fn Layout() -> impl IntoView {
    let auth = use_auth();
    let mobile_nav_open = RwSignal::new(false);
    let focus_return_armed = RwSignal::new(false);

    let open_mobile_nav = move |_| {
        mobile_nav_open.set(true);
    };

    let close_mobile_nav = move |_| {
        mobile_nav_open.set(false);
    };

    // Keep keyboard focus inside the mobile drawer while it is open, and
    // return focus to the menu trigger after closing. The drawer remains
    // mounted so the focus targets are stable and no async work is involved.
    Effect::new(move |_| {
        let is_open = mobile_nav_open.get();

        if is_open {
            focus_element_by_id("mobile-nav-close");
            focus_return_armed.set(true);
        } else if focus_return_armed.get_untracked() {
            focus_element_by_id("mobile-nav-toggle");
        }
    });

    view! {
        <div class="app-shell">
            <aside
                id="mobile-navigation"
                class:mobile-nav-open=move || mobile_nav_open.get()
                class="sidebar"
                aria-label="Primary navigation"
                on:keydown=trap_mobile_nav_focus
            >
                <div class="sidebar-brand">
                    <A href="/" on:click=close_mobile_nav>"PingVertex"</A>
                    <button
                        id="mobile-nav-close"
                        class="mobile-nav-close icon-button"
                        type="button"
                        aria-label="Close navigation menu"
                        on:click=close_mobile_nav
                    >
                        "×"
                    </button>
                </div>

                <nav
                    class="sidebar-nav"
                    aria-label="Application navigation"
                >
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
                                                <A href=item.href on:click=close_mobile_nav>{item.label}</A>
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
                    <A href="/logout" attr:class="sidebar-signout" on:click=close_mobile_nav>"Sign out"</A>
                </div>
            </aside>

            <div
                class="mobile-nav-backdrop"
                class:mobile-nav-backdrop-open=move || mobile_nav_open.get()
                aria-hidden="true"
                on:click=close_mobile_nav
            ></div>

            <div class="app-content">
                <div class="mobile-nav-toolbar">
                    <button
                        id="mobile-nav-toggle"
                        class="mobile-nav-toggle"
                        type="button"
                        aria-label="Open navigation menu"
                        aria-expanded=move || mobile_nav_open.get().to_string()
                        aria-controls="mobile-navigation"
                        on:click=open_mobile_nav
                    >
                        <span class="mobile-nav-toggle-icon" aria-hidden="true">"☰"</span>
                        <span>"Menu"</span>
                    </button>
                </div>

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
