use leptos::prelude::*;
use wasm_bindgen::prelude::*;

const GOOGLE_CLIENT_ID: &str = match option_env!("GOOGLE_CLIENT_ID") {
    Some(value) => value,
    None => "",
};

const APPLE_CLIENT_ID: &str = match option_env!("APPLE_CLIENT_ID") {
    Some(value) => value,
    None => "",
};

const APPLE_REDIRECT_URI: &str = match option_env!("APPLE_REDIRECT_URI") {
    Some(value) => value,
    None => "",
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = PingVertexOAuth, js_name = init)]
    fn init_oauth(google_client_id: &str, apple_client_id: &str, apple_redirect_uri: &str);

    #[wasm_bindgen(js_namespace = PingVertexOAuth, js_name = google)]
    fn google_sign_in();

    #[wasm_bindgen(js_namespace = PingVertexOAuth, js_name = apple)]
    fn apple_sign_in();
}

#[component]
pub fn OAuthButtons() -> impl IntoView {
    let google_enabled = !GOOGLE_CLIENT_ID.trim().is_empty();
    let apple_enabled = !APPLE_CLIENT_ID.trim().is_empty() && !APPLE_REDIRECT_URI.trim().is_empty();

    Effect::new(move |_| {
        init_oauth(GOOGLE_CLIENT_ID, APPLE_CLIENT_ID, APPLE_REDIRECT_URI);
    });

    view! {
        <div class="oauth-section" aria-label="Social sign in">

            <div class="auth-divider">
                <span>"or continue with"</span>
            </div>

            <div class="oauth-stack">

                <button
                    type="button"
                    class="oauth-provider oauth-google"
                    on:click=move |_| {
                        google_sign_in();
                    }
                >
                    <span class="oauth-icon google-icon">
                        "G"
                    </span>

                    <span class="oauth-provider-label">
                        "Continue with Google"
                    </span>
                </button>

                <button
                    type="button"
                    class="oauth-provider oauth-apple"
                    on:click=move |_| {
                        apple_sign_in();
                    }
                >
                    <span class="oauth-icon apple-icon">
                        "●"
                    </span>

                    <span class="oauth-provider-label">
                        "Continue with Apple"
                    </span>
                </button>

            </div>

            <p
                id="oauth-error"
                class="oauth-error"
                attr:role="alert"
                attr:aria-live="polite"
            ></p>

            {(!google_enabled || !apple_enabled).then(|| view! {
                <p class="oauth-config-note">
                    "Social sign-in is available once the provider credentials are configured."
                </p>
            })}

        </div>
    }
}
