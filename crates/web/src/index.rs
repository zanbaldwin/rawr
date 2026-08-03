//! The in-memory index cache behind `GET /api/v1/index`.
//!
//! Two-tier change detection: tier 1 is [`SnapshotToken`] (five cheap SQL
//! aggregates) plus a generation counter bumped by the upload handler —
//! it only decides whether to *rebuild*. Tier 2 is the ETag,
//! `blake3(identity_body)[..16]`, computed from the serialised payload
//! itself — so the validator can never be stale relative to the body, and
//! a tier-1 false positive costs one rebuild that still 304s.
//!
//! The payload is pre-compressed once per snapshot (gzip -6, brotli q5 —
//! moderate levels on purpose; q11 over ~6 MB is seconds of latency), and
//! the strong ETag carries an encoding suffix because validators are
//! per-representation.

use crate::dto::LibraryIndex;
use crate::error::{ErrorKind, Result};
use axum::body::Bytes;
use axum::http::HeaderMap;
use axum::http::header;
use exn::ResultExt;
use rawr_cache::{Repository, SnapshotToken};
use rawr_config::models::FandomConfig;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use time::UtcDateTime;
use tokio::sync::{Mutex, RwLock};

/// The `Cache-Control` value for the index and other revalidate-always
/// API responses.
pub const CACHE_CONTROL_REVALIDATE: &str = "private, no-cache";

/// The response encoding chosen for a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Encoding {
    Identity,
    Gzip,
    Brotli,
}

impl Encoding {
    /// Hand-rolled `Accept-Encoding` negotiation: token presence only,
    /// preferring brotli, then gzip. LAN clients (Safari, curl) don't
    /// send q-values worth honouring.
    pub fn negotiate(headers: &HeaderMap) -> Self {
        let accept =
            headers.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).unwrap_or_default().to_ascii_lowercase();
        let offered = |token: &str| accept.split(',').any(|part| part.split(';').next().unwrap_or("").trim() == token);
        if offered("br") {
            Self::Brotli
        } else if offered("gzip") {
            Self::Gzip
        } else {
            Self::Identity
        }
    }

    /// Value for the `Content-Encoding` header; `None` for identity.
    pub fn content_encoding(self) -> Option<&'static str> {
        match self {
            Self::Identity => None,
            Self::Gzip => Some("gzip"),
            Self::Brotli => Some("br"),
        }
    }

    /// Strong validators are per-representation, so the ETag carries the
    /// encoding.
    fn etag_suffix(self) -> &'static str {
        match self {
            Self::Identity => "id",
            Self::Gzip => "gz",
            Self::Brotli => "br",
        }
    }
}

/// One built snapshot of the index, in all three representations.
pub struct CachedIndex {
    /// `blake3(identity_body)[..16]` hex; also the payload's `snapshot`.
    pub snapshot: String,
    pub token: SnapshotToken,
    pub generation: u64,
    pub identity: Bytes,
    pub gzip: Bytes,
    pub brotli: Bytes,
}

impl CachedIndex {
    pub fn etag(&self, encoding: Encoding) -> String {
        format!("\"{}-{}\"", self.snapshot, encoding.etag_suffix())
    }

    pub fn body(&self, encoding: Encoding) -> Bytes {
        match encoding {
            Encoding::Identity => self.identity.clone(),
            Encoding::Gzip => self.gzip.clone(),
            Encoding::Brotli => self.brotli.clone(),
        }
    }
}

/// Shared, single-flight cache of the current index snapshot.
pub struct IndexCache {
    current: RwLock<Option<Arc<CachedIndex>>>,
    rebuild: Mutex<()>,
    generation: AtomicU64,
}

impl Default for IndexCache {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexCache {
    pub fn new() -> Self {
        Self {
            current: RwLock::new(None),
            rebuild: Mutex::new(()),
            generation: AtomicU64::new(0),
        }
    }

    /// Server-initiated change (an upload) never relies on tier-1
    /// aggregates: bumping the generation forces the next request to
    /// rebuild.
    pub fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// The freshest snapshot the cache knows about, if any — without
    /// checking whether it is stale. Used by `/meta`.
    pub async fn peek(&self) -> Option<Arc<CachedIndex>> {
        self.current.read().await.clone()
    }

