mod api;
mod auth;
mod components;
mod pages;
mod public_api;
mod routes;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Sets `data-theme` before Leptos mounts so the first paint is already
/// dark (PingVertex branded UI). Reads `pingvertex-theme-v2`; if absent,
/// defaults to dark. The topbar toggle saves the user's choice here.
fn apply_initial_theme() {
    let Some(window) = web_sys::window() else { return; };
    let Some(document) = window.document() else { return; };
    let Some(root) = document.document_element() else { return; };

    let saved = js_sys::Reflect::get(&window, &JsValue::from_str("localStorage"))
        .ok()
        .and_then(|storage| {
            if storage.is_null() || storage.is_undefined() {
                return None;
            }
            let get_item = js_sys::Reflect::get(&storage, &JsValue::from_str("getItem")).ok()?;
            let get_item_fn = get_item.dyn_into::<js_sys::Function>().ok()?;
            get_item_fn
                .call1(&storage, &JsValue::from_str("pingvertex-theme-v2"))
                .ok()?
                .as_string()
        });

    let theme = match saved.as_deref() {
        Some("light") => "light",
        _ => "dark",
    };

    let _ = root.set_attribute("data-theme", theme);
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    apply_initial_theme();
    leptos::mount::mount_to_body(routes::App);
}
