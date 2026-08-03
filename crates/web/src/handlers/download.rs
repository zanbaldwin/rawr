//! EPUB downloads.
//!
//! Two routes, both ending in `.epub` (WebKit sniffs the extension):
//! `/works/{work_id}/download.epub` is the mutable best-version pointer;
//! `/versions/{cid}/download.epub` is content-addressed and immutable —
//! the one the SPA links to. Safari probes with `Range: bytes=0-1` and
//! then fetches the remainder, so renders land in a single-entry cache
//! keyed `cid-RENDERER_REV` and single ranges get an honest 206.

use crate::error::{ErrorKind, WebError};
use crate::state::AppState;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use exn::{OptionExt, ResultExt};
use rawr_extract::Extractor;
use rawr_extract::models::Version;
use rawr_render::epub::EpubInput;
use rawr_storage::file::{FileInfo, Processed};
use std::sync::Arc;

type File = FileInfo<Processed>;

/// Renderer revision: bytes at a `cid` change when the renderer does, so
/// the version is part of every validator.
const RENDERER_REV: &str = env!("CARGO_PKG_VERSION");

pub async fn best_epub(
    State(state): State<AppState>,
    Path(work_id): Path<u64>,
    headers: HeaderMap,
) -> std::result::Result<Response, WebError> {
    let (version, files) = state
        .cache
        .get_best_for_work_id(work_id)
        .await
        .or_raise(|| ErrorKind::Cache)?
        .ok_or_raise(|| ErrorKind::NotFound)?;
    let mut response = serve_epub(&state, &version, &files, &headers).await?;
    // Point clients at the stable content-addressed URL.
    response.headers_mut().insert(
        header::CONTENT_LOCATION,
        header::HeaderValue::from_str(&format!("/api/v1/versions/{}/download.epub", version.hash))
            .or_raise(|| ErrorKind::Render)?,
    );
    Ok(response)
}

pub async fn version_epub(
    State(state): State<AppState>,
    Path(cid): Path<String>,
    headers: HeaderMap,
) -> std::result::Result<Response, WebError> {
    let (version, files) = state
        .cache
        .get_by_content_hash(&cid)
        .await
        .or_raise(|| ErrorKind::Cache)?
        .ok_or_raise(|| ErrorKind::NotFound)?;
    let mut response = serve_epub(&state, &version, &files, &headers).await?;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, header::HeaderValue::from_static("private, max-age=31536000, immutable"));
    Ok(response)
}

async fn serve_epub(
    state: &AppState,
    version: &Version,
    files: &[File],
    headers: &HeaderMap,
) -> std::result::Result<Response, WebError> {
    let etag = format!("\"{}-{}\"", &version.hash[..16], RENDERER_REV);
    if if_none_match(headers, &etag) {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag),
                (header::CACHE_CONTROL, "private, no-cache".to_string()),
            ],
        )
            .into_response());
    }

    let bytes = rendered(state, version, files).await?;
    let filename = download_filename(state, version);
    let mut response = match byte_range(headers, &etag, bytes.len()) {
        RangeOutcome::Full => (StatusCode::OK, bytes.clone()).into_response(),
        RangeOutcome::Partial(start, end) => {
            let mut partial = (StatusCode::PARTIAL_CONTENT, bytes.slice(start..=end)).into_response();
            partial.headers_mut().insert(
                header::CONTENT_RANGE,
                header::HeaderValue::from_str(&format!("bytes {start}-{end}/{}", bytes.len()))
                    .or_raise(|| ErrorKind::Render)?,
            );
            partial
        },
        RangeOutcome::Unsatisfiable => {
            let mut unsatisfiable = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            unsatisfiable.headers_mut().insert(
                header::CONTENT_RANGE,
                header::HeaderValue::from_str(&format!("bytes */{}", bytes.len())).or_raise(|| ErrorKind::Render)?,
            );
            return Ok(unsatisfiable);
        },
    };
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/epub+zip"));
    response_headers.insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_str(&content_disposition(&filename)).or_raise(|| ErrorKind::Render)?,
    );
    response_headers.insert(header::ETAG, header::HeaderValue::from_str(&etag).or_raise(|| ErrorKind::Render)?);
    response_headers.insert(header::ACCEPT_RANGES, header::HeaderValue::from_static("bytes"));
    response_headers.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("private, no-cache"));
    Ok(response)
}

/// Render (or fetch from the single-entry cache) the EPUB for a version.
async fn rendered(state: &AppState, version: &Version, files: &[File]) -> std::result::Result<Bytes, WebError> {
    let cache_key = format!("{}-{}", version.hash, RENDERER_REV);
    if let Some((key, bytes)) = state.epub_cache.read().await.as_ref()
        && *key == cache_key
    {
        return Ok(bytes.clone());
    }

    // First file that still exists on storage — mirrors the CLI export.
    let mut selected = None;
    for file in files {
        if state.library.exists(&file.path).await.is_ok_and(|exists| exists) {
            selected = Some(file);
            break;
        }
    }
    let file = selected.ok_or_raise(|| ErrorKind::NotFound)?;

    let compressed = state.library.read(&file.path).await.or_raise(|| ErrorKind::Storage)?;
    let html = file.compression.decompress(&compressed).or_raise(|| ErrorKind::Storage)?;
    let metadata = version.metadata.clone();
    let renderer = Arc::clone(&state.epub);
    // Full-document parse + zip assembly: CPU-bound, off the async threads.
    let bytes: Bytes = tokio::task::spawn_blocking(move || -> crate::error::Result<Vec<u8>> {
        let chapters = Extractor::from_html(&html).chapters_xhtml();
        let mut buffer = Vec::new();
        renderer.write(&EpubInput { metadata, chapters }, &mut buffer).or_raise(|| ErrorKind::Render)?;
        Ok(buffer)
    })
    .await
    .or_raise(|| ErrorKind::Render)??
    .into();

    *state.epub_cache.write().await = Some((cache_key, bytes.clone()));
    Ok(bytes)
}

