use crate::api::monitoring::{self, Monitor};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::filter_bar::FilterBar;
use crate::components::polling;
use crate::components::search_input::SearchInput;
use crate::components::status_badge::StatusBadge;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

const PAGE_SIZE: usize = 20;

#[component]
pub fn MonitorsPage() -> impl IntoView {
    let monitors = RwSignal::new(Vec::<Monitor>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let action_error = RwSignal::new(None::<String>);
    let pending_delete = RwSignal::new(None::<(String, String)>);
    let search = RwSignal::new(String::new());
    let status_filter = RwSignal::new("all".to_string());
    let kind_filter = RwSignal::new("all".to_string());
    let sort_key = RwSignal::new("name".to_string());
    let sort_asc = RwSignal::new(true);
    let page = RwSignal::new(0usize);
    let navigate = use_navigate();
    let refresh_in_flight = RwSignal::new(false);
    let navigate_new = navigate.clone();

    let poll_monitors = move || {
        let monitors = monitors;
        let error = error;
        let loading = loading;
        let refresh_in_flight = refresh_in_flight;
        async move {
            if refresh_in_flight.get_untracked() {
                return Ok(());
            }
            refresh_in_flight.set(true);
            match monitoring::fetch_monitors().await {
                Ok(data) => {
                    monitors.set(data);
                    error.set(None);
                    loading.set(false);
                    refresh_in_flight.set(false);
                    Ok(())
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                    refresh_in_flight.set(false);
                    Err(())
                }
            }
        }
    };

    spawn_local(async move {
        let _ = poll_monitors().await;
    });
    polling::start(poll_monitors);

    let set_sort = move |key: &'static str| {
        if sort_key.get_untracked() == key {
            sort_asc.update(|v| *v = !*v);
        } else {
            sort_key.set(key.to_string());
            sort_asc.set(true);
        }
    };

    view! {
        <div class="monitors-page">
            <header class="page-header">
                <h1>"Monitors"</h1>
                <button on:click=move |_| navigate_new("/monitors/new", Default::default())>
                    "New Monitor"
                </button>
            </header>

            {move || match action_error.get() {
                Some(e) => view! { <div class="error action-error">{e}</div> }.into_any(),
                None => ().into_any(),
            }}

            {move || match pending_delete.get() {
                Some((id, name)) => {
                    let id_confirm = id.clone();
                    view! {
                        <ConfirmDialog
                            title="Delete Monitor"
                            body=format!("Are you sure you want to delete \"{name}\"? This cannot be undone.")
                            on_confirm=Callback::new(move |_| {
                                let id_owned = id_confirm.clone();
                                spawn_local(async move {
                                    match monitoring::delete_monitor(&id_owned).await {
                                        Ok(()) => {
                                            monitors.update(|list| list.retain(|m| m.id != id_owned));
                                            pending_delete.set(None);
                                        }
                                        Err(e) => {
                                            action_error.set(Some(e));
                                            pending_delete.set(None);
                                        }
                                    }
                                });
                            })
                            on_cancel=Callback::new(move |_| pending_delete.set(None))
                        />
                    }.into_any()
                }
                None => ().into_any(),
            }}

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

                        let mut kinds: Vec<String> = list.iter().map(|m| m.kind.clone()).collect();
                        kinds.sort();
                        kinds.dedup();

                        let search_value = search.get();
                        let search_lower = search_value.to_lowercase();
                        let status_value = status_filter.get();
                        let kind_value = kind_filter.get();

                        let mut filtered: Vec<Monitor> = list
                            .iter()
                            .filter(|m| {
                                search_lower.is_empty() || m.name.to_lowercase().contains(&search_lower)
                            })
                            .filter(|m| status_value == "all" || effective_status(m) == status_value)
                            .filter(|m| kind_value == "all" || m.kind == kind_value)
                            .cloned()
                            .collect();

                        let key = sort_key.get();
                        let asc = sort_asc.get();
                        filtered.sort_by(|a, b| {
                            let ord = match key.as_str() {
                                "kind" => a.kind.to_lowercase().cmp(&b.kind.to_lowercase()),
                                "status" => effective_status(a).cmp(&effective_status(b)),
                                "uptime" => a
                                    .uptime_percentage
                                    .unwrap_or(-1.0)
                                    .partial_cmp(&b.uptime_percentage.unwrap_or(-1.0))
                                    .unwrap_or(std::cmp::Ordering::Equal),
                                "avg_response" => a
                                    .avg_response_time_ms
                                    .unwrap_or(-1.0)
                                    .partial_cmp(&b.avg_response_time_ms.unwrap_or(-1.0))
                                    .unwrap_or(std::cmp::Ordering::Equal),
                                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                            };
                            if asc { ord } else { ord.reverse() }
                        });

                        let total = filtered.len();
                        let total_pages = if total == 0 { 1 } else { total.div_ceil(PAGE_SIZE) };
                        let current_page = page.get().min(total_pages - 1);
                        let show_pagination = total > PAGE_SIZE;
                        let start = current_page * PAGE_SIZE;
                        let page_slice: Vec<Monitor> = if show_pagination {
                            filtered.iter().skip(start).take(PAGE_SIZE).cloned().collect()
                        } else {
                            filtered.clone()
                        };
                        let shown = page_slice.len();

                        let sort_indicator = move |k: &str| {
                            if sort_key.get() == k {
                                if sort_asc.get() { " ^" } else { " v" }
                            } else {
                                ""
                            }
                        };

                        view! {
                            <div class="monitors-content">
                                <div class="monitors-toolbar">
                                    <SearchInput
                                        value=search_value
                                        on_input=Callback::new(move |v: String| {
                                            search.set(v);
                                            page.set(0);
                                        })
                                        placeholder="Search by name..."
                                    />
                                    <FilterBar>
                                        <label>
                                            "Status"
                                            <select
                                                on:change=move |ev| {
                                                    status_filter.set(event_target_value(&ev));
                                                    page.set(0);
                                                }
                                            >
                                                <option value="all" selected=status_value == "all">"All Status"</option>
                                                <option value="up" selected=status_value == "up">"Up"</option>
                                                <option value="down" selected=status_value == "down">"Down"</option>
                                                <option value="degraded" selected=status_value == "degraded">"Degraded"</option>
                                                <option value="paused" selected=status_value == "paused">"Paused"</option>
                                                <option value="unknown" selected=status_value == "unknown">"Unknown"</option>
                                            </select>
                                        </label>
                                        <label>
                                            "Type"
                                            <select
                                                on:change=move |ev| {
                                                    kind_filter.set(event_target_value(&ev));
                                                    page.set(0);
                                                }
                                            >
                                                <option value="all" selected=kind_value == "all">"All Types"</option>
                                                {kinds.iter().map(|k| {
                                                    let k1 = k.clone();
                                                    let k2 = k.clone();
                                                    let is_selected = kind_value == k1;
                                                    view! {
                                                        <option value=k1 selected=is_selected>{k2}</option>
                                                    }
                                                }).collect_view()}
                                            </select>
                                        </label>
                                    </FilterBar>
                                </div>

                                {if shown == 0 {
                                    view! { <p>"No monitors match your search/filters."</p> }.into_any()
                                } else {
                                    view! {
                                        <table class="monitors-table data-table">
                                            <thead>
                                                <tr>
                                                    <th>
                                                        <button class="sort-header" on:click=move |_| set_sort("name")>
                                                            "Name" {move || sort_indicator("name")}
                                                        </button>
                                                    </th>
                                                    <th>
                                                        <button class="sort-header" on:click=move |_| set_sort("kind")>
                                                            "Kind" {move || sort_indicator("kind")}
                                                        </button>
                                                    </th>
                                                    <th>
                                                        <button class="sort-header" on:click=move |_| set_sort("status")>
                                                            "Status" {move || sort_indicator("status")}
                                                        </button>
                                                    </th>
                                                    <th>
                                                        <button class="sort-header" on:click=move |_| set_sort("uptime")>
                                                            "Uptime %" {move || sort_indicator("uptime")}
                                                        </button>
                                                    </th>
                                                    <th>
                                                        <button class="sort-header" on:click=move |_| set_sort("avg_response")>
                                                            "Avg Response (ms)" {move || sort_indicator("avg_response")}
                                                        </button>
                                                    </th>
                                                    <th>"Enabled"</th>
                                                    <th>"Actions"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {page_slice.into_iter().map(|m| {
                                                    let id = m.id.clone();
                                                    let name = m.name.clone();
                                                    let enabled_now = m.enabled;
                                                    let status_display = effective_status(&m);
                                                    let navigate_detail = navigate.clone();
                                                    let navigate_edit = navigate.clone();
                                                    let id_detail = id.clone();
                                                    let id_edit = id.clone();
                                                    let id_toggle = id.clone();
                                                    let id_delete = id.clone();
                                                    let name_delete = name.clone();
                                                    view! {
                                                        <tr>
                                                            <td>
                                                                <A href=format!("/monitors/{id}") attr:class="monitor-link">{m.name.clone()}</A>
                                                            </td>
                                                            <td>{m.kind.clone()}</td>
                                                            <td><StatusBadge status=status_display/></td>
                                                            <td>{m.uptime_percentage.map(|v| format!("{v:.2}")).unwrap_or_else(|| "-".into())}</td>
                                                            <td>{m.avg_response_time_ms.map(|v| format!("{v:.0}")).unwrap_or_else(|| "-".into())}</td>
                                                            <td>
                                                                <button
                                                                    class=if enabled_now { "btn btn-toggle btn-toggle-on" } else { "btn btn-toggle btn-toggle-off" }
                                                                    on:click=move |_| {
                                                                        let id_owned = id_toggle.clone();
                                                                        let new_val = !enabled_now;
                                                                        spawn_local(async move {
                                                                            match monitoring::set_monitor_enabled(&id_owned, new_val).await {
                                                                                Ok(()) => {
                                                                                    monitors.update(|list| {
                                                                                        if let Some(mon) = list.iter_mut().find(|x| x.id == id_owned) {
                                                                                            mon.enabled = new_val;
                                                                                        }
                                                                                    });
                                                                                }
                                                                                Err(e) => action_error.set(Some(e)),
                                                                            }
                                                                        });
                                                                    }
                                                                >
                                                                    {if enabled_now { "Enabled" } else { "Disabled" }}
                                                                </button>
                                                            </td>
                                                            <td class="actions-cell">
                                                                <button on:click=move |_| navigate_detail(&format!("/monitors/{id_detail}"), Default::default())>
                                                                    "View"
                                                                </button>
                                                                <button on:click=move |_| navigate_edit(&format!("/monitors/{id_edit}/edit"), Default::default())>
                                                                    "Edit"
                                                                </button>
                                                                <button
                                                                    class="btn btn-danger"
                                                                    on:click=move |_| pending_delete.set(Some((id_delete.clone(), name_delete.clone())))
                                                                >
                                                                    "Delete"
                                                                </button>
                                                            </td>
                                                        </tr>
                                                    }.into_any()
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    }.into_any()
                                }}

                                {if show_pagination {
                                    view! {
                                        <div class="pagination-controls">
                                            <span class="pagination-info">
                                                {format!("Showing {} of {} (page {} of {})", shown, total, current_page + 1, total_pages)}
                                            </span>
                                            <button
                                                disabled=current_page == 0
                                                on:click=move |_| {
                                                    if current_page > 0 {
                                                        page.set(current_page - 1);
                                                    }
                                                }
                                            >
                                                "Prev"
                                            </button>
                                            <button
                                                disabled=current_page + 1 >= total_pages
                                                on:click=move |_| {
                                                    if current_page + 1 < total_pages {
                                                        page.set(current_page + 1);
                                                    }
                                                }
                                            >
                                                "Next"
                                            </button>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="pagination-info">
                                            {format!("Showing {} of {}", shown, total)}
                                        </div>
                                    }.into_any()
                                }}
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

fn effective_status(m: &Monitor) -> String {
    if !m.enabled {
        return "paused".to_string();
    }
    m.status
        .clone()
        .unwrap_or_else(|| "unknown".to_string())
        .to_lowercase()
}
