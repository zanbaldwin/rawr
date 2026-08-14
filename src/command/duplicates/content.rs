use super::File;
use super::cluster::Cluster;
use crate::error::Result;
use crate::output::{Line, Loudness, Output, PALETTE, Piece};
use rawr_storage::BackendHandle;
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;

/// The rendered result of diffing two versions' content.
pub(crate) struct DiffResult {
    /// Git-style unified diff lines (hunks with context), ready to print.
    pub lines: Vec<Line<'static>>,
    /// Similarity ratio in `0.0..=1.0`.
    pub ratio: f32,
    /// Whether the two inputs are byte-identical (after any normalisation).
    pub identical: bool,
    pub additions: usize,
    pub deletions: usize,
}

/// Read a version's first file, decompress it, and return the text to diff:
/// the raw decompressed HTML, or — when `normalize` — the canonicalised prose.
pub(crate) async fn version_diff_text(backend: &BackendHandle, file: &File, normalize: bool) -> Result<String> {
    let compressed = backend.read(&file.path).await?;
    let bytes = file.compression.decompress(&compressed)?;
    let html = String::from_utf8_lossy(&bytes);
    Ok(if normalize { normalize_prose(&html) } else { html.into_owned() })
}

/// Compute a Git-style line diff between two texts.
pub(crate) fn compute_diff(keep: &str, other: &str) -> DiffResult {
    let diff = TextDiff::from_lines(keep, other);
    let ratio = diff.ratio();
    let identical = keep == other;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let (mut additions, mut deletions) = (0usize, 0usize);
    for (group_index, group) in diff.grouped_ops(3).iter().enumerate() {
        if group_index > 0 {
            lines.push(Line::new([Piece::fixed("\u{22ee}", &PALETTE.muted)]).with_volume(Loudness::Shout));
        }
        for op in group {
            for change in diff.iter_changes(op) {
                let (sign, style) = match change.tag() {
                    ChangeTag::Delete => {
                        deletions += 1;
                        ("-", &PALETTE.removed)
                    },
                    ChangeTag::Insert => {
                        additions += 1;
                        ("+", &PALETTE.added)
                    },
                    ChangeTag::Equal => (" ", &PALETTE.muted),
                };
                let text = change.value().trim_end_matches(['\n', '\r']).to_string();
                lines.push(
                    Line::new([Piece::fixed(sign, style), Piece::space(), Piece::flex(text, style, 0)])
                        .with_volume(Loudness::Shout),
                );
            }
        }
    }
    DiffResult { lines, ratio, identical, additions, deletions }
}

/// Populate `identical`/`similarity` on every cluster member by reading and
/// normalising each version's content and comparing it to the keep-pick.
pub(crate) async fn normalize_pass(
    clusters: &mut [Cluster],
    backends: &HashMap<String, BackendHandle>,
    output: &dyn Output,
) {
    let total: u64 = clusters.iter().map(|c| c.candidates.len() as u64).sum();
    let bar = output.progress_bar("Normalising");
    bar.set_length(total);
    for cluster in clusters.iter_mut() {
        let keep = read_member_text(&cluster.candidates[0], backends, true).await;
        bar.inc(1);
        for candidate in cluster.candidates.iter_mut().skip(1) {
            let other = read_member_text(candidate, backends, true).await;
            bar.inc(1);
            if let (Ok(keep), Ok(other)) = (&keep, &other) {
                candidate.identical = keep == other;
                candidate.similarity = Some(TextDiff::from_lines(keep, other).ratio());
            }
        }
    }
    bar.finish_and_clear();
}

/// Read the text of a cluster member via its first file in a resolvable target.
pub(crate) async fn read_member_text(
    candidate: &super::cluster::Candidate,
    backends: &HashMap<String, BackendHandle>,
    normalize: bool,
) -> Result<String> {
    let (file, backend) = candidate
        .files
        .iter()
        .find_map(|file| backends.get(&file.target).map(|backend| (file, backend)))
        .ok_or_else(|| miette::miette!("version has no file in a target defined in the config"))?;
    version_diff_text(backend, file, normalize).await
}

/// Reduce an AO3 HTML download to canonical prose: chapter text only (excluding
/// the download header/footer), markup stripped, whitespace collapsed. Two
/// downloads that differ only in markup/whitespace reduce to identical text.
fn normalize_prose(html: &str) -> String {
    let extractor = rawr_extract::Extractor::from_html(html.as_bytes());
    let mut raw = String::new();
    for chapter in extractor.chapters_xhtml() {
        if let Some(title) = &chapter.title {
            raw.push_str(title);
            raw.push('\n');
        }
        if let Some(summary) = &chapter.summary {
            raw.push_str(summary);
            raw.push('\n');
        }
        raw.push_str(&chapter.body_html);
        raw.push('\n');
        if let Some(notes) = &chapter.author_notes {
            raw.push_str(notes);
            raw.push('\n');
        }
        if let Some(notes) = &chapter.end_notes {
            raw.push_str(notes);
            raw.push('\n');
        }
    }
    let text = strip_tags(&raw);
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove HTML tags, turning block-level boundaries into newlines so the result
/// diffs paragraph-by-paragraph.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after = &rest[lt + 1..];
        let Some(gt) = after.find('>') else {
            out.push_str(&rest[lt..]);
            return out;
        };
        if is_block_tag(&after[..gt]) {
            out.push('\n');
        }
        rest = &after[gt + 1..];
    }
    out.push_str(rest);
    out
}

fn is_block_tag(tag: &str) -> bool {
    let name: String =
        tag.trim_start_matches('/').chars().take_while(char::is_ascii_alphanumeric).collect::<String>().to_lowercase();
    matches!(
        name.as_str(),
        "p" | "br"
            | "div"
            | "li"
            | "ul"
            | "ol"
            | "blockquote"
            | "hr"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "tr"
            | "table"
            | "section"
            | "article"
            | "pre"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_diff_lines() {
        let result = compute_diff("a\nb\nc\n", "a\nb\nc\n");
        assert!(result.identical);
        assert!(result.lines.is_empty());
        assert_eq!(result.ratio, 1.0);
    }

    #[test]
    fn changed_text_reports_additions_and_deletions() {
        let result = compute_diff("a\nb\nc\n", "a\nB\nc\n");
        assert!(!result.identical);
        assert_eq!(result.deletions, 1);
        assert_eq!(result.additions, 1);
    }

    #[test]
    fn normalize_collapses_markup_and_whitespace() {
        let a = "<div id=chapters><div class=userstuff><p class=a id=x>Hello   world.</p></div></div>";
        let b = "<div id=chapters><div class=userstuff><p id=x class=a>Hello world.</p>\n</div></div>";
        // Different bytes, but identical prose after normalisation.
        assert_ne!(a, b);
        assert_eq!(normalize_prose(a), normalize_prose(b));
    }

    #[test]
    fn strip_tags_breaks_on_block_boundaries() {
        assert_eq!(strip_tags("<p>one</p><p>two</p>"), "\none\n\ntwo\n");
        assert_eq!(strip_tags("a<br/>b"), "a\nb");
    }
}