    /// Return the current snapshot, rebuilding when the change token (or
    /// generation) moved. `force` skips tier 1 entirely — the manual
    /// refresh path (`Cache-Control: no-cache`).
    pub async fn current(
        &self,
        cache: &Repository,
        target: &str,
        fandoms: &FandomConfig,
        force: bool,
    ) -> Result<Arc<CachedIndex>> {
        let token = cache.snapshot_token(target).await.or_raise(|| ErrorKind::Cache)?;
        let generation = self.generation.load(Ordering::Relaxed);
        if !force && let Some(cached) = self.fresh(token, generation).await {
            return Ok(cached);
        }
        // Single-flight: concurrent requests queue here; whoever wins
        // rebuilds, the rest re-check and reuse.
        let _guard = self.rebuild.lock().await;
        let generation = self.generation.load(Ordering::Relaxed);
        if !force && let Some(cached) = self.fresh(token, generation).await {
            return Ok(cached);
        }
        let built = Arc::new(build(cache, target, fandoms, token, generation).await?);
        *self.current.write().await = Some(Arc::clone(&built));
        Ok(built)
    }

    async fn fresh(&self, token: SnapshotToken, generation: u64) -> Option<Arc<CachedIndex>> {
        let current = self.current.read().await;
        current.as_ref().filter(|c| c.token == token && c.generation == generation).map(Arc::clone)
    }
}

async fn build(
    cache: &Repository,
    target: &str,
    fandoms: &FandomConfig,
    token: SnapshotToken,
    generation: u64,
) -> Result<CachedIndex> {
    let works = cache.list_works_for_target(target).await.or_raise(|| ErrorKind::Cache)?;
    let target = target.to_string();
    let fandoms = fandoms.clone();
    let generated_at = UtcDateTime::now();
    // Transform + serialise + hash + compress are all CPU-bound over
    // megabytes; keep them off the async threads.
    tokio::task::spawn_blocking(move || -> Result<CachedIndex> {
        let mut index = LibraryIndex::build(&target, generated_at, &works, &fandoms)?;
        // The snapshot hash covers the body serialised with an empty
        // `snapshot` field; serialising twice (~ms) buys a validator that
        // can never diverge from the content.
        let unsalted = serde_json::to_vec(&index).or_raise(|| ErrorKind::Index)?;
        let snapshot = blake3::hash(&unsalted).to_hex()[..16].to_string();
        index.snapshot = snapshot.clone();
        let identity = serde_json::to_vec(&index).or_raise(|| ErrorKind::Index)?;

        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(6));
        gzip.write_all(&identity).or_raise(|| ErrorKind::Index)?;
        let gzip = gzip.finish().or_raise(|| ErrorKind::Index)?;

        let mut brotli_out = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut brotli_out, 4096, 5, 22);
            writer.write_all(&identity).or_raise(|| ErrorKind::Index)?;
        }

        tracing::info!(
            works = index.count,
            raw = identity.len(),
            gzip = gzip.len(),
            brotli = brotli_out.len(),
            "index rebuilt"
        );
        Ok(CachedIndex {
            snapshot,
            token,
            generation,
            identity: identity.into(),
            gzip: gzip.into(),
            brotli: brotli_out.into(),
        })
    })
    .await
    .or_raise(|| ErrorKind::Index)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(accept: &str) -> HeaderMap {
        let mut map = HeaderMap::new();
        map.insert(header::ACCEPT_ENCODING, accept.parse().unwrap());
        map
    }

    #[test]
    fn negotiates_encoding() {
        assert_eq!(Encoding::negotiate(&HeaderMap::new()), Encoding::Identity);
        assert_eq!(Encoding::negotiate(&headers("gzip, deflate, br")), Encoding::Brotli);
        assert_eq!(Encoding::negotiate(&headers("gzip;q=1.0, identity")), Encoding::Gzip);
        assert_eq!(Encoding::negotiate(&headers("identity")), Encoding::Identity);
        // "brotli" is not the token name; must not match "br".
        assert_eq!(Encoding::negotiate(&headers("brotli")), Encoding::Identity);
    }
}
