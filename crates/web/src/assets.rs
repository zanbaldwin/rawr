//! Embedded SPA asset serving. Compiled only when the frontend was built
//! (`ui_built` cfg from `build.rs`).
//!
//! One fallback handler covers everything that is not `/api`: hashed
//! assets get `immutable` (gated by `vite_chunks::Manifest::contains`, so
//! an unhashed file can never be cached forever), `public/` files get a
//! day, and everything else — `/`, deep links — falls back to the shell
//! with `no-cache`, **200, never a redirect**.

use axum::http::{Uri, header};
use axum::response::{IntoResponse, Response};
use std::sync::LazyLock;
use vite_chunks::Manifest;

static MANIFEST: LazyLock<Manifest> = LazyLock::new(|| {
    include_str!(concat!(env!("OUT_DIR"), "/vite-manifest.json"))
        .parse()
        .expect("asset manifest validated by build.rs")
});

#[derive(rust_embed::RustEmbed)]
#[folder = "ui/dist/"]
#[exclude = ".vite/*"]
#[exclude = ".gitkeep"]
struct Assets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let Some(file) = Assets::get(path) else {
        // Standard SPA fallback rule: extensionless paths are app routes
        // and get the shell; a dotted final segment is a missing *file*
        // and must 404 (iOS treats an HTML body where it expected a PNG
        // as a broken icon, not a graceful fallback).
        let looks_like_file = path.rsplit('/').next().is_some_and(|segment| segment.contains('.'));
        if looks_like_file {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        }
        return shell();
    };
    let cache_control = if MANIFEST.contains(path) {
        "public, max-age=31536000, immutable"
    } else if path == "sw.js" {
        // The service worker script drives its own update cycle; a cached
        // copy would pin users to old bundles for a day.
        "no-cache"
    } else {
        // Icons, manifest.webmanifest — replaceable, but not hot.
        "public, max-age=86400"
    };
    (
        [
            (header::CONTENT_TYPE, content_type(path)),
            (header::CACHE_CONTROL, cache_control),
        ],
        file.data,
    )
        .into_response()
}

/// The SPA shell: served for `/` and every deep link.
fn shell() -> Response {
    match Assets::get("index.html") {
        Some(file) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            file.data,
        )
            .into_response(),
        // ui_built guarantees the manifest existed at compile time, but a
        // debug build reads dist/ from disk at runtime — stay graceful.
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Frontend build missing its index.html — re-run `make assets`.",
        )
            .into_response(),
    }
}

/// Deliberately a hand-rolled match, not a mime dependency: the embed
/// only ever contains what Vite emits.
fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
