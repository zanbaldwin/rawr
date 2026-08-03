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
        .route("/works/{work_id}", get(handlers::works::detail))
        // Explicit fallback: without it, unmatched paths inside the nest
        // fall through to the OUTER router's fallback and skip the JSON
        // guard below.
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
        .layer(axum::middleware::map_response(json_error_guard));
    let router = Router::new().nest("/api/v1", api);
    // One fallback covers `/`, `/assets/*`, icons, and SPA deep links.
    #[cfg(ui_built)]
    let router = router.fallback(crate::assets::serve);
    #[cfg(not(ui_built))]
    let router = router.fallback(|| async {
        "rawr-web: API only — frontend not built. Run `make assets`, or `npm run dev` on :5173."
    });
    router.with_state(state)
}
