use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn DataTable(
    #[prop(into)] headers: Vec<String>,
    #[prop(optional, into)] caption: Option<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="table-responsive">
            <table class="data-table">
                {caption.map(|text| view! { <caption>{text}</caption> })}
                <thead><tr>{headers.into_iter().map(|h| view! { <th scope="col">{h}</th> }).collect_view()}</tr></thead>
                <tbody>{children()}</tbody>
            </table>
        </div>
    }
}
