//! `/api/v1/reads` — the read log.
//!
//! The log is a small CSV file in the reads target. It is loaded fresh on
//! every request, so edits made outside the server are always seen.
//! Writes hold [`reads_lock`](crate::state::Inner::reads_lock) across load,
//! change and save. The library index is not involved, so recording a read
//! never rebuilds it.

use crate::dto::{ReadEventDto, ReadLogDto};
use crate::error::{ErrorKind, WebError};
use crate::index::CACHE_CONTROL_REVALIDATE;
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use exn::ResultExt;
use rawr_reads::ReadLog;

async fn load(state: &AppState) -> crate::error::Result<ReadLog> {
    ReadLog::load(&state.reads, &state.reads_path).await.or_raise(|| ErrorKind::Reads)
}

async fn save(state: &AppState, log: &ReadLog) -> crate::error::Result<()> {
    log.save(&state.reads, &state.reads_path).await.or_raise(|| ErrorKind::Reads)
}

/// `GET` — the whole log, with an ETag so an unchanged log costs a 304.
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> std::result::Result<Response, WebError> {
    let log = load(&state).await?;
    let body = serde_json::to_vec(&ReadLogDto::build(&log)?).or_raise(|| ErrorKind::Reads)?;
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

/// `POST` — append one read event.
pub async fn record(
    State(state): State<AppState>,
    Json(event): Json<ReadEventDto>,
) -> std::result::Result<(StatusCode, Json<ReadEventDto>), WebError> {
    let entry = event.to_entry()?;
    let _guard = state.reads_lock.lock().await;
    let mut log = load(&state).await?;
    log.push(entry);
    save(&state, &log).await?;
    Ok((StatusCode::CREATED, Json(event)))
}

/// `DELETE` — remove the last event equal to the one given (undo).
pub async fn undo(
    State(state): State<AppState>,
    Json(event): Json<ReadEventDto>,
) -> std::result::Result<StatusCode, WebError> {
    let entry = event.to_entry()?;
    let _guard = state.reads_lock.lock().await;
    let mut log = load(&state).await?;
    if !log.remove_last(&entry) {
        return Err(exn::Exn::new(ErrorKind::ReadNotFound).into());
    }
    save(&state, &log).await?;
    Ok(StatusCode::NO_CONTENT)
}
