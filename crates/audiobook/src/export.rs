//! Building the book from stored recordings. Export never pays.

use crate::audio::Codec;
use crate::epub3::{Chapter, Package, SyncLevel, narrator_credit};
use crate::error::{ErrorKind, Result};
use crate::plan::Plan;
use crate::store::Store;
use crate::timeline::assemble;
use rawr_render::StyleConfig;
use rawr_source::Document;
use time::OffsetDateTime;

/// How to build the book.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportOptions {
    /// What is highlighted while the audio plays.
    pub sync: SyncLevel,
    /// The book's `dc:identifier`, when it should differ from the document's
    /// (for example a second edition of the same narration). The ledger is
    /// still found by the plan's identifier.
    pub identifier: Option<String>,
    /// Narrate only these sections ([`Section::number`](rawr_source::Section::number));
    /// the others are text only. `None` narrates every planned section.
    pub sections: Option<Vec<u32>>,
}

/// [`export_with_options`] with the default [`ExportOptions`].
///
/// # Errors
///
/// As [`export_with_options`].
pub async fn export<'a>(
    document: &'a Document,
    plan: &'a Plan,
    store: &Store,
    codec: &dyn Codec,
    styles: &'a StyleConfig,
) -> Result<Package<'a>> {
    export_with_options(document, plan, store, codec, styles, &ExportOptions::default()).await
}

/// Join and encode every planned section from the store, and return the
/// book, ready to [`write`](Package::write).
///
/// The plan must be made from `document`. Only one chapter's PCM is held at
/// a time. Decoding and encoding are CPU-bound and run on the caller's task.
/// The narrator credit uses the voice names in the ledger, when there are
/// any: "{voice}, {voice} as {role}, …".
///
/// # Errors
///
/// - [`MissingRecording`](ErrorKind::MissingRecording) if a batch has no usable recording
/// - [`MissingAlignment`](ErrorKind::MissingAlignment) if a recording has no alignment
/// - [`Storage`](ErrorKind::Storage), [`CorruptRecording`](ErrorKind::CorruptRecording) or
///   [`CorruptLedger`](ErrorKind::CorruptLedger) if the store cannot be read
/// - codec errors from decoding and encoding
pub async fn export_with_options<'a>(
    document: &'a Document,
    plan: &'a Plan,
    store: &Store,
    codec: &dyn Codec,
    styles: &'a StyleConfig,
    options: &ExportOptions,
) -> Result<Package<'a>> {
    let mut chapters = Vec::with_capacity(plan.sections.len());
    let included = |section: &&crate::plan::SectionPlan| {
        options.sections.as_ref().is_none_or(|sections| sections.contains(&section.script.section))
    };
    for section in plan.sections.iter().filter(included) {
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
        chapters.push(Chapter {
            section,
            units: audio.units,
            words: audio.words,
            encoded,
        });
    }
    let ledger = store.ledger(&plan.identifier).await?;
    let mut voice_name = ledger.as_ref().and_then(|ledger| ledger.voice_name.clone());
    if let Some(ledger) = ledger.filter(|ledger| !ledger.voices.is_empty()) {
        let cast = ledger.voices.iter().map(|(role, name)| format!("{name} as {role}"));
        voice_name = Some(voice_name.into_iter().chain(cast).collect::<Vec<_>>().join(", "));
    }
    Ok(Package {
        document,
        chapters,
        styles,
        narrator: Some(narrator_credit(&plan.provider, &plan.fingerprint, voice_name.as_deref())),
        modified: OffsetDateTime::now_utc(),
        sync: options.sync,
        identifier: options.identifier.clone(),
    })
}
