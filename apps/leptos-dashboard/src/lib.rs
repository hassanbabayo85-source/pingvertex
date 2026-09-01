mod api;
mod auth;
mod components;
mod pages;
mod public_api;
mod routes;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(routes::App);
}
