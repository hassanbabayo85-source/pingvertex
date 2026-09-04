use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};

fn focus_first_dialog_action() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Ok(Some(button)) = document.query_selector(".confirm-dialog-panel button:not([disabled])")
    else {
        return;
    };
    if let Ok(button) = button.dyn_into::<HtmlElement>() {
        let _ = button.focus();
    }
}

fn trap_dialog_focus(event: KeyboardEvent) {
    if event.key() != "Tab" {
        return;
    }

    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Ok(Some(panel)) = document.query_selector(".confirm-dialog-panel") else {
        return;
    };
    let Ok(focusable) = panel.query_selector_all(
        "button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex='-1'])",
    ) else {
        return;
    };

    let length = focusable.length();
    if length == 0 {
        event.prevent_default();
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

    let (Some(first), Some(last)) = (first, last) else {
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

#[allow(dead_code)]
#[component]
pub fn ConfirmDialog(
    #[prop(into)] title: String,
    #[prop(into)] body: String,
    on_confirm: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let cancel_for_key = on_cancel.clone();
    let confirm_for_click = on_confirm.clone();
    let cancel_for_click = on_cancel.clone();

    Effect::new(move |_| {
        focus_first_dialog_action();
    });

    view! {
        <div
            class="dialog-overlay"
            role="presentation"
            on:keydown=move |event| {
                if event.key() == "Escape" {
                    event.prevent_default();
                    cancel_for_key.run(());
                } else {
                    trap_dialog_focus(event);
                }
            }
        >
            <section
                class="dialog-panel confirm-dialog-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby="confirm-dialog-title"
                tabindex="-1"
            >
                <h2 class="dialog-title" id="confirm-dialog-title">{title}</h2>
                <p class="dialog-body">{body}</p>
                <div class="dialog-actions">
                    <button class="btn" type="button" on:click=move |_| cancel_for_click.run(())>
                        "Cancel"
                    </button>
                    <button class="btn btn-danger" type="button" on:click=move |_| confirm_for_click.run(())>
                        "Confirm"
                    </button>
                </div>
            </section>
        </div>
    }
}
