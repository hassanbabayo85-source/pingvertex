use leptos::prelude::*;

#[component]
#[allow(dead_code)]
pub fn Pagination(
    #[prop(into)] current_page: u32,
    #[prop(into)] total_pages: u32,
    on_change: Callback<u32>,
) -> impl IntoView {
    let previous = current_page.saturating_sub(1).max(1);
    let next = (current_page + 1).min(total_pages.max(1));
    view! {
        <nav class="pagination" aria-label="Pagination">
            <button type="button" disabled=current_page <= 1 on:click=move |_| on_change.run(previous)>"Previous"</button>
            <span aria-current="page">{format!("Page {} of {}", current_page, total_pages.max(1))}</span>
            <button type="button" disabled=current_page >= total_pages.max(1) on:click=move |_| on_change.run(next)>"Next"</button>
        </nav>
    }
}
