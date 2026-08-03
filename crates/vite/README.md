# vite-chunks

A typed model of Vite's build `manifest.json` for [backend integration](https://vite.dev/guide/backend-integration):
resolve a named entry to everything a page must load, in the order Vite's guide prescribes, and render it as HTML tags.

No I/O, no framework coupling: hand it manifest bytes, it hands back URLs or HTML.

## Parsing

`Manifest` deserialises from whatever form you have the manifest in:

```rust
use vite_chunks::Manifest;

let manifest: Manifest = json_str.parse()?;     // FromStr
let manifest = Manifest::try_from(bytes)?;      // TryFrom<&[u8]>
let manifest = Manifest::try_from(json_value)?; // TryFrom<serde_json::Value>

// (Optional) Path from document root to manifest directory.
let manifest = manifest.with_prefix("/assets");
```

`Manifest::default()` is the empty manifest — useful as a stand-in when assets are unbuilt, since every lookup just
returns `None`.

## Resolving an entry

```rust
let entry = manifest.entry("app").expect("`app` is a build entry");

for css in &entry.stylesheets { println!("<link rel='stylesheet' href='{css}'>"); }
if let Some(script) = &entry.script {
    // None when the entry is itself a stylesheet.
    println!("<script type='module' src='{script}'></script>");
};
for js in &entry.preloads { println!("<link rel='modulepreload' href='{js}'>"); }

// Or let it render the tags for you, in the guide's order:
let html = entry.to_html();
```

## Real-world usage

### Parse

```rust
/// The Vite build manifest, baked in at compile time.
pub(crate) static MANIFEST: LazyLock<Manifest> = LazyLock::new(|| {
    let bytes = include_bytes!("../../static/manifest.json");
    Manifest::try_from(bytes.as_slice()).expect("asset manifest to be valid").with_prefix("/static")
});
```

### Serve

```rust
/// The embedded `crates/app/static/` tree (built assets — gitignored, produced
/// by `vite build`).
#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

/// Axum: GET `/static/{*path}`.
pub(crate) async fn serve_asset(Path(path): Path<String>) -> Response {
    let Some(file) = Assets::get(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = HeaderValue::from_str(file.metadata.mimetype()).unwrap_or_else(
        |_| HeaderValue::from_static("application/octet-stream")
    );
    let mut response = ([(header::CONTENT_TYPE, mime)], file.data).into_response();
    if MANIFEST.contains(&path) {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable")
        );
    }
    response
}
```

### Template

```rust
mod filters {
    /// Askama: Render the `<link>`/`<script>` tags for a Vite entry.
    ///
    /// # Example
    /// ```html
    /// {{ "app"|vite }}
    /// ```
    #[askama::filter_fn]
    pub fn vite(value: impl AsRef<str>, _: &dyn Values) -> askama::Result<Safe<String>> {
        let name = value.as_ref();
        let html = ASSET_MANIFEST
            .entry(name)
            .map(|entry| entry.to_html())
            .or_else(|| { tracing::warn!(%name, "unknown Vite entry"); None })
            .unwrap_or_default();
        Ok(Safe(html))
    }
}
```

### Re-compile

```rust
// build.rs
const MANIFEST_PATH: &str = "static/manifest.json";

println!("cargo:rerun-if-changed={MANIFEST_PATH}");
let json = std::fs::read_to_string(MANIFEST_PATH).expect("run the asset build first");
_ = json.parse::<vite_chunks::Manifest>().expect("asset manifest to be valid");
```

## Licence

MIT OR Apache-2.0.
