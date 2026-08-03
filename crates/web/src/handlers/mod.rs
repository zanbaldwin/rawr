//! Request handlers.

pub mod download;
pub mod index;
pub mod upload;
pub mod works;

use crate::dto::ApiMeta;
use crate::error::{ErrorKind, WebError};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use exn::ResultExt;
use time::UtcDateTime;

/// Liveness only — no DB touch, for systemd/proxy health checks.
pub async fn health() -> &'static str {
    "{\"ok\":true}"
}

/// Tiny "is the NAS awake, has anything changed" probe.
///
/// Runs the cheap token query plus a work count; the `snapshot` field is
/// filled from the built index only when it is still fresh, so this never
/// triggers a rebuild.
pub async fn meta(State(state): State<AppState>) -> std::result::Result<Json<ApiMeta>, WebError> {
    let token = state.cache.snapshot_token(&state.target).await.or_raise(|| ErrorKind::Cache)?;
    let works = state.cache.count_works().await.or_raise(|| ErrorKind::Cache)?;
    let snapshot = match state.index.peek().await {
        Some(cached) if cached.token == token => cached.snapshot.clone(),
        _ => String::new(),
    };
    Ok(Json(ApiMeta {
        name: "rawr-web",
        version: env!("CARGO_PKG_VERSION"),
        target: state.target.clone(),
        dry_run: state.dry_run,
        snapshot,
        works: u32::try_from(works).unwrap_or(u32::MAX),
        versions: u32::try_from(token.version_count).unwrap_or(u32::MAX),
        generated_at: UtcDateTime::now().unix_timestamp(),
    }))
}
