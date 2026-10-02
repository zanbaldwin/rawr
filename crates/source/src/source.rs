use crate::document::Document;
use crate::error::Result;

/// Anything that turns input bytes into a [`Document`].
///
/// Input is the complete, decompressed file. Reading from storage and
/// decompressing are the caller's job. Implementations are synchronous and
/// CPU-bound; async callers should run them in `spawn_blocking`.
///
/// The trait is object-safe, so callers can hold several sources as
/// `Box<dyn Source>` and use the first one that [`accepts`](Self::accepts)
/// the input.
pub trait Source {
    /// Cheap check (sniffing, no full parse) that this source understands the input.
    fn accepts(&self, input: &[u8]) -> bool;

    /// Parse the whole input into a document.
    ///
    /// # Errors
    ///
    /// - [`Unsupported`](crate::error::ErrorKind::Unsupported) if the input is not for this source
    /// - [`Extract`](crate::error::ErrorKind::Extract) if the input is malformed
    /// - [`NoSections`](crate::error::ErrorKind::NoSections) if there is nothing to narrate
    fn to_document(&self, input: &[u8]) -> Result<Document>;
}
