//! Version/Metadata Comparison

use crate::models::{Metadata, Version};
use std::cmp::Ordering;

impl Version {
    /// Detect if this version appears to be a deletion notice.
    ///
    /// Authors sometimes replace their fic content with a brief message like
    /// "This work has been removed by the author" rather than deleting the work.
    /// This results in a "newer" version with almost no content.
    ///
    /// Returns true if `other` has significantly more content, suggesting `self`
    /// might be a deletion notice.
    fn appears_to_be_deletion_notice(&self, other: &Self) -> bool {
        // Check if we have drastically fewer chapters (>50% reduction)
        let chapter_threshold = other.metadata.chapters.written as f64 * 0.5;
        let chapters_reduced = (self.metadata.chapters.written as f64) < chapter_threshold;
        // Check if we're much smaller (>80% size reduction)
        let size_threshold = other.length as f64 * 0.2;
        let size_reduced = (self.length as f64) < size_threshold;
        chapters_reduced && size_reduced
    }
}
impl Ord for Version {
    /// Compare two versions to determine which is "newer" or "better".
    ///
    /// This comparison is only meaningful between Versions with the same `work_id`.
    /// The algorithm is used during import/sync with `conflict_policy: overwrite`,
    /// when resolving which Version represents the "current best" for a Work,
    /// and in the `duplicates` command for recommending which version to keep.
    fn cmp(&self, other: &Self) -> Ordering {
        // Step 1: Detect deletion notices
        // If one version has drastically less content, it may be a deletion notice
        if self.appears_to_be_deletion_notice(other) {
            return Ordering::Less; // other is better (self appears to be deletion notice)
        }
        if other.appears_to_be_deletion_notice(self) {
            return Ordering::Greater; // self is better (other appears to be deletion notice)
        }
        // Step 2: Compare metadata
        self.metadata.cmp(&other.metadata)
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Metadata {
    /// Compare two pieces of metadata to determine which is "newer" or "better".
    fn cmp(&self, other: &Self) -> Ordering {
        // Step 1: Compare by last modified date
        if self.last_modified != other.last_modified {
            return self.last_modified.cmp(&other.last_modified);
        }
        // Step 2: Compare by content quantity
        if self.words != other.words {
            return self.words.cmp(&other.words);
        }
        if self.chapters.written != other.chapters.written {
            return self.chapters.written.cmp(&other.chapters.written);
        }
        // Step 3: Compare by publication date
        if self.published != other.published {
            return self.published.cmp(&other.published);
        }
        // Step 4: Truly ambiguous - treat as equal
        Ordering::Equal
    }
}
impl PartialOrd for Metadata {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
