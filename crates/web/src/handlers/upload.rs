//! `POST /api/v1/uploads` — multipart HTML import.
//!
//! The client sends one file per request (XHR is the only upload-progress
//! path on iPad), but the handler happily takes several fields. House
//! pattern throughout: individual file failures never fail the request —
//! always 200 with per-file outcomes once the request itself parsed.

use crate::dto::{UploadOutcome, UploadResponse, UploadResult, UploadSummary, UploadedWork};
use crate::error::{ErrorKind, WebError};
use crate::state::AppState;
use axum::Json;
use axum::extract::{Multipart, State};
use futures::TryStreamExt;
use rawr_compress::Compression;
use rawr_extract::models::Version;
use rawr_library::import::{Import, import_file};
use rawr_storage::file::{FileInfo, Processed};
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_util::io::StreamReader;

pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> std::result::Result<Json<UploadResponse>, WebError> {
    let mut results = Vec::new();
    let mut summary = UploadSummary::default();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => return Err(exn::Exn::new(ErrorKind::Upload(error.to_string())).into()),
        };
        // Plain form values (no filename) are not uploads — skip them.
        let Some(filename) = field.file_name().map(str::to_string) else {
            continue;
        };
        // The extension is a *compression* hint only — iOS pickers mangle
        // names; extraction decides validity.
        let compression = Compression::from_path(&filename);
        let reader = StreamReader::new(field.map_err(std::io::Error::other)).compat();
        let outcome = match import_file(&state.library, &state.cache, &state.library_ctx, compression, reader).await {
            Ok(import) => {
                let outcome = to_outcome(import);
                match &outcome {
                    UploadOutcome::Added { .. } => summary.added += 1,
                    UploadOutcome::Upgraded { .. } => summary.upgraded += 1,
                    UploadOutcome::Unchanged { .. } => summary.unchanged += 1,
                    UploadOutcome::Outdated { .. } => summary.outdated += 1,
                    UploadOutcome::Failed { .. } => summary.failed += 1,
                }
                outcome
            },
            Err(error) => {
                tracing::warn!(file = %filename, error = %error, "import failed");
                summary.failed += 1;
                // The deepest frame is the actual cause; intermediate
                // frames are routing labels.
                let mut chain = Vec::new();
                collect_chain(error.frame(), &mut chain);
                UploadOutcome::Failed {
                    message: chain.last().cloned().unwrap_or_else(|| error.to_string()),
                    kind: "import".to_string(),
                }
            },
        };
        results.push(UploadResult { filename, outcome });
    }

    if results.is_empty() {
        return Err(exn::Exn::new(ErrorKind::BadRequest("no files in the upload")).into());
    }

    let changed = summary.added + summary.upgraded + summary.outdated > 0;
    if changed && !state.dry_run {
        // Server-initiated change must never rely on tier-1 aggregates.
        state.index.bump_generation();
    }
    // Informational: the still-current snapshot when nothing changed,
    // empty when the client should re-sync.
    let snapshot =
        if changed { String::new() } else { state.index.peek().await.map(|c| c.snapshot.clone()).unwrap_or_default() };

    Ok(Json(UploadResponse {
        dry_run: state.dry_run,
        snapshot,
        results,
        summary,
    }))
}

fn collect_chain(frame: &exn::Frame, out: &mut Vec<String>) {
    for child in frame.children() {
        out.push(child.error().to_string());
        collect_chain(child, out);
    }
}

fn to_outcome(import: Import) -> UploadOutcome {
    match import {
        Import::NewImport(file, version) => UploadOutcome::Added { work: uploaded(&file, &version) },
        Import::Upgrade(file, version) => UploadOutcome::Upgraded { work: uploaded(&file, &version) },
        Import::AlreadyExists(file, version) => UploadOutcome::Unchanged { work: uploaded(&file, &version) },
        Import::Outdated(file, version) => UploadOutcome::Outdated { work: uploaded(&file, &version) },
    }
}

fn uploaded(file: &FileInfo<Processed>, version: &Version) -> UploadedWork {
    UploadedWork {
        work_id: u32::try_from(version.metadata.work_id).unwrap_or(u32::MAX),
        title: version.metadata.title.clone(),
        hash: format!("{:08x}", version.crc32),
        path: file.path.to_string(),
        size: u32::try_from(file.size).unwrap_or(u32::MAX),
        compression: file.compression.to_string(),
    }
}
