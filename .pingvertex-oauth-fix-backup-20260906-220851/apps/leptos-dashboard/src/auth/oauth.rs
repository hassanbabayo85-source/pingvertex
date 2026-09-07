use leptos::prelude::*;
use wasm_bindgen::prelude::*;

const GOOGLE_CLIENT_ID: &str = option_env!("GOOGLE_CLIENT_ID").unwrap_or("");
const APPLE_CLIENT_ID: &str = option_env!("APPLE_CLIENT_ID").unwrap_or("");
const APPLE_REDIRECT_URI: &str = option_env!("APPLE_REDIRECT_URI").unwrap_or("");

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = PingVertexOAuth, js_name = init)]
    fn init_oauth(google_client_id: &str, apple_client_id: &str, apple_redirect_uri: &str);
}

#[component]
pub fn OAuthButtons() -> impl IntoView {
    let google_enabled = !GOOGLE_CLIENT_ID.trim().is_empty();
    let apple_enabled = !APPLE_CLIENT_ID.trim().is_empty() && !APPLE_REDIRECT_URI.trim().is_empty();

    Effect::new(move |_| {
        init_oauth(GOOGLE_CLIENT_ID, APPLE_CLIENT_ID, APPLE_REDIRECT_URI);
    });

    if !google_enabled && !apple_enabled {
        return ().into_any();
    }

    view! {
        <div class="oauth-section" aria-label="Social sign in">
            <div class="auth-divider"><span>"or continue with"</span></div>
            <div class="oauth-stack">
                {google_enabled.then(|| view! { <div id="google-signin" class="oauth-provider-slot"></div> })}
                {apple_enabled.then(|| view! {
                    <div
                        id="appleid-signin"
                        class="oauth-provider-slot"
                        data-color="black"
                        data-border="true"
                        data-type="continue"
                        data-mode="center-align"
                    ></div>
                })}
            </div>
            <p id="oauth-error" class="oauth-error" role="alert" aria-live="polite"></p>
        </div>
    }.into_any()
}
