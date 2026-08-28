use axum::{
    Router,
    routing::{get, post},
};

use crate::{
    handlers::support::{
        create_support_ticket, get_support_ticket, list_my_support_tickets,
        reply_to_support_ticket, update_support_ticket_status,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/support/tickets",
            post(create_support_ticket).get(list_my_support_tickets),
        )
        .route("/support/tickets/{id}", get(get_support_ticket))
        .route(
            "/support/tickets/{id}/messages",
            post(reply_to_support_ticket),
        )
        .route(
            "/support/tickets/{id}/status",
            axum::routing::patch(update_support_ticket_status),
        )
}
