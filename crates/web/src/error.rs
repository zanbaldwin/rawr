//! Web Server Error Types
//!
//! This module provides structured errors using `exn` for automatic location
//! tracking and error tree construction. See `ERRORS.md` for design rationale.
//!
//! TODO: Definitely going to refactor this later once I've written a few
//!       more crates. Designing errors in Rust is **hard** and I don't want
//!       to resort to anyhow+thiserror just because I don't want to deal with it.

use crate::dto::ApiError;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use derive_more::{Display, Error};

/// A web server error with automatic location tracking.
pub type Error = exn::Exn<ErrorKind>;
/// Result type alias for web server operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Actionable error categories.
///
/// These describe what the caller should *do*, not what went wrong internally.
/// Each maps to an HTTP status; the mapping lives with the response layer.
#[derive(Debug, Display, Error)]
pub enum ErrorKind {
    /// The requested work/version is not in the library.
    #[display("that work is not in your library")]
    NotFound,
    /// The request could not be understood.
    #[display("the request could not be understood: {_0}")]
    BadRequest(#[error(not(source))] &'static str),
    /// The request body is not a type this endpoint accepts.
    #[display("this endpoint does not accept that content type")]
    Unsupported,
    /// The request body exceeds the configured limit.
    #[display("the upload is too large")]
    TooLarge,
    /// An uploaded file could not be read.
    #[display("the upload could not be read: {_0}")]
    Upload(#[error(not(source))] String),
    /// The library index could not be built.
    #[display("the library index could not be built")]
    Index,
    /// The library cache could not be queried.
    #[display("the library cache could not be queried")]
    Cache,
    /// The work's file could not be read from storage.
    #[display("the work's file could not be read from storage")]
    Storage,
    /// The work could not be converted for download.
    #[display("the work could not be converted for download")]
    Render,
}

impl ErrorKind {
    /// Returns `true` if retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Cache | Self::Storage)
    }

    /// HTTP status this kind maps to.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unsupported => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Upload(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Index | Self::Cache | Self::Render => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Storage => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// Stable machine slug for the JSON body. Exhaustive on purpose.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Unsupported => "unsupported",
            Self::TooLarge => "too_large",
            Self::Upload(_) => "upload",
            Self::Index => "index",
            Self::Cache => "cache",
            Self::Storage => "storage",
            Self::Render => "render",
        }
    }
}

/// Newtype so `IntoResponse` can be implemented (orphan rules forbid it
/// on `Exn<ErrorKind>` directly). Handlers return `Result<_, WebError>`
/// and `?` converts through `From<Error>`.
pub struct WebError(pub Error);

impl From<Error> for WebError {
    fn from(error: Error) -> Self {
        Self(error)
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let kind = &*self.0;
        let status = kind.status();
        let mut chain = Vec::new();
        collect_chain(self.0.frame(), &mut chain);
        if status.is_server_error() {
            tracing::error!(status = %status, error = %kind, chain = ?chain, "request failed");
        } else {
            tracing::warn!(status = %status, error = %kind, "request rejected");
        }
        let body = ApiError {
            kind: kind.slug().to_string(),
            status: status.as_u16(),
            message: kind.to_string(),
            chain,
        };
        (status, Json(body)).into_response()
    }
}

/// Flatten an exn frame tree, outermost cause first (preorder; the root
/// frame's own message is already the response `message`).
fn collect_chain(frame: &exn::Frame, out: &mut Vec<String>) {
    for child in frame.children() {
        out.push(child.error().to_string());
        collect_chain(child, out);
    }
}

/// `map_response` guard for the `/api/v1` router: rewrites any non-JSON
/// error response (axum's own 404/405s, `DefaultBodyLimit` 413s) into an
/// [`ApiError`] body, so the SPA's blanket `res.json()` never throws.
pub async fn json_error_guard(response: Response) -> Response {
    let status = response.status();
    if !(status.is_client_error() || status.is_server_error()) {
        return response;
    }
    let is_json = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/json"));
    if is_json {
        return response;
    }
    let (kind, message) = match status {
        StatusCode::NOT_FOUND => ("not_found", "no such API endpoint"),
        StatusCode::METHOD_NOT_ALLOWED => ("bad_request", "wrong method for this endpoint"),
        StatusCode::PAYLOAD_TOO_LARGE => ("too_large", "the upload is too large"),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ("unsupported", "this endpoint does not accept that content type"),
        _ => ("error", "the request failed"),
    };
    let body = ApiError {
        kind: kind.to_string(),
        status: status.as_u16(),
        message: message.to_string(),
        chain: Vec::new(),
    };
    (status, Json(body)).into_response()
}
