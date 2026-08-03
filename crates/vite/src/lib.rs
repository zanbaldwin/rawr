// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed model of Vite's build `manifest.json`, resolving a named entry to the
//! URLs a page must load. Original implementation of the algorithm Vite documents
//! for backend integrations (`importedChunks`, <https://vite.dev/guide/backend-integration>):
//! the entry's stylesheets come first, then the stylesheets of its statically
//! imported chunks (depth-first, dependencies before importers), then the entry's
//! module script, with the imported chunks' files offered as `modulepreload`
//! hints.
//!
//! This exists purely because I didn't want to commit `htmx.min.js` to the
//! repository. Years and years of conditioning not to commit third-party
//! dependencies to Git and now I'm using goddamn Vite like a front-ender.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fmt::Write as _;
use std::path::Path;
use std::str::FromStr;

/// One manifest chunk: a built (content-hashed) file plus what it pulls in. Only
/// the fields the resolution algorithm needs are modelled; `dynamicImports` are
/// deliberately not followed — the browser fetches those on demand.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Chunk {
    /// The built file, relative to the output dir (e.g. `app.aCsAB4Ax.js`).
    file: String,
    /// The chunk's logical name (the Vite `input` key for entries);
    /// absent for anonymous chunks.
    #[serde(default)]
    name: Option<String>,
    /// Whether this chunk is a build entry (addressable by name).
    #[serde(default)]
    is_entry: bool,
    /// Stylesheets extracted from this chunk.
    #[serde(default)]
    css: Vec<String>,
    /// Other build outputs this chunk references (fonts, images, …).
    #[serde(default)]
    assets: Vec<String>,
    /// Manifest keys of the chunks this one statically imports.
    #[serde(default)]
    imports: Vec<String>,
}

/// The URLs one entry needs, in render order, borrowed from the [`Manifest`].
pub struct Entry<'a> {
    /// The entry's own module script; `None` when the entry is itself a stylesheet
    /// (a CSS `input`), whose built file then leads `stylesheets`.
    pub script: Option<Url<'a>>,
    /// Stylesheets: the entry's own, then those of its static import graph.
    pub stylesheets: Vec<Url<'a>>,
    /// The static import graph's module files, for `<link rel="modulepreload">`.
    pub preloads: Vec<Url<'a>>,
}
impl Entry<'_> {
    /// This entry's HTML, in the order Vite's backend-integration guide prescribes:
    /// `<link rel="stylesheet">` tags, then the module `<script>` (a stylesheet
    /// entry has none), then `<link rel="modulepreload">` hints. Every URL is
    /// already mounted under the manifest's prefix.
    pub fn to_html(&self) -> String {
        let mut html = String::new();
        // Writing to a String is infallible.
        for css in &self.stylesheets {
            _ = writeln!(html, r#"<link rel="stylesheet" href="{css}">"#);
        }
        // TODO: Something something async attribute for stuff like workers and
        //       analytics?
        if let Some(script) = &self.script {
            _ = writeln!(html, r#"<script type="module" src="{script}"></script>"#);
        }
        for preload in &self.preloads {
            _ = writeln!(html, r#"<link rel="modulepreload" href="{preload}">"#);
        }
        html
    }
}

/// A root-absolute URL for one built file: the manifest's mount prefix joined to
/// the file's path at write time — a borrowed pair, so rendering allocates nothing.
#[derive(Debug)]
pub struct Url<'a> {
    /// The mount prefix; empty when root-mounted.
    prefix: &'a str,
    /// The built file's path within the output dir (eg, `app.CasAB4Ax.js`).
    path: &'a str,
}
impl fmt::Display for Url<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = self.prefix.trim_matches('/');
        // The empty-prefix arm keeps the URL `/path`, never the scheme-relative `//path`.
        if prefix.is_empty() {
            write!(f, "/{}", self.path.trim_start_matches('/'))
        } else {
            write!(f, "/{}/{}", prefix, self.path.trim_start_matches('/'))
        }
    }
}
#[cfg(test)]
impl PartialEq<&str> for Url<'_> {
    #[expect(clippy::cmp_owned, reason = "test-only; comparing the rendered form is the point")]
    fn eq(&self, other: &&str) -> bool {
        self.to_string() == *other
    }
}

