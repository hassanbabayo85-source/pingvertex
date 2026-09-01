use crate::api::monitoring::{self, Monitor};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

#[component]
pub fn MonitorsPage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<Monitor>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let navigate = use_navigate();
    let navigate_new = navigate.clone();

    Effect::new(move |_| {
        spawn_local(async move {
            match monitoring::fetch_monitors().await {
                Ok(data) => monitors.set(data),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

    view! {
        <div class="monitors-page">
            <header class="page-header">
                <h1>"Monitors"</h1>
                <button on:click=move |_| navigate_new("/monitors/new", Default::default())>
                    "New Monitor"
                </button>
            </header>
            {move || {
                if loading.get() {
                    view! { <div class="loading">"Loading monitors..."</div> }.into_any()
                } else if let Some(err) = error.get() {
                    view! { <div class="error">{err}</div> }.into_any()
                } else {
                    let list = monitors.get();
                    if list.is_empty() {
                        view! { <p>"No monitors yet."</p> }.into_any()
                    } else {
                        let navigate = navigate.clone();
                        view! {
                            <table class="monitors-table">
                                <thead>
                                    <tr>
                                        <th>"Name"</th>
                                        <th>"Kind"</th>
                                        <th>"Status"</th>
                                        <th>"Uptime %"</th>
                                        <th>"Avg Response (ms)"</th>
                                        <th>"Enabled"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {list.into_iter().map(|m| {
                                        let id = m.id.clone();
                                        let navigate_detail = navigate.clone();
                                        let navigate_edit = navigate.clone();
                                        let id_detail = id.clone();
                                        let id_edit = id.clone();
                                        view! {
                                            <tr>
                                                <td>
                                                    <a href=format!("/monitors/{id}")>{m.name.clone()}</a>
                                                </td>
                                                <td>{m.kind.clone()}</td>
                                                <td>{m.status.clone().unwrap_or_else(|| "unknown".to_string())}</td>
                                                <td>{m.uptime_percentage.map(|v| format!("{v:.2}")).unwrap_or_else(|| "—".into())}</td>
                                                <td>{m.avg_response_time_ms.map(|v| format!("{v:.0}")).unwrap_or_else(|| "—".into())}</td>
                                                <td>{if m.enabled { "Yes" } else { "No" }}</td>
                                                <td>
                                                    <button on:click=move |_| navigate_detail(&format!("/monitors/{id_detail}"), Default::default())>
                                                        "View"
                                                    </button>
                                                    <button on:click=move |_| navigate_edit(&format!("/monitors/{id_edit}/edit"), Default::default())>
                                                        "Edit"
                                                    </button>
                                                </td>
                                            </tr>
                                        }.into_any()
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}
