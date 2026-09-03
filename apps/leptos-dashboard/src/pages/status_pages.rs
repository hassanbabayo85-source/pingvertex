use crate::api::{monitoring, status_pages as api};
use crate::components::confirm_dialog::ConfirmDialog;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;

#[component]
pub fn StatusPagesPage() -> impl IntoView {
    let pages = RwSignal::new(Vec::<api::StatusPage>::new());
    let monitors = RwSignal::new(Vec::<monitoring::Monitor>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let selected = RwSignal::new(None::<String>);
    let pending_delete = RwSignal::new(None::<String>);
    let navigate = use_navigate();

    let reload = move || {
        spawn_local(async move {
            let p = api::list().await;
            let m = api::monitors().await;
            match (p, m) {
                (Ok(p), Ok(m)) => {
                    pages.set(p);
                    monitors.set(m);
                    error.set(None);
                }
                (Err(e), _) | (_, Err(e)) => error.set(Some(e.to_string())),
            }
            loading.set(false);
        });
    };
    reload();

    view! {
        <div class="feature-page">
            <header class="page-header"><h1>"Status pages"</h1></header>
            {move || notice.get().map(|v| view!{<div class="success">{v}</div>})}
            {move || error.get().map(|v| view!{<div class="error">{v}</div>})}
            {move || {
                let nav = navigate.clone();
                if loading.get() {
                    view!{<div class="loading">"Loading status pages…"</div>}.into_any()
                } else {
                    view! {
                        <div class="feature-grid">
                            <section class="card"><h2>"Create status page"</h2><StatusPageForm monitors=monitors on_created=Callback::new(move |p| { pages.update(|v| v.push(p)); notice.set(Some("Status page created.".into())); }) /></section>
                            <section class="card"><h2>"Your status pages"</h2>
                                {move || pages.get().into_iter().map(|p| {
                                    let id=p.id.clone();
                                    let id2=id.clone();
                                    let _id3=id.clone();
                                    let _nav = nav.clone();
                                    view!{
                                        <article class="list-row">
                                            <div>
                                                <strong>{p.name.clone()}</strong>
                                                <div class="muted">{"/status/"}{p.subdomain.clone()}</div>
                                                <div class="muted">{format!("{} monitor(s)", p.monitor_ids.len())}</div>
                                            </div>
                                            <div class="row-actions">
                                                <button class="btn" on:click=move |_| {selected.set(Some(id.clone()));}>"Manage"</button>
                                                <button class="btn btn-danger" on:click=move |_| pending_delete.set(Some(id2.clone()))>"Delete"</button>
                                            </div>
                                        </article>
                                    }
                                }).collect_view()}
                                {move || if pages.get().is_empty() { view!{<p class="muted">"No status pages yet."</p>}.into_any() } else { ().into_any() }}
                            </section>
                        </div>
                        {move || selected.get().and_then(|id| pages.get().into_iter().find(|p: &api::StatusPage| p.id == id)).map(|p| view!{<StatusPageEditor page=p monitors=monitors on_saved=Callback::new(move |p: api::StatusPage| { pages.update(|v| { if let Some(old)=v.iter_mut().find(|x| x.id==p.id) {*old=p;} }); notice.set(Some("Status page updated.".into())); }) on_close=Callback::new(move |_| selected.set(None)) />})}
                    }.into_any()
                }
            }}
            {move || pending_delete.get().map(|id| {
                let id2=id.clone();
                view!{
                    <ConfirmDialog
                        title="Delete status page"
                        body="This permanently removes the status page."
                        on_confirm=Callback::new(move |_| {
                            let id=id2.clone();
                            spawn_local(async move {
                                match api::delete(&id).await {
                                    Ok(())=>{
                                        pages.update(|v|v.retain(|p|p.id!=id));
                                        notice.set(Some("Status page deleted.".into()));
                                    },
                                    Err(e)=>error.set(Some(e.to_string()))
                                }
                                pending_delete.set(None);
                            });
                        })
                        on_cancel=Callback::new(move |_| pending_delete.set(None))
                    />
                }
            })}
        </div>
    }
}

#[component]
fn StatusPageForm(
    monitors: RwSignal<Vec<monitoring::Monitor>>,
    on_created: Callback<api::StatusPage>,
) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let subdomain = RwSignal::new(String::new());
    let selected = RwSignal::new(Vec::<String>::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let req = api::CreateStatusPageRequest {
            name: name.get(),
            subdomain: subdomain.get(),
            monitor_ids: selected.get(),
        };
        busy.set(true);
        spawn_local(async move {
            match api::create(&req).await {
                Ok(p) => {
                    on_created.run(p);
                    name.set(String::new());
                    subdomain.set(String::new());
                    selected.set(Vec::new());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    view! {<form on:submit=submit class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Subdomain"<input placeholder="status.example" prop:value=subdomain on:input=move|e|subdomain.set(event_target_value(&e)) required/></label><fieldset><legend>"Monitors"</legend>{move||monitors.get().into_iter().map(|m|{let id=m.id.clone();view!{<label class="check-row"><input type="checkbox" on:change=move|e|{let checked=event_target::<web_sys::HtmlInputElement>(&e).checked();selected.update(|v|if checked{if !v.contains(&id){v.push(id.clone())}}else{v.retain(|x|x!=&id)});}/>{m.name}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get() type="submit">"Create"</button>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</form>}
}

#[component]
fn StatusPageEditor(
    page: api::StatusPage,
    monitors: RwSignal<Vec<monitoring::Monitor>>,
    on_saved: Callback<api::StatusPage>,
    on_close: Callback<()>,
) -> impl IntoView {
    let domain_initial = page.custom_domain.clone();
    let domain_verified = page.custom_domain_verified;
    let name = RwSignal::new(page.name.clone());
    let color = RwSignal::new(page.primary_color.clone().unwrap_or_default());
    let selected = RwSignal::new(page.monitor_ids.clone());
    let domain = RwSignal::new(domain_initial.clone().unwrap_or_default());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = page.id.clone();
    let logo_url = page.logo_url.clone();
    let save_callback = on_saved;
    let domain_callback = on_saved;
    let remove_callback = on_saved;
    let upload_callback = on_saved;

    let save = {
        let id = id.clone();
        move |ev: leptos::ev::SubmitEvent| {
            ev.prevent_default();
            let id = id.clone();
            let req = api::UpdateStatusPageRequest {
                name: name.get(),
                monitor_ids: selected.get(),
                logo_url: logo_url.clone(),
                primary_color: if color.get().trim().is_empty() {
                    None
                } else {
                    Some(color.get())
                },
            };
            busy.set(true);
            spawn_local(async move {
                match api::update(&id, &req).await {
                    Ok(p) => save_callback.run(p),
                    Err(e) => error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    };

    let request_domain = {
        let id2 = id.clone();
        move || {
            let d = domain.get();
            if d.trim().is_empty() {
                return;
            }
            let id2 = id2.clone();
            busy.set(true);
            spawn_local(async move {
                match api::request_custom_domain(&id2, &d).await {
                    Ok(p) => domain_callback.run(p),
                    Err(e) => error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    };

    let remove_domain = {
        let id3 = id.clone();
        move || {
            let id3 = id3.clone();
            busy.set(true);
            spawn_local(async move {
                match api::remove_custom_domain(&id3).await {
                    Ok(p) => remove_callback.run(p),
                    Err(e) => error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    };

    let upload_id = page.id.clone();
    let upload = move |ev: leptos::ev::Event| {
        let input = event_target::<HtmlInputElement>(&ev);
        if let Some(file) = input.files().and_then(|f| f.get(0)) {
            let file_name = file.name();
            let mime = file.type_();
            let id = upload_id.clone();
            busy.set(true);
            spawn_local(async move {
                match JsFuture::from(file.array_buffer()).await {
                    Ok(buf) => {
                        let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                        match api::upload_logo(&id, bytes, &file_name, &mime).await {
                            Ok(p) => upload_callback.run(p),
                            Err(e) => error.set(Some(e.to_string())),
                        }
                    }
                    Err(_) => error.set(Some("Could not read the selected image.".into())),
                }
                busy.set(false);
            });
        }
    };

    view! {<section class="card feature-editor"><header class="page-header"><h2>"Manage status page"</h2><button class="btn" on:click=move |_|on_close.run(())>"Close"</button></header><form on:submit=save class="stack-form"><label>"Name"<input prop:value=name on:input=move|e|name.set(event_target_value(&e)) required/></label><label>"Primary color"<input type="text" placeholder="#111827" prop:value=color on:input=move|e|color.set(event_target_value(&e))/></label><fieldset><legend>"Monitors"</legend>{move||monitors.get().into_iter().map(|m|{let mid=m.id.clone();let checked=selected.get().contains(&mid);view!{<label class="check-row"><input type="checkbox" prop:checked=checked on:change=move|e|{let c=event_target::<HtmlInputElement>(&e).checked();selected.update(|v|if c{if !v.contains(&mid){v.push(mid.clone())}}else{v.retain(|x|x!=&mid)});}/>{m.name}</label>}}).collect_view()}</fieldset><button class="btn btn-primary" disabled=move||busy.get() type="submit">"Save"</button></form><div class="subsection"><h3>"Logo"</h3><input type="file" accept="image/*" on:change=upload/><p class="muted">"The server validates the image and enforces its upload limit."</p></div><div class="subsection"><h3>"Custom domain"</h3><p>{move||if domain_verified{"Verified".to_string()}else if page.custom_domain.is_some(){"Pending verification".to_string()}else{"Not configured".to_string()}}</p><div class="inline-form"><input placeholder="status.example.com" prop:value=domain on:input=move|e|domain.set(event_target_value(&e))/><button class="btn" type="button" disabled=move||busy.get() on:click=move|_|request_domain()>"Request"</button><button class="btn btn-danger" type="button" disabled=move||busy.get() on:click=move|_|remove_domain()>"Remove"</button></div></div>{move||error.get().map(|e|view!{<div class="form-error">{e}</div>})}</section>}
}
