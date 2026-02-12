mod compare;
mod consts;
pub mod error;
mod extract;
pub mod models;
mod truncate;

pub use crate::extract::{extract, is_valid};
pub use crate::truncate::{ESTIMATED_HEADER_SIZE_BYTES, safe_html_truncate};
