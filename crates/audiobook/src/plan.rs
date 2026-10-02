//! What narrating a document involves: scripts, batch keys, and which
//! batches are already recorded. Planning pays for nothing.

use crate::error::{ErrorKind, Result};
use crate::script::{Script, ScriptOptions};
use crate::store::{BatchKey, KeyInputs, Ledger, LedgerSection, RecordingStatus, Store};
use exn::ResultExt;
use rawr_render::pages::chapter_heading;
use rawr_source::Document;
use rawr_tts::{Fingerprint, Synthesiser};
use time::OffsetDateTime;

/// The narration plan of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// [`DocumentMeta::identifier`](rawr_source::DocumentMeta::identifier).
    pub identifier: String,
    pub title: String,
    /// Hash of the source the plan was made from, for the ledger. The caller sets it when known.
    pub source_hash: Option<String>,
    pub provider: String,
    pub fingerprint: Fingerprint,
    pub language: Option<String>,
    pub sections: Vec<SectionPlan>,
    /// Sections with nothing to speak.
    pub skipped: Vec<u32>,
}

/// One section: its script and a planned batch for each script batch, in the same order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionPlan {
    pub script: Script,
    pub batches: Vec<PlannedBatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedBatch {
    pub key: BatchKey,
    /// The prepared text, exactly as it will be sent.
    pub text: String,
    pub chars: usize,
    pub state: BatchState,
}

/// Whether a batch still has to be paid for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BatchState {
    Recorded,
    Missing,
    /// Recorded, but the alignment did not match. Paid for again only on explicit request.
    Mismatched,
}

impl Plan {
    /// Plan a document for a synthesiser, against the recordings in a store.
    ///
    /// # Errors
    ///
    /// - [`Provider`](ErrorKind::Provider) if the synthesiser has no voice for
    ///   the document's language (checked before anything else)
    /// - [`Storage`](ErrorKind::Storage) or [`CorruptRecording`](ErrorKind::CorruptRecording)
    ///   if the store cannot be read
    pub async fn new(document: &Document, synthesiser: &dyn Synthesiser, store: &Store) -> Result<Self> {
        let language = document.meta.language.clone();
        let fingerprint = synthesiser.fingerprint(language.as_deref()).or_raise(|| ErrorKind::Provider)?;
        let options = ScriptOptions {
            max_batch_chars: synthesiser.capabilities().max_batch_chars,
        };
        let mut sections = Vec::new();
        let mut skipped = Vec::new();
        let multi_chapter = document.sections.len() > 1;
        for section in &document.sections {
            let heading = chapter_heading(section.number, section.title.as_deref(), multi_chapter);
            let script = match Script::new(section, heading.as_deref(), &options) {
                Ok(script) => script,
                Err(error) if matches!(*error, ErrorKind::EmptySection(_)) => {
                    skipped.push(section.number);
                    continue;
                },
                Err(error) => return Err(error),
            };
            let mut batches = Vec::with_capacity(script.batches.len());
            for batch in &script.batches {
                let text = synthesiser.prepare(&batch.text);
                let key = BatchKey::new(&KeyInputs {
                    provider: synthesiser.name(),
                    fingerprint: &fingerprint,
                    language: language.as_deref(),
                    seed: script.seed,
                    text: &text,
                });
                let state = match store.meta(&key).await? {
                    None => BatchState::Missing,
                    Some(meta) if meta.status == RecordingStatus::Ok => BatchState::Recorded,
                    Some(_) => BatchState::Mismatched,
                };
                batches.push(PlannedBatch {
                    key,
                    chars: text.chars().count(),
                    text,
                    state,
                });
            }
            sections.push(SectionPlan { script, batches });
        }
        Ok(Self {
            identifier: document.meta.identifier.clone(),
            title: document.meta.title.clone(),
            source_hash: None,
            provider: synthesiser.name().to_string(),
            fingerprint,
            language,
            sections,
            skipped,
        })
    }

    /// Every planned batch, in section order.
    pub fn batches(&self) -> impl Iterator<Item = &PlannedBatch> {
        self.sections.iter().flat_map(|section| &section.batches)
    }

    /// Batches in a state.
    pub fn count(&self, state: BatchState) -> usize {
        self.batches().filter(|batch| batch.state == state).count()
    }

    /// Characters still to be paid for (missing batches only).
    pub fn missing_chars(&self) -> usize {
        self.batches().filter(|batch| batch.state == BatchState::Missing).map(|batch| batch.chars).sum()
    }

    /// The key inputs of a planned batch in a section.
    pub fn key_inputs<'a>(&'a self, section: &'a SectionPlan, batch: &'a PlannedBatch) -> KeyInputs<'a> {
        KeyInputs {
            provider: &self.provider,
            fingerprint: &self.fingerprint,
            language: self.language.as_deref(),
            seed: section.script.seed,
            text: &batch.text,
        }
    }

    /// The ledger for this plan.
    pub fn ledger(&self, planned_at: OffsetDateTime) -> Ledger {
        Ledger {
            identifier: self.identifier.clone(),
            title: self.title.clone(),
            source_hash: self.source_hash.clone(),
            planned_at,
            voice_name: None,
            sections: self
                .sections
                .iter()
                .map(|section| LedgerSection {
                    number: section.script.section,
                    seed: section.script.seed,
                    keys: section.batches.iter().map(|batch| batch.key.to_string()).collect(),
                })
                .collect(),
        }
    }
}
