use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};

#[component]
pub fn NetworkStatus() -> impl IntoView {
    let online = RwSignal::new(
        web_sys::window()
            .and_then(|window| js_sys::Reflect::get(&window.navigator(), &"onLine".into()).ok())
            .and_then(|value| value.as_bool())
            .unwrap_or(true),
    );

    Effect::new(move |_| {
        let Some(window) = web_sys::window() else {
            return;
        };

        let online_callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| online.set(true));
        let offline_callback =
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_| online.set(false));

        let _ = window
            .add_event_listener_with_callback("online", online_callback.as_ref().unchecked_ref());
        let _ = window
            .add_event_listener_with_callback("offline", offline_callback.as_ref().unchecked_ref());

        online_callback.forget();
        offline_callback.forget();
    });

    view! {
        {move || if !online.get() {
            view! {
                <div class="network-status network-status-offline" role="status" aria-live="polite">
                    <span class="network-status-dot" aria-hidden="true"></span>
                    <span>"Offline — showing the last available data."</span>
                </div>
            }.into_any()
        } else {
            ().into_any()
        }}
    }
}
