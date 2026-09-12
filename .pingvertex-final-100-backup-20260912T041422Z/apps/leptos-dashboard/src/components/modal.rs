use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Modal(
    #[prop(into)] title: String,
    #[prop(optional, into)] labelled_by: Option<String>,
    #[prop(optional)] on_close: Option<Callback<()>>,
    children: Children,
) -> impl IntoView {
    let title_id = labelled_by.unwrap_or_else(|| "modal-title".into());
    let overlay_close = on_close.clone();
    let button_close = on_close.clone();
    view! {
        <div class="modal-overlay" role="presentation" on:click=move |_| {
            if let Some(cb) = overlay_close.as_ref() {
                cb.run(());
            }
        }>
            <section
                class="modal-panel"
                role="dialog"
                aria-modal="true"
                aria-labelledby=title_id.clone()
                on:click=move |ev| ev.stop_propagation()
            >
                <header class="modal-header">
                    <h2 class="modal-title" id=title_id>{title}</h2>
                    {button_close.map(|cb| view! {
                        <button class="icon-button" type="button" aria-label="Close" on:click=move |_| cb.run(())>
                            "×"
                        </button>
                    })}
                </header>
                <div class="modal-body">{children()}</div>
            </section>
        </div>
    }
}