/// The Vite build manifest: source path → built [`Chunk`]. `Default` is the
/// empty manifest, standing in when the assets are unbuilt.
#[derive(Default, Deserialize)]
#[serde(transparent)]
pub struct Manifest {
    /// The URL mount point above the built files.
    /// Set via [`Self::with_prefix`], never by the manifest itself.
    #[serde(skip)]
    prefix: &'static str,
    chunks: HashMap<String, Chunk>,
}
impl Manifest {
    /// Resolve an entry by its `input` name to the URLs it needs, walking the
    /// static import graph depth-first (dependencies before importers, each chunk
    /// once, cycles included). `None` for an unknown name or a chunk that isn't
    /// a build entry.
    pub fn entry(&self, name: &str) -> Option<Entry<'_>> {
        let (key, chunk) =
            self.chunks.iter().find(|(_, chunk)| chunk.is_entry && chunk.name.as_deref() == Some(name))?;
        let mut imported = Vec::new();
        // Seed with the entry itself so an import cycle back to it can't re-list
        // it as a preload.
        self.walk(chunk, &mut HashSet::from([key.as_str()]), &mut imported);
        // A CSS `input` builds to a stylesheet, not a module: its file renders
        // as the leading stylesheet link, and there is no script to load.
        let is_stylesheet =
            Path::new(&chunk.file).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("css"));
        let mut stylesheets: Vec<Url<'_>> = Vec::new();
        if is_stylesheet {
            stylesheets.push(self.url(&chunk.file));
        }
        stylesheets.extend(chunk.css.iter().map(|css| self.url(css)));
        stylesheets.extend(imported.iter().flat_map(|import| &import.css).map(|css| self.url(css)));
        let preloads = imported.iter().map(|import| self.url(&import.file)).collect();
        Some(Entry {
            script: (!is_stylesheet).then(|| self.url(&chunk.file)),
            stylesheets,
            preloads,
        })
    }

    /// Collect `chunk`'s static import graph into `out`, post-order, visiting
    /// each key once.
    fn walk<'a>(&'a self, chunk: &'a Chunk, seen: &mut HashSet<&'a str>, out: &mut Vec<&'a Chunk>) {
        for import in &chunk.imports {
            if !seen.insert(import) {
                continue;
            }
            if let Some(imported) = self.chunks.get(import) {
                self.walk(imported, seen, out);
                out.push(imported);
            }
        }
    }

    /// Whether `path` is a build output named anywhere in the manifest (a chunk's
    /// `file`, `css`, or `assets`) — i.e. content-hashed and safe to cache forever.
    pub fn contains(&self, path: &str) -> bool {
        self.chunks.values().any(|chunk| {
            chunk.file == path
                || chunk.css.iter().any(|css| css == path)
                || chunk.assets.iter().any(|asset| asset == path)
        })
    }

    /// Mount the built files under the URL prefix they're served from (eg,
    /// `"/static"`); an empty prefix means root-mounted.
    #[must_use]
    pub fn with_prefix(mut self, prefix: &'static str) -> Self {
        self.prefix = prefix.trim_matches('/');
        self
    }

    /// `path` as a root-absolute [`Url`] under this manifest's mount prefix.
    fn url<'a>(&'a self, path: &'a str) -> Url<'a> {
        Url { prefix: self.prefix, path }
    }
}
impl TryFrom<serde_json::Value> for Manifest {
    type Error = serde_json::Error;
    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {
        serde_json::from_value(value)
    }
}
impl TryFrom<&[u8]> for Manifest {
    type Error = serde_json::Error;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        serde_json::from_slice(value)
    }
}
impl FromStr for Manifest {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::Manifest;

