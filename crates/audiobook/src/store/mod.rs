//! Recordings: every paid text-to-speech response, stored by batch key.
//!
//! ```text
//! keys/ab/<key>/audio.opus       provider audio, exactly as received
//! keys/ab/<key>/recording.json   written last: the recording exists only if this exists
//! ledgers/<blake3(identifier)>.json
//! ```
//!
//! Recordings are content-addressed, so resuming needs no state: a batch is
//! paid for exactly when its key is missing.

mod key;
mod ledger;
mod recording;

pub use self::key::{BatchKey, KEY_RECIPE, KeyInputs};
pub use self::ledger::{Ledger, LedgerSection};
pub use self::recording::{
    AlignmentMeta, AudioFormatMeta, AudioMeta, ContinuityMeta, CostMeta, CostUnitMeta, ReceiptMeta, Recording,
    RecordingMeta, RecordingStatus,
};
use crate::error::{ErrorKind, Result};
use exn::{OptionExt, ResultExt};
use rawr_storage::{BackendHandle, ValidPath};
use rawr_tts::Audio;
use std::collections::BTreeSet;

const KEYS: &str = "keys";
const LEDGERS: &str = "ledgers";
const METADATA_FILE: &str = "recording.json";
const METADATA_TEMPORARY: &str = "recording.json.tmp";

/// The recordings store, on any storage backend.
#[derive(Clone)]
pub struct Store {
    backend: BackendHandle,
}

impl Store {
    pub fn new(backend: BackendHandle) -> Self {
        Self { backend }
    }

    /// Whether a complete recording exists for this key.
    pub async fn exists(&self, key: &BatchKey) -> Result<bool> {
        self.backend.exists(&path(&recording_path(key, METADATA_FILE))?).await.or_raise(|| ErrorKind::Storage)
    }

    /// A recording's metadata, if it exists.
    pub async fn meta(&self, key: &BatchKey) -> Result<Option<RecordingMeta>> {
        if !self.exists(key).await? {
            return Ok(None);
        }
        let bytes =
            self.backend.read(&path(&recording_path(key, METADATA_FILE))?).await.or_raise(|| ErrorKind::Storage)?;
        let meta: RecordingMeta =
            serde_json::from_slice(&bytes).or_raise(|| ErrorKind::CorruptRecording(key.to_string()))?;
        if meta.key != key.as_str() {
            exn::bail!(ErrorKind::CorruptRecording(key.to_string()));
        }
        Ok(Some(meta))
    }

    /// A whole recording, with its audio, if it exists.
    pub async fn read(&self, key: &BatchKey) -> Result<Option<Recording>> {
        let Some(meta) = self.meta(key).await? else {
            return Ok(None);
        };
        let bytes =
            self.backend.read(&path(&recording_path(key, &meta.audio.file))?).await.or_raise(|| ErrorKind::Storage)?;
        let audio = Audio { bytes, format: meta.audio_format() };
        Ok(Some(Recording { meta, audio }))
    }

    /// Store a recording, replacing any earlier one with the same key.
    ///
    /// The audio is written first, then the metadata under a temporary name,
    /// which is then renamed into place. A recording is complete only once
    /// its metadata is in place.
    pub async fn write(&self, recording: &Recording) -> Result<()> {
        let key = BatchKey::parse(&recording.meta.key)
            .ok_or_raise(|| ErrorKind::CorruptRecording(recording.meta.key.clone()))?;
        let metadata = serde_json::to_vec_pretty(&recording.meta).or_raise(|| ErrorKind::Storage)?;
        let temporary = path(&recording_path(&key, METADATA_TEMPORARY))?;
        self.backend
            .write(&path(&recording_path(&key, &recording.meta.audio.file))?, &recording.audio.bytes)
            .await
            .or_raise(|| ErrorKind::Storage)?;
        self.backend.write(&temporary, &metadata).await.or_raise(|| ErrorKind::Storage)?;
        self.backend
            .rename(&temporary, &path(&recording_path(&key, METADATA_FILE))?)
            .await
            .or_raise(|| ErrorKind::Storage)
    }

    /// Delete a recording. The metadata goes first, so a partial delete leaves "not recorded".
    pub async fn delete(&self, key: &BatchKey) -> Result<()> {
        let metadata = path(&recording_path(key, METADATA_FILE))?;
        if self.backend.exists(&metadata).await.or_raise(|| ErrorKind::Storage)? {
            self.backend.delete(&metadata).await.or_raise(|| ErrorKind::Storage)?;
        }
        let prefix = path(&recording_dir(key))?;
        for file in self.backend.list(Some(&prefix)).await.or_raise(|| ErrorKind::Storage)? {
            self.backend.delete(&file.meta().path).await.or_raise(|| ErrorKind::Storage)?;
        }
        Ok(())
    }

