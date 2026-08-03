//! `GET /api/v1/works/{work_id}` — full detail for one work.

use crate::dto::WorkDetail;
use crate::error::{ErrorKind, WebError};
use crate::index::CACHE_CONTROL_REVALIDATE;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use exn::ResultExt;

pub async fn detail(
    State(state): State<AppState>,
    Path(work_id): Path<u64>,
    headers: HeaderMap,
) -> std::result::Result<Response, WebError> {
    let versions = state.cache.get_by_work_id(work_id).await.or_raise(|| ErrorKind::Cache)?;
    if versions.is_empty() {
        return Err(exn::Exn::new(ErrorKind::NotFound).into());
    }
    let detail = WorkDetail::build(work_id, &versions)?;
    let body = serde_json::to_vec(&detail).or_raise(|| ErrorKind::Index)?;
    let etag = format!("\"{}\"", &blake3::hash(&body).to_hex()[..16]);

    let base = [
        (header::ETAG, etag.clone()),
        (header::CACHE_CONTROL, CACHE_CONTROL_REVALIDATE.to_string()),
    ];
    let matches = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|candidate| candidate.trim() == etag));
    if matches {
        return Ok((StatusCode::NOT_MODIFIED, base).into_response());
    }
    let mut response = (StatusCode::OK, base, body).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().expect("static header"));
    Ok(response)
}