    /// The example manifest from Vite's backend-integration guide: two entries
    /// sharing a chunk (which carries its own CSS), plus a dynamic entry that
    /// must not be followed.
    const DOCS_MANIFEST: &str = r#"{
        "_shared-B7PI925R.js": {
            "file": "assets/shared-B7PI925R.js",
            "name": "shared",
            "css": ["assets/shared-ChJ_j-JJ.css"]
        },
        "_shared-ChJ_j-JJ.css": {
            "file": "assets/shared-ChJ_j-JJ.css",
            "src": "_shared-ChJ_j-JJ.css"
        },
        "baz.js": {
            "file": "assets/baz-B2H3sXNv.js",
            "name": "baz",
            "src": "baz.js",
            "isDynamicEntry": true
        },
        "views/bar.js": {
            "file": "assets/bar-gkvgaI9m.js",
            "name": "bar",
            "src": "views/bar.js",
            "isEntry": true,
            "imports": ["_shared-B7PI925R.js"],
            "dynamicImports": ["baz.js"]
        },
        "views/foo.js": {
            "file": "assets/foo-BRBmoGS9.js",
            "name": "foo",
            "src": "views/foo.js",
            "isEntry": true,
            "imports": ["_shared-B7PI925R.js"],
            "css": ["assets/foo-5UjPuW-k.css"]
        }
    }"#;

    #[test]
    fn resolves_the_docs_example_exactly_as_the_guide_renders_it() {
        let manifest: Manifest = DOCS_MANIFEST.parse().unwrap();
        let entry = manifest.entry("foo").unwrap();
        assert_eq!(entry.script.unwrap(), "/assets/foo-BRBmoGS9.js");
        // Entry CSS first, then the shared chunk's — the guide's cascade order.
        assert_eq!(entry.stylesheets, ["/assets/foo-5UjPuW-k.css", "/assets/shared-ChJ_j-JJ.css"]);
        assert_eq!(entry.preloads, ["/assets/shared-B7PI925R.js"]);
    }

    #[test]
    fn urls_mount_under_the_normalised_prefix() {
        let manifest = DOCS_MANIFEST.parse::<Manifest>().unwrap().with_prefix("/static/");
        assert_eq!(manifest.entry("foo").unwrap().script.unwrap(), "/static/assets/foo-BRBmoGS9.js");
        // An empty prefix means root-mounted — never the scheme-relative `//…` form.
        let manifest = DOCS_MANIFEST.parse::<Manifest>().unwrap().with_prefix("");
        assert_eq!(manifest.entry("foo").unwrap().script.unwrap(), "/assets/foo-BRBmoGS9.js");
    }

    #[test]
    fn to_html_renders_stylesheets_then_script_then_preloads() {
        let manifest: Manifest = DOCS_MANIFEST.parse().unwrap();
        assert_eq!(
            manifest.entry("foo").unwrap().to_html(),
            "<link rel=\"stylesheet\" href=\"/assets/foo-5UjPuW-k.css\">\n\
             <link rel=\"stylesheet\" href=\"/assets/shared-ChJ_j-JJ.css\">\n\
             <script type=\"module\" src=\"/assets/foo-BRBmoGS9.js\"></script>\n\
             <link rel=\"modulepreload\" href=\"/assets/shared-B7PI925R.js\">\n"
        );
    }

    #[test]
    fn a_stylesheet_entry_renders_a_link_not_a_script() {
        // The shape Vite emits for a CSS `input` named `c` (alongside the JS entry that
        // extracts to the same deduped stylesheet).
        let manifest: Manifest = r#"{
            "assets/app.css": {"file": "c.1.css", "src": "assets/app.css", "isEntry": true, "name": "c", "names": ["c.css"]},
            "assets/app.js": {"file": "app.1.js", "name": "app", "src": "assets/app.js", "isEntry": true, "css": ["c.1.css"]}
        }"#
        .parse()
        .unwrap();
        let entry = manifest.entry("c").unwrap();
        // A stylesheet is not a module: a `<script>` pointing at CSS fails the browser's
        // strict MIME check and loads nothing.
        assert!(entry.script.is_none());
        assert!(entry.preloads.is_empty());
        assert_eq!(entry.stylesheets, ["/c.1.css"]);
        assert_eq!(entry.to_html(), "<link rel=\"stylesheet\" href=\"/c.1.css\">\n");
    }

    #[test]
    fn ignores_dynamic_imports_and_only_resolves_true_entries() {
        let manifest: Manifest = DOCS_MANIFEST.parse().unwrap();
        let entry = manifest.entry("bar").unwrap();
        // `baz` is only dynamically imported: fetched on demand, so neither preloaded nor styled here.
        assert_eq!(entry.preloads, ["/assets/shared-B7PI925R.js"]);
        assert_eq!(entry.stylesheets, ["/assets/shared-ChJ_j-JJ.css"]);
        // `baz` is a dynamic entry, not a build entry — not addressable; nor are unknown names.
        assert!(manifest.entry("baz").is_none());
        assert!(manifest.entry("nope").is_none());
    }

    #[test]
    fn diamond_imports_dedupe_and_cycles_terminate() {
        // app → b, c; b → d, c → d (diamond); d → app (cycle back to the entry).
        let manifest: Manifest = r#"{
            "app.js": {"file": "app.1.js", "name": "app", "isEntry": true, "imports": ["_b.js", "_c.js"]},
            "_b.js": {"file": "b.1.js", "imports": ["_d.js"]},
            "_c.js": {"file": "c.1.js", "imports": ["_d.js"]},
            "_d.js": {"file": "d.1.js", "css": ["d.1.css"], "imports": ["app.js"]}
        }"#
        .parse()
        .unwrap();
        let entry = manifest.entry("app").unwrap();
        // Each chunk once, dependencies before importers, and the entry never preloads itself.
        assert_eq!(entry.preloads, ["/d.1.js", "/b.1.js", "/c.1.js"]);
        assert_eq!(entry.stylesheets, ["/d.1.css"]);
    }

    #[test]
    fn contains_recognises_files_css_and_assets() {
        let manifest: Manifest = r#"{
            "app.js": {"file": "app.1.js", "css": ["app.1.css"], "assets": ["logo.1.svg"]}
        }"#
        .parse()
        .unwrap();
        assert!(manifest.contains("app.1.js"));
        assert!(manifest.contains("app.1.css"));
        assert!(manifest.contains("logo.1.svg"));
        assert!(!manifest.contains("app.2.js"));
    }
}
