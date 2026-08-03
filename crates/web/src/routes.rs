//! Router assembly.

use crate::error::json_error_guard;
use crate::handlers;
use crate::state::AppState;
use axum::Router;
use axum::routing::get;

pub fn build(state: AppState) -> Router {
    // Every `/api/v1` response is JSON, always — including axum's own
    // 404/405 rejections, which the guard rewrites.
    let api = Router::new()
        .route("/health", get(handlers::health))
        .route("/meta", get(handlers::meta))
        .route("/index", get(handlers::index::index))
        // Explicit fallback: without it, unmatched paths inside the nest
        // fall through to the OUTER router's fallback and skip the JSON
        // guard below.
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
        .layer(axum::middleware::map_response(json_error_guard));
    Router::new()
        .nest("/api/v1", api)
        // Placeholder until the SPA embed lands; becomes the shell +
        // asset routes + SPA fallback.
        .route("/", get(|| async { "rawr-web: API only (frontend not built yet)" }))
        .with_state(state)
}
