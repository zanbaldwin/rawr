use super::File;
use crate::error::Result;
use rawr_cache::Repository;
use rawr_extract::models::Version;
use std::cmp::Ordering;
use std::collections::HashMap;

/// What the user intends to do with a version in a cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Keep,
    Delete,
}

/// A single member of a near-duplicate cluster: one version plus every file
/// that references it (possibly across multiple targets).
pub(crate) struct Candidate {
    pub version: Version,
    pub files: Vec<File>,
    pub mark: Mark,
    /// Normalised content equals the cluster's keep-pick (only set by the
    /// `--normalize` pass).
    pub identical: bool,
    /// Similarity ratio (0.0–1.0) versus the keep-pick (only set by the
    /// `--normalize` pass).
    pub similarity: Option<f32>,
}
impl Candidate {
    fn new(version: Version, files: Vec<File>) -> Self {
        Self { version, files, mark: Mark::Keep, identical: false, similarity: None }
    }
}

/// A group of two or more semantically-similar versions of the same work that
/// differ only in content hash (i.e. markup/whitespace, not metadata).
pub(crate) struct Cluster {
    pub work_id: u64,
    pub title: String,
    /// Sorted best-first; index 0 is the recommended version to keep.
    pub candidates: Vec<Candidate>,
    pub expanded: bool,
}
impl Cluster {
    /// Number of members currently marked for deletion.
    pub(crate) fn marked(&self) -> usize {
        self.candidates.iter().filter(|c| c.mark == Mark::Delete).count()
    }

    /// Decompressed byte length of the recommended keep-pick.
    pub(crate) fn keep_length(&self) -> u64 {
        self.candidates.first().map(|c| c.version.length).unwrap_or(0)
    }
}

/// Build near-duplicate clusters for one storage target (or a single work).
///
/// Clustering uses only cached metadata (no file I/O): two versions of the same
/// work cluster when they share an identical chapter count and their word counts
/// are within `word_tolerance` percent of each other. A large content reduction
/// (deletion notice) never clusters, since equal chapters already excludes it.
pub(crate) async fn build_clusters(
    cache: &Repository,
    work: Option<u64>,
    target: &str,
    word_tolerance: f64,
) -> Result<Vec<Cluster>> {
    let work_ids = match work {
        Some(id) => vec![id],
        None => cache.find_works_with_multiple_versions().await?.into_iter().map(|(id, _)| id).collect(),
    };

    let mut clusters = Vec::new();
    for work_id in work_ids {
        let mut versions = cache.get_by_work_id(work_id).await?;
        versions.retain(|(_, files)| files.iter().any(|f| f.target == target));
        if versions.len() < 2 {
            continue;
        }
        for component in partition(versions, word_tolerance) {
            if component.len() < 2 {
                continue;
            }
            let title = component[0].version.metadata.title.clone();
            clusters.push(Cluster { work_id, title, candidates: component, expanded: false });
        }
    }

    // Most-duplicated works first, then by work id for stable ordering.
    clusters.sort_by(|a, b| b.candidates.len().cmp(&a.candidates.len()).then(a.work_id.cmp(&b.work_id)));
    Ok(clusters)
}

/// Partition a work's versions into connected components of near-duplicates,
/// each sorted best-first.
fn partition(versions: Vec<(Version, Vec<File>)>, word_tolerance: f64) -> Vec<Vec<Candidate>> {
    let n = versions.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            if are_near_duplicates(&versions[i].0, &versions[j].0, word_tolerance) {
                union(&mut parent, i, j);
            }
        }
    }

    let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    let mut groups: HashMap<usize, Vec<Candidate>> = HashMap::new();
    for (root, (version, files)) in roots.into_iter().zip(versions) {
        groups.entry(root).or_default().push(Candidate::new(version, files));
    }

    groups
        .into_values()
        .map(|mut candidates| {
            candidates.sort_by(|a, b| b.version.partial_cmp(&a.version).unwrap_or(Ordering::Less));
            candidates
        })
        .collect()
}

/// Two versions of the same work are near-duplicates when they share an
/// identical chapter count, their word counts are within tolerance, and they
/// are not byte-identical (which would make them the same version).
fn are_near_duplicates(a: &Version, b: &Version, word_tolerance: f64) -> bool {
    a.hash != b.hash
        && a.metadata.chapters.written == b.metadata.chapters.written
        && a.metadata.chapters.total == b.metadata.chapters.total
        && words_within(a.metadata.words, b.metadata.words, word_tolerance)
}

fn words_within(a: u64, b: u64, tolerance_pct: f64) -> bool {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let delta = (hi - lo) as f64;
    delta <= (hi as f64) * (tolerance_pct / 100.0)
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let a = find(parent, a);
    let b = find(parent, b);
    if a != b {
        parent[a] = b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_within_zero_delta() {
        assert!(words_within(1000, 1000, 0.0));
    }

    #[test]
    fn words_within_tolerance_boundary() {
        // 1% of 1000 = 10
        assert!(words_within(1000, 990, 1.0));
        assert!(!words_within(1000, 989, 1.0));
    }

    #[test]
    fn words_within_symmetric() {
        assert_eq!(words_within(990, 1000, 1.0), words_within(1000, 990, 1.0));
    }
}
