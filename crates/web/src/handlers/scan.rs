//! `POST /api/v1/scan` — rescan the library into the cache.
//!
//! Exists because works can be imported from another machine: the CLI on
//! the desktop writes files into the shared library, and this server's
//! cache only learns about them when something walks the storage again.
//!
//! Single-flight: POST starts a background scan (202) or reports the one
//! already running (200); GET polls the same status. A scan that changed
//! anything bumps the index generation, so the next sync rebuilds.

use crate::dto::ScanStatusDto;
use crate::error::WebError;
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use futures::StreamExt;
use rawr_library::scan::{ScanEffort, ScanEvent, scan};
use rawr_storage::BackendHandle;
use rawr_storage::backend::HtmlOnlyBackend;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use time::UtcDateTime;

/// Lock-free scan bookkeeping; a status snapshot is just atomic reads.
#[derive(Default)]
pub struct ScanTracker {
    running: AtomicBool,
    discovered: AtomicU64,
    processed: AtomicU64,
    changed: AtomicU64,
    errors: AtomicU64,
    /// 0 = never.
    started_at: AtomicI64,
    /// 0 = running or never.
    finished_at: AtomicI64,
}

impl ScanTracker {
    /// Claim the single flight; false when a scan is already running.
    fn try_start(&self) -> bool {
        if self.running.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return false;
        }
        self.discovered.store(0, Ordering::Relaxed);
        self.processed.store(0, Ordering::Relaxed);
        self.changed.store(0, Ordering::Relaxed);
        self.errors.store(0, Ordering::Relaxed);
        self.started_at.store(UtcDateTime::now().unix_timestamp(), Ordering::Relaxed);
        self.finished_at.store(0, Ordering::Relaxed);
        true
    }

    fn finish(&self) {
        self.finished_at.store(UtcDateTime::now().unix_timestamp(), Ordering::Relaxed);
        self.running.store(false, Ordering::Release);
    }

    pub fn status(&self) -> ScanStatusDto {
        let stamp = |value: i64| (value != 0).then_some(value);
        ScanStatusDto {
            running: self.running.load(Ordering::Acquire),
            discovered: u32::try_from(self.discovered.load(Ordering::Relaxed)).unwrap_or(u32::MAX),
            processed: u32::try_from(self.processed.load(Ordering::Relaxed)).unwrap_or(u32::MAX),
            changed: u32::try_from(self.changed.load(Ordering::Relaxed)).unwrap_or(u32::MAX),
            errors: u32::try_from(self.errors.load(Ordering::Relaxed)).unwrap_or(u32::MAX),
            started_at: stamp(self.started_at.load(Ordering::Relaxed)),
            finished_at: stamp(self.finished_at.load(Ordering::Relaxed)),
        }
    }
}

pub async fn status(State(state): State<AppState>) -> Json<ScanStatusDto> {
    Json(state.scan.status())
}

pub async fn trigger(
    State(state): State<AppState>,
) -> std::result::Result<(StatusCode, Json<ScanStatusDto>), WebError> {
    if !state.scan.try_start() {
        return Ok((StatusCode::OK, Json(state.scan.status())));
    }
    let task_state = state.clone();
    tokio::spawn(async move {
        run_scan(&task_state).await;
    });
    Ok((StatusCode::ACCEPTED, Json(state.scan.status())))
}

async fn run_scan(state: &AppState) {
    // Same wrap as the CLI's scan command: only HTML(+compression) paths.
    let backend: BackendHandle = Arc::new(HtmlOnlyBackend::new(Arc::clone(&state.library)));
    let tracker = &state.scan;
    match scan(&backend, &state.cache, None::<&str>) {
        Ok(stream) => {
            let mut stream = pin!(stream);
            while let Some(event) = stream.next().await {
                match event {
                    Ok(ScanEvent::Started) | Ok(ScanEvent::DiscoveryComplete(_)) | Ok(ScanEvent::Complete) => {},
                    Ok(ScanEvent::FileDiscovered(_)) => {
                        tracker.discovered.fetch_add(1, Ordering::Relaxed);
                    },
                    Ok(ScanEvent::Scanned(scanned)) => {
                        tracker.processed.fetch_add(1, Ordering::Relaxed);
                        match scanned.as_ref().effort {
                            ScanEffort::Cached => {},
                            ScanEffort::Processed | ScanEffort::Recalculated => {
                                tracker.changed.fetch_add(1, Ordering::Relaxed);
                            },
                        }
                    },
                    // House pattern: individual file failures never abort
                    // the batch.
                    Err(error) => {
                        tracker.processed.fetch_add(1, Ordering::Relaxed);
                        tracker.errors.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!(error = %error, "scan: file failed");
                    },
                }
            }
        },
        Err(error) => {
            tracker.errors.fetch_add(1, Ordering::Relaxed);
            tracing::error!(error = %error, "scan could not start");
        },
    }
    let status = tracker.status();
    tracker.finish();
    tracing::info!(processed = status.processed, changed = status.changed, errors = status.errors, "scan finished");
    if status.changed > 0 && !state.dry_run {
        // New metadata in the cache: the next index request must rebuild.
        state.index.bump_generation();
    }
}
