//! Building the book from stored recordings. Export never pays.

use crate::audio::Codec;
use crate::epub3::{Chapter, Package, narrator_credit};
use crate::error::{ErrorKind, Result};
use crate::plan::Plan;
use crate::store::Store;
use crate::timeline::assemble;
use rawr_render::StyleConfig;
use rawr_source::Document;
use time::OffsetDateTime;

/// Join and encode every planned section from the store, and return the
/// book, ready to [`write`](Package::write).
///
/// The plan must be made from `document`. Only one chapter's PCM is held at
/// a time. Decoding and encoding are CPU-bound and run on the caller's task.
/// The narrator credit uses the voice name in the ledger, when there is one.
///
/// # Errors
///
/// - [`MissingRecording`](ErrorKind::MissingRecording) if a batch has no usable recording
/// - [`MissingAlignment`](ErrorKind::MissingAlignment) if a recording has no alignment
/// - [`Storage`](ErrorKind::Storage), [`CorruptRecording`](ErrorKind::CorruptRecording) or
///   [`CorruptLedger`](ErrorKind::CorruptLedger) if the store cannot be read
/// - codec errors from decoding and encoding
pub async fn export<'a>(
    document: &'a Document,
    plan: &'a Plan,
    store: &Store,
    codec: &dyn Codec,
    styles: &'a StyleConfig,
) -> Result<Package<'a>> {
    let mut chapters = Vec::with_capacity(plan.sections.len());
    for section in &plan.sections {
        let mut recordings = Vec::with_capacity(section.batches.len());
        for batch in &section.batches {
            let recording = store
                .read(&batch.key)
                .await?
                .ok_or_else(|| exn::Exn::new(ErrorKind::MissingRecording(batch.key.to_string())))?;
            recordings.push(recording);
        }
        let audio = assemble(section, &recordings, codec)?;
        drop(recordings);
        let encoded = codec.encode(&audio.pcm)?;
        chapters.push(Chapter { section, units: audio.units, encoded });
    }
    let voice_name = store.ledger(&plan.identifier).await?.and_then(|ledger| ledger.voice_name);
    Ok(Package {
        document,
        chapters,
        styles,
        narrator: Some(narrator_credit(&plan.provider, &plan.fingerprint, voice_name.as_deref())),
        modified: OffsetDateTime::now_utc(),
    })
}
