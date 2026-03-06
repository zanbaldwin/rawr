//! Readonly filesystem storage backend.
//!
//! This module provides a storage backend implementation that wraps other
//! implementations and prevents write operations from executing, but
//! indicating success on return.

use async_trait::async_trait;
use opendal::Operator;
use std::path::Path;

use crate::{
    BackendHandle, StorageBackend,
    backend::{BoxedReader, BoxedWriter, FileInfoStream, OperatorAware},
    error::Result,
    file::FileInfo,
};

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
impl OperatorAware for ReadOnlyBackend {
    fn operator(&self) -> &Operator {
        self.inner.operator()
    }
}
#[async_trait]
impl StorageBackend for ReadOnlyBackend {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn list_stream<'a>(&'a self, prefix: Option<&'a Path>) -> Result<FileInfoStream<'a>> {
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

    async fn reader(&self, path: &Path) -> Result<BoxedReader> {
        self.inner.reader(path).await
    }

    async fn writer(&self, path: &Path) -> Result<BoxedWriter> {
        tracing::info!(path = %path.display(), "Skipping writer during read-only mode");
        Ok(Box::new(futures::io::sink()))
    }
}
