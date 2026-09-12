use crate::api::support;
use leptos::prelude::*;
use leptos::task::spawn_local;
#[component]
pub fn HelpCenterPage() -> impl IntoView {
    let categories = RwSignal::new(Vec::<support::HelpCategory>::new());
    let articles = RwSignal::new(Vec::<support::HelpArticle>::new());
    let selected = RwSignal::new(None::<support::HelpArticle>);
    let q = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        spawn_local(async move {
            match support::categories().await {
                Ok(v) => categories.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        })
    });
    let search = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let query = q.get();
        spawn_local(async move {
            match support::search(&query).await {
                Ok(v) => articles.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    let load_cat = move |slug: String| {
        spawn_local(async move {
            match support::articles(Some(&slug), None).await {
                Ok(v) => articles.set(v),
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };
    view! {<div class="feature-page"><header class="page-header"><h1>"Help center"</h1></header>{move||error.get().map(|e|view!{<div class="error">{e}</div>})}<form on:submit=search class="inline-form"><input placeholder="Search help" prop:value=q on:input=move|e|q.set(event_target_value(&e))/><button class="btn">"Search"</button></form><section class="feature-grid"><div class="card"><h2>"Categories"</h2>{move||categories.get().into_iter().map(|c|{let slug=c.slug.clone();view!{<button class="link-button" on:click=move |_|load_cat(slug.clone())>{c.name}</button>}}).collect_view()}</div><div class="card"><h2>"Articles"</h2>{move||articles.get().into_iter().map(|a|{let slug=a.slug.clone();view!{<button class="list-row link-button" on:click=move |_|{let slug=slug.clone();spawn_local(async move{match support::article(&slug).await{Ok(v)=>selected.set(Some(v)),Err(e)=>error.set(Some(e.to_string()))}})}>{a.title}</button>}}).collect_view()}</div></section>{move||selected.get().map(|a|view!{<article class="card article"><h2>{a.title}</h2><div class="article-content">{a.content}</div></article>})}</div>}
}
