//! LAN web API + app for browsing your rawr library.
//!
//! A JSON API (axum) plus an embedded single-page app, so the library can
//! be browsed — and EPUBs fetched — from an iPad without the desktop. The
//! whole library's metadata ships to the client as one dictionary-encoded
//! index (`dto::LibraryIndex`); the client filters in memory and works
//! offline from a cached copy.

#[cfg(ui_built)]
pub mod assets;
pub mod dto;
pub mod error;
pub mod handlers;
pub mod index;
pub mod routes;
pub mod state;
