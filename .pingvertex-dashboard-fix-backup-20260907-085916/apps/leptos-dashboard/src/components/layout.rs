use crate::auth::context::{use_auth, AuthState};
use crate::components::navigation::{NavStatus, NAV_GROUPS};
use crate::components::network_status::NetworkStatus;
use leptos::prelude::*;
use leptos_router::components::{Outlet, A};
use leptos_router::hooks::use_location;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};


fn apply_theme(dark: bool) {
    let Some(window) = web_sys::window() else { return; };
    let Some(document) = window.document() else { return; };
    let Some(root) = document.document_element() else { return; };
    if dark {
        let _ = root.set_attribute("data-theme", "dark");
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item("pingvertex-theme", "dark");
        }
    } else {
        let _ = root.set_attribute("data-theme", "light");
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item("pingvertex-theme", "light");
        }
    }
}

fn initial_dark_theme() -> bool {
    let Some(window) = web_sys::window() else { return false; };
    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(value)) = storage.get_item("pingvertex-theme") {
            return value == "dark";
        }
    }
    window
        .match_media("(prefers-color-scheme: dark)")
        .ok()
        .flatten()
        .map(|query| query.matches())
        .unwrap_or(false)
}

fn is_nav_active(current: &str, href: &str) -> bool {
    let matches = if href == "/" { current == "/" } else { current == href || current.starts_with(&format!("{href}/")) };
    if !matches { return false; }
    !NAV_GROUPS.iter().flat_map(|g| g.items.iter()).any(|other| {
        other.href != href
            && other.href.len() > href.len()
            && (other.href == current || current.starts_with(&format!("{}/", other.href)))
    })
}

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

    let Ok(focusable) = drawer.query_selector_all(
        "button:not([disabled]), a[href], input:not([disabled]), [tabindex]:not([tabindex='-1'])",
    ) else {
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
        .and_then(|e| e.dyn_into::<HtmlElement>().ok());

    let last = focusable
        .item(length.saturating_sub(1))
        .and_then(|e| e.dyn_into::<HtmlElement>().ok());

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
    let dark_mode = RwSignal::new(initial_dark_theme());
    let location = use_location();

    Effect::new(move |_| {
        apply_theme(dark_mode.get());
    });

    let toggle_theme = move |_| {
        let next = !dark_mode.get_untracked();
        dark_mode.set(next);
        apply_theme(next);
    };

    let open_mobile_nav = move |_| {
        mobile_nav_open.set(true);
    };

    let close_mobile_nav = move |_| {
        mobile_nav_open.set(false);
    };

    Effect::new(move |_| {
        if mobile_nav_open.get() {
            focus_element_by_id("mobile-nav-close");
            focus_return_armed.set(true);
        } else if focus_return_armed.get_untracked() {
            focus_element_by_id("mobile-nav-toggle");
        }
    });

    view! {
        <div class="app-shell">

            // ============================================================
            // SIDEBAR
            // ============================================================

            <aside
                id="mobile-navigation"
                class:mobile-nav-open=move || mobile_nav_open.get()
                class="sidebar"
                attr:aria-label="Primary navigation"
                on:keydown=trap_mobile_nav_focus
            >

                <div class="sidebar-brand">

                    <A
                        href="/"
                        attr:class="brand-link"
                        on:click=close_mobile_nav
                        attr:aria-label="PingVertex dashboard"
                    >
                        <svg
                            class="brand-mark"
                            viewBox="0 0 64 64"
                            attr:aria-hidden="true"
                        >
                            <path
                                d="M20 48 L31 31 L44 48"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="5"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                opacity=".48"
                            />

                            <circle
                                cx="20"
                                cy="49"
                                r="5.5"
                                fill="currentColor"
                                opacity=".72"
                            />

                            <circle
                                cx="44"
                                cy="49"
                                r="5.5"
                                fill="currentColor"
                                opacity=".72"
                            />

                            <circle
                                cx="31"
                                cy="30"
                                r="8.5"
                                fill="#5b5cf0"
                                stroke="#0b1020"
                                stroke-width="2"
                            />

                            <path
                                d="M36 20 C39 22 42 24 44 27"
                                fill="none"
                                stroke="#5b5cf0"
                                stroke-width="3.5"
                                stroke-linecap="round"
                            />

                            <path
                                d="M34 14 C39 16 44 19 48 23"
                                fill="none"
                                stroke="#4649ba"
                                stroke-width="3.5"
                                stroke-linecap="round"
                            />

                            <circle
                                cx="46"
                                cy="25"
                                r="3.5"
                                fill="#ff6b4a"
                            />
                        </svg>

                        <span class="brand-copy">
                            <strong>"PingVertex"</strong>
                            <small>"MONITORING"</small>
                        </span>
                    </A>

                    <button
                        id="mobile-nav-close"
                        class="mobile-nav-close icon-button"
                        type="button"
                        attr:aria-label="Close navigation menu"
                        on:click=close_mobile_nav
                    >
                        "×"
                    </button>
                </div>

                <div class="sidebar-live">
                    <span class="live-dot" attr:aria-hidden="true"></span>
                    <span>"Monitoring active"</span>
                </div>

                <nav
                    class="sidebar-nav"
                    attr:aria-label="Application navigation"
                >
                    {NAV_GROUPS
                        .iter()
                        .map(|group| {
                            view! {
                                <div class="sidebar-group">

                                    <span class="sidebar-group-label">
                                        {group.label}
                                    </span>

                                    {group
                                        .items
                                        .iter()
                                        .map(|item| match item.status {
                                            NavStatus::Available => view! {
                                                <A
                                                    href=item.href
                                                    class:active=move || is_nav_active(&location.pathname.get(), item.href)
                                                    attr:aria-current=move || if is_nav_active(&location.pathname.get(), item.href) { "page" } else { "false" }
                                                    on:click=close_mobile_nav
                                                >
                                                    {item.label}
                                                </A>
                                            }
                                            .into_any(),
                                        })
                                        .collect_view()}

                                </div>
                            }
                        })
                        .collect_view()}
                </nav>

                <div class="sidebar-footer">

                    <div class="sidebar-user">

                        <span
                            class="sidebar-avatar"
                            attr:aria-hidden="true"
                        >
                            {move || match auth.state.get() {
                                AuthState::Authenticated(u) => {
                                    u.email
                                        .chars()
                                        .next()
                                        .unwrap_or('U')
                                        .to_uppercase()
                                        .to_string()
                                }
                                _ => "U".to_string(),
                            }}
                        </span>

                        <span class="sidebar-user-meta">

                            <strong>
                                {move || match auth.state.get() {
                                    AuthState::Authenticated(u) => {
                                        u.email
                                            .split('@')
                                            .next()
                                            .unwrap_or("User")
                                            .to_string()
                                    }
                                    _ => "User".to_string(),
                                }}
                            </strong>

                            <small>
                                {move || match auth.state.get() {
                                    AuthState::Authenticated(u) => u.email,
                                    _ => String::new(),
                                }}
                            </small>

                        </span>
                    </div>

                    <A
                        href="/logout"
                        attr:class="sidebar-signout"
                        on:click=close_mobile_nav
                    >
                        <span attr:aria-hidden="true">"↪"</span>
                        <span>"Sign out"</span>
                    </A>

                </div>

            </aside>

            <div
                class="mobile-nav-backdrop"
                class:mobile-nav-backdrop-open=move || mobile_nav_open.get()
                attr:aria-hidden="true"
                on:click=close_mobile_nav
            ></div>

            <div class="app-content">

                <NetworkStatus/>

                // ========================================================
                // GLOBAL TOP BAR
                // ========================================================

                <header class="topbar">

                    <div class="mobile-nav-toolbar">

                        <button
                            id="mobile-nav-toggle"
                            class="mobile-nav-toggle"
                            type="button"
                            attr:aria-label="Open navigation menu"
                            attr:aria-expanded=move || mobile_nav_open.get().to_string()
                            attr:aria-controls="mobile-navigation"
                            on:click=open_mobile_nav
                        >
                            <span
                                class="mobile-nav-toggle-icon"
                                attr:aria-hidden="true"
                            >
                                "☰"
                            </span>

                            <span>"Menu"</span>
                        </button>

                    </div>

                    <div
                        class="topbar-search"
                        attr:role="search"
                        attr:aria-label="Search"
                    >
                        <span
                            class="topbar-search-icon"
                            attr:aria-hidden="true"
                        >
                            "⌕"
                        </span>

                        <span>
                            "Search monitors, incidents, or anything..."
                        </span>

                        <kbd>"⌘ K"</kbd>
                    </div>

                    <div class="topbar-actions">

                        <button
                            class="topbar-icon theme-toggle"
                            type="button"
                            on:click=toggle_theme
                            attr:aria-label=move || if dark_mode.get() { "Switch to light mode" } else { "Switch to dark mode" }
                            attr:title=move || if dark_mode.get() { "Light mode" } else { "Dark mode" }
                        >
                            {move || if dark_mode.get() { "☀" } else { "☾" }}
                        </button>

                        <button
                            class="topbar-icon"
                            type="button"
                            attr:aria-label="Notifications"
                        >
                            "♢"
                        </button>

                        <button
                            class="topbar-icon"
                            type="button"
                            attr:aria-label="Help"
                        >
                            "?"
                        </button>

                        <div class="topbar-workspace">

                            <span class="workspace-avatar">
                                "P"
                            </span>

                            <span class="workspace-name">
                                "PingVertex Workspace"
                            </span>

                            <span attr:aria-hidden="true">
                                "⌄"
                            </span>

                        </div>

                    </div>

                </header>

                <main class="app-main">
                    <Outlet/>
                </main>

                <footer class="app-footer">

                    <span>"PingVertex"</span>
                    <span>"Infrastructure Monitoring"</span>
                    <span>"·"</span>
                    <span>
                        "All systems are monitored continuously"
                    </span>

                </footer>

            </div>
        </div>
    }
}
