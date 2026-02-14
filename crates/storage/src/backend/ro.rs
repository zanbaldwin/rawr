//! Readonly filesystem storage backend.
//!
//! This module provides a storage backend implementation that wraps other
//! implementations and prevents write operations from executing, but
//! indicating success on return.

use async_trait::async_trait;
use std::path::Path;

use crate::{BackendHandle, StorageBackend, backend::FileInfoStream, error::Result, file::FileInfo};

/// Read-only storage backend.
///
/// Wraps another backend and silently drops all write operations, logging an
/// [`info event`](tracing::Event).
#[derive(Clone)]
pub struct ReadOnlyBackend {
    inner: BackendHandle,
}
impl ReadOnlyBackend {
    pub fn new(inner: BackendHandle) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl StorageBackend for ReadOnlyBackend {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn list_stream<'a>(&'a self, prefix: Option<&'a Path>) -> FileInfoStream<'a> {
        self.inner.list_stream(prefix)
    }

    async fn exists(&self, path: &Path) -> Result<bool> {
        self.inner.exists(path).await
    }

    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        self.inner.read(path).await
    }

    async fn read_head(&self, path: &Path, bytes: usize) -> Result<Vec<u8>> {
        self.inner.read_head(path, bytes).await
    }

    async fn write(&self, path: &Path, data: &[u8]) -> Result<()> {
        tracing::info!(path = %path.display(), bytes = data.len(), "Skipping write during read-only mode");
        Ok(())
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        tracing::info!(path = %path.display(), "Skipping delete during read-only mode");
        Ok(())
    }

    async fn rename(&self, from: &Path, _to: &Path) -> Result<()> {
        tracing::info!(path = %from.display(), "Skipping rename/move during read-only mode");
        Ok(())
    }

    async fn stat(&self, path: &Path) -> Result<FileInfo> {
        self.inner.stat(path).await
    }
}
