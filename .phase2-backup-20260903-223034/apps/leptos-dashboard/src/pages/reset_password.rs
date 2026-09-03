use crate::auth::api;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_query_map};

#[component]
pub fn ResetPasswordPage() -> impl IntoView {
    let query = use_query_map();
    let token = move || query.get().get("token").unwrap_or_default();
    let new_password = RwSignal::new(String::new());
    let confirm_password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let token_val = token();
        let new_password_val = new_password.get();
        let confirm_password_val = confirm_password.get();
        let navigate = navigate.clone();

        if new_password_val != confirm_password_val {
            error.set(Some("Passwords do not match".to_string()));
            return;
        }

        spawn_local(async move {
            match api::reset_password(&token_val, &new_password_val).await {
                Ok(()) => navigate("/login", Default::default()),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="auth-shell">
            <div class="auth-card">
                <div class="auth-brand">
                    <span class="auth-brand-mark"><span class="pulse-dot"></span></span>
                    <span class="auth-brand-name">"PingVertex"</span>
                </div>
                <h1>"Set a new password"</h1>

                <form on:submit=on_submit class="auth-form">
                    <div class="field">
                        <label for="new-password">"New password"</label>
                        <input type="password" id="new-password" prop:value=new_password on:input=move |ev| new_password.set(event_target_value(&ev)) required/>
                    </div>
                    <div class="field">
                        <label for="confirm-password">"Confirm new password"</label>
                        <input type="password" id="confirm-password" prop:value=confirm_password on:input=move |ev| confirm_password.set(event_target_value(&ev)) required/>
                    </div>
                    <button type="submit" class="btn btn-primary btn-block">"Reset password"</button>
                </form>

                {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}

                <div class="auth-links">
                    <a href="/login">"Back to sign in"</a>
                </div>
            </div>
        </div>
    }
}
