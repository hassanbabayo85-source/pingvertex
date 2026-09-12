use leptos::prelude::*;

#[allow(dead_code)]
#[component]
pub fn TableShell(#[prop(into)] headers: Vec<String>, children: Children) -> impl IntoView {
    view! {
        <div class="table-responsive">
            <table class="data-table">
                <thead>
                    <tr>
                        {headers.into_iter().map(|h| view! { <th>{h}</th> }).collect_view()}
                    </tr>
                </thead>
                <tbody>
                    {children()}
                </tbody>
            </table>
        </div>
    }
}
