//! `GET /api/v1/stats` — storage sizes only.
//!
//! Deliberately narrow: every other statistic derives client-side from
//! the index in milliseconds; only the byte totals live nowhere else.

use crate::dto::StorageStatsDto;
use crate::error::{ErrorKind, WebError};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use exn::ResultExt;

pub async fn stats(State(state): State<AppState>) -> std::result::Result<Json<StorageStatsDto>, WebError> {
    let (content_size, file_size) = state.cache.storage_sizes(&state.target).await.or_raise(|| ErrorKind::Cache)?;
    Ok(Json(StorageStatsDto { content_size, file_size }))
}