    /// Every key with anything stored under it, complete or not.
    pub async fn keys(&self) -> Result<BTreeSet<BatchKey>> {
        let files = self.backend.list(Some(&path(KEYS)?)).await.or_raise(|| ErrorKind::Storage)?;
        Ok(files
            .iter()
            .filter_map(|file| file.meta().path.as_ref().split('/').nth(2).and_then(BatchKey::parse))
            .collect())
    }

    /// Write (or replace) a document's ledger.
    pub async fn write_ledger(&self, ledger: &Ledger) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(ledger).or_raise(|| ErrorKind::Storage)?;
        self.backend.write(&path(&ledger_path(&ledger.identifier))?, &bytes).await.or_raise(|| ErrorKind::Storage)
    }

    /// A document's ledger, if it has one.
    pub async fn ledger(&self, identifier: &str) -> Result<Option<Ledger>> {
        let file = path(&ledger_path(identifier))?;
        if !self.backend.exists(&file).await.or_raise(|| ErrorKind::Storage)? {
            return Ok(None);
        }
        let bytes = self.backend.read(&file).await.or_raise(|| ErrorKind::Storage)?;
        let ledger = serde_json::from_slice(&bytes).or_raise(|| ErrorKind::CorruptLedger(identifier.to_string()))?;
        Ok(Some(ledger))
    }

    /// Every ledger in the store.
    pub async fn ledgers(&self) -> Result<Vec<Ledger>> {
        let prefix = path(LEDGERS)?;
        let mut ledgers = Vec::new();
        for file in self.backend.list(Some(&prefix)).await.or_raise(|| ErrorKind::Storage)? {
            let file_path = &file.meta().path;
            let bytes = self.backend.read(file_path).await.or_raise(|| ErrorKind::Storage)?;
            ledgers.push(serde_json::from_slice(&bytes).or_raise(|| ErrorKind::CorruptLedger(file_path.to_string()))?);
        }
        Ok(ledgers)
    }

    /// Delete a document's ledger and the recordings that no other ledger uses.
    ///
    /// Returns the deleted keys.
    pub async fn delete_document(&self, identifier: &str) -> Result<Vec<BatchKey>> {
        let Some(ledger) = self.ledger(identifier).await? else {
            return Ok(Vec::new());
        };
        let others: BTreeSet<BatchKey> = self
            .ledgers()
            .await?
            .iter()
            .filter(|other| other.identifier != identifier)
            .flat_map(|other| other.keys().collect::<Vec<_>>())
            .collect();
        let doomed: BTreeSet<BatchKey> = ledger.keys().filter(|key| !others.contains(key)).collect();
        for key in &doomed {
            self.delete(key).await?;
        }
        self.backend.delete(&path(&ledger_path(identifier))?).await.or_raise(|| ErrorKind::Storage)?;
        Ok(doomed.into_iter().collect())
    }

    /// Keys stored but used by no ledger.
    pub async fn orphans(&self) -> Result<Vec<BatchKey>> {
        let used: BTreeSet<BatchKey> =
            self.ledgers().await?.iter().flat_map(|ledger| ledger.keys().collect::<Vec<_>>()).collect();
        Ok(self.keys().await?.into_iter().filter(|key| !used.contains(key)).collect())
    }

    /// Delete every orphan. Returns the deleted keys.
    pub async fn delete_orphans(&self) -> Result<Vec<BatchKey>> {
        let orphans = self.orphans().await?;
        for key in &orphans {
            self.delete(key).await?;
        }
        Ok(orphans)
    }
}

fn recording_dir(key: &BatchKey) -> String {
    format!("{KEYS}/{}/{key}", &key.as_str()[..2])
}

fn recording_path(key: &BatchKey, file: &str) -> String {
    format!("{}/{file}", recording_dir(key))
}

fn ledger_path(identifier: &str) -> String {
    format!("{LEDGERS}/{}.json", blake3::hash(identifier.as_bytes()).to_hex())
}

fn path(value: &str) -> Result<ValidPath> {
    ValidPath::new(value).or_raise(|| ErrorKind::Storage)
}
