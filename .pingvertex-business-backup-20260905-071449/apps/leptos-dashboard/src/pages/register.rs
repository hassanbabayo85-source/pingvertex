use crate::auth::api;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn RegisterPage() -> impl IntoView {
    let name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let success = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let name_val = name.get();
        let email_val = email.get();
        let password_val = password.get();
        spawn_local(async move {
            match api::register(&name_val, &email_val, &password_val).await {
                Ok(()) => {
                    success.set(true);
                    error.set(None);
                }
                Err(e) => {
                    error.set(Some(e));
                    success.set(false);
                }
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
                <h1>"Create your account"</h1>
                <p class="auth-subtitle">"Start watching your services in minutes."</p>

                {move || {
                    if success.get() {
                        view! {
                            <p class="auth-success">"Registration successful! You can now " <a href="/login">"sign in"</a> "."</p>
                        }.into_any()
                    } else {
                        view! {
                            <form on:submit=on_submit class="auth-form">
                                <div class="field">
                                    <label for="name">"Name"</label>
                                    <input type="text" id="name" autocomplete="name" prop:value=name on:input=move |ev| name.set(event_target_value(&ev)) required/>
                                </div>
                                <div class="field">
                                    <label for="email">"Email"</label>
                                    <input type="email" id="email" autocomplete="email" prop:value=email on:input=move |ev| email.set(event_target_value(&ev)) required/>
                                </div>
                                <div class="field">
                                    <label for="password">"Password"</label>
                                    <input type="password" id="password" autocomplete="new-password" prop:value=password on:input=move |ev| password.set(event_target_value(&ev)) required/>
                                </div>
                                <button type="submit" class="btn btn-primary btn-block">"Create account"</button>
                            </form>
                        }.into_any()
                    }
                }}

                {move || error.get().map(|e| view! { <div class="form-error">{e}</div> })}

                <div class="auth-links">
                    <a href="/login">"Already have an account? Sign in"</a>
                </div>
            </div>
        </div>
    }
}
