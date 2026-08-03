//! `GET /api/v1/index` — the whole-library sync payload.

use crate::error::WebError;
use crate::index::{CACHE_CONTROL_REVALIDATE, Encoding};
use crate::state::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

pub async fn index(State(state): State<AppState>, headers: HeaderMap) -> std::result::Result<Response, WebError> {
    // `fetch(url, {cache:'reload'})` — the client's manual refresh —
    // sends `Cache-Control: no-cache`; that is the tier-1 bypass.
    let force =
        headers.get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("no-cache"));
    let cached = state.index.current(&state.cache, &state.target, &state.fandoms, force).await?;
    let encoding = Encoding::negotiate(&headers);
    let etag = cached.etag(encoding);

    // 304s must carry ETag, Cache-Control AND Vary.
    let base = [
        (header::ETAG, etag.clone()),
        (header::CACHE_CONTROL, CACHE_CONTROL_REVALIDATE.to_string()),
        (header::VARY, "Accept-Encoding".to_string()),
    ];
    let matches = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|candidate| candidate.trim() == etag));
    if matches {
        return Ok((StatusCode::NOT_MODIFIED, base).into_response());
    }

    let body = cached.body(encoding);
    let mut response = (StatusCode::OK, base, body).into_response();
    let headers_mut = response.headers_mut();
    headers_mut.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().expect("static header"));
    if let Some(value) = encoding.content_encoding() {
        headers_mut.insert(header::CONTENT_ENCODING, value.parse().expect("static header"));
    }
    // The decompressed size, so the client's first-run progress bar has an
    // honest denominator (Content-Length is the compressed length).
    headers_mut.insert("x-index-length", cached.identity.len().to_string().parse().expect("numeric header"));
    Ok(response)
}
