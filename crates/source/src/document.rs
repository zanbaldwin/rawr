/// A source-neutral book: metadata, ordered sections, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub meta: DocumentMeta,
    pub sections: Vec<Section>,
    pub origin: Origin,
}

impl Document {
    /// BLAKE3 hash (hex) of the input the document was made from, when the source records one.
    pub fn source_hash(&self) -> Option<&str> {
        match &self.origin {
            #[cfg(feature = "ao3")]
            Origin::Ao3(version) => Some(&version.hash),
            Origin::Generic => None,
        }
    }
}

/// The metadata every source can provide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentMeta {
    /// Stable, globally unique identifier; suitable for `dc:identifier`.
    pub identifier: String,
    pub title: String,
    /// Display names, in source order. May be empty (anonymous works).
    pub authors: Vec<String>,
    /// ISO 639 / BCP 47 language code, when known.
    pub language: Option<String>,
    /// Plain-text summary.
    pub summary: Option<String>,
    pub rating: Rating,
}

/// The audience a document is rated for.
///
/// Providers decide which ratings need explicit approval before the text is sent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Rating {
    General,
    Teen,
    Mature,
    Explicit,
    /// Not rated, or the source does not know the rating.
    #[default]
    Unrated,
}

/// One chapter-like unit of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// 1-indexed, in document order.
    pub number: u32,
    pub title: Option<String>,
    /// XHTML fragment.
    pub summary: Option<String>,
    /// XHTML fragment of the section's main text.
    pub body_xhtml: String,
    /// XHTML fragment shown before the body.
    pub notes: Option<String>,
    /// XHTML fragment shown after the body.
    pub end_notes: Option<String>,
}

/// Where a document came from, with any source-specific data.
///
/// The narration pipeline never matches on this; only the edges do (rating
/// gate, path templates, rich title page).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Origin {
    /// An AO3 HTML download, with the full extracted version.
    #[cfg(feature = "ao3")]
    Ao3(Box<rawr_extract::models::Version>),
    /// A source with no metadata beyond [`DocumentMeta`].
    Generic,
}
