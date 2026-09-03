use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn ConfirmDialog(
    #[prop(into)] title: String,
    #[prop(into)] body: String,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="dialog-overlay" role="dialog" aria-modal="true" aria-labelledby="confirm-dialog-title">
            <div class="dialog-panel">
                <h2 class="dialog-title" id="confirm-dialog-title">{title}</h2>
                <p class="dialog-body">{body}</p>
                <div class="dialog-actions">
                    <button class="btn" on:click=move |_| on_cancel.run(())>
                        "Cancel"
                    </button>
                    <button class="btn btn-danger" on:click=move |_| on_confirm.run(())>
                        "Confirm"
                    </button>
                </div>
            </div>
        </div>
    }
}
