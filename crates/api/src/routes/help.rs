use axum::{Router, routing::get};

use crate::{
    handlers::help_center::{
        get_help_article, list_help_articles, list_help_categories, search_help_articles,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/help/categories", get(list_help_categories))
        .route("/help/articles", get(list_help_articles))
        .route("/help/articles/{slug}", get(get_help_article))
        .route("/help/search", get(search_help_articles))
}