fn if_none_match(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|candidate| candidate.trim() == etag))
}

enum RangeOutcome {
    Full,
    /// Inclusive byte positions.
    Partial(usize, usize),
    Unsatisfiable,
}

/// Single ranges get an honest 206; multi-range and malformed fall back
/// to the full body. `If-Range` must match the strong ETag or the range
/// is ignored.
fn byte_range(headers: &HeaderMap, etag: &str, len: usize) -> RangeOutcome {
    let Some(range) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
        return RangeOutcome::Full;
    };
    if let Some(if_range) = headers.get(header::IF_RANGE).and_then(|v| v.to_str().ok())
        && if_range.trim() != etag
    {
        return RangeOutcome::Full;
    }
    let Some(spec) = range.strip_prefix("bytes=") else {
        return RangeOutcome::Full;
    };
    if spec.contains(',') {
        return RangeOutcome::Full;
    }
    let Some((start_raw, end_raw)) = spec.split_once('-') else {
        return RangeOutcome::Full;
    };
    match (start_raw.trim(), end_raw.trim()) {
        // bytes=a-b and bytes=a-
        (start, end) if !start.is_empty() => {
            let Ok(start) = start.parse::<usize>() else {
                return RangeOutcome::Full;
            };
            if start >= len {
                return RangeOutcome::Unsatisfiable;
            }
            let end = if end.is_empty() {
                len - 1
            } else {
                match end.parse::<usize>() {
                    Ok(end) => end.min(len - 1),
                    Err(_) => return RangeOutcome::Full,
                }
            };
            if end < start {
                return RangeOutcome::Full;
            }
            RangeOutcome::Partial(start, end)
        },
        // bytes=-n (final n bytes)
        ("", suffix) => match suffix.parse::<usize>() {
            Ok(0) => RangeOutcome::Unsatisfiable,
            Ok(n) => RangeOutcome::Partial(len.saturating_sub(n), len - 1),
            Err(_) => RangeOutcome::Full,
        },
        _ => RangeOutcome::Full,
    }
}

/// Download filename from the user's export template (final path
/// segment), falling back to `{work_id}.epub`.
fn download_filename(state: &AppState, version: &Version) -> String {
    state
        .export_paths
        .generate(version, "epub", None)
        .ok()
        .and_then(|path| path.to_string().rsplit('/').next().map(str::to_string))
        .unwrap_or_else(|| format!("{}.epub", version.metadata.work_id))
}

/// RFC 5987 dual form: ASCII fallback first, UTF-8 second. The template
/// slug is usually ASCII already, but never trust a user template.
fn content_disposition(filename: &str) -> String {
    let capped = cap_chars(filename, 100);
    let ascii: String =
        capped.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '_' }).collect();
    let encoded: String = capped
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"._-".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

fn cap_chars(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                header::HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn range_parsing() {
        let etag = "\"abc-1\"";
        assert!(matches!(byte_range(&headers(&[]), etag, 100), RangeOutcome::Full));
        assert!(matches!(byte_range(&headers(&[("range", "bytes=0-1")]), etag, 100), RangeOutcome::Partial(0, 1)));
        assert!(matches!(byte_range(&headers(&[("range", "bytes=10-")]), etag, 100), RangeOutcome::Partial(10, 99)));
        assert!(matches!(byte_range(&headers(&[("range", "bytes=-5")]), etag, 100), RangeOutcome::Partial(95, 99)));
        assert!(matches!(byte_range(&headers(&[("range", "bytes=0-999")]), etag, 100), RangeOutcome::Partial(0, 99)));
        assert!(matches!(byte_range(&headers(&[("range", "bytes=200-")]), etag, 100), RangeOutcome::Unsatisfiable));
        // Multi-range and junk fall back to the full body.
        assert!(matches!(byte_range(&headers(&[("range", "bytes=0-1,5-9")]), etag, 100), RangeOutcome::Full));
        assert!(matches!(byte_range(&headers(&[("range", "lines=0-1")]), etag, 100), RangeOutcome::Full));
        // If-Range mismatch disables the range.
        let mismatched = headers(&[("range", "bytes=0-1"), ("if-range", "\"other\"")]);
        assert!(matches!(byte_range(&mismatched, etag, 100), RangeOutcome::Full));
        let matched = headers(&[("range", "bytes=0-1"), ("if-range", "\"abc-1\"")]);
        assert!(matches!(byte_range(&matched, etag, 100), RangeOutcome::Partial(0, 1)));
    }

    #[test]
    fn content_disposition_dual_form() {
        let value = content_disposition("fandom-tea☕.epub");
        assert!(value.starts_with("attachment; filename=\"fandom-tea_.epub\";"));
        assert!(value.contains("filename*=UTF-8''fandom-tea%E2%98%95.epub"));
        // Header value must be ASCII.
        assert!(value.is_ascii());
    }

    #[test]
    fn caps_at_char_boundary() {
        let long = "é".repeat(80); // 160 bytes
        let capped = cap_chars(&long, 100);
        assert!(capped.len() <= 100);
        assert!(capped.chars().all(|c| c == 'é'));
    }
}
