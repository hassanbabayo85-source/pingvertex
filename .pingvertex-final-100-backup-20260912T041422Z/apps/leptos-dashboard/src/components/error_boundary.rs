use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn GlobalErrorBoundary(children: Children) -> impl IntoView {
    view! {
        <ErrorBoundary fallback=|errors| {
            view! {
                <div class="error-boundary">
                    <h2>"Something went wrong."</h2>
                    <ul>
                        {move || {
                            errors.get()
                                .iter()
                                .map(|(_, e)| view! { <li>{e.to_string()}</li> })
                                .collect_view()
                        }}
                    </ul>
                </div>
            }
        }>
            {children()}
        </ErrorBoundary>
    }
}
