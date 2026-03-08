//! Readonly filesystem storage backend.
//!
//! This module provides a storage backend implementation that wraps other
//! implementations and prevents write operations from executing, but
//! indicating success on return.

use crate::ValidatedPath;
use async_trait::async_trait;
use opendal::Operator;

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

    fn list_stream<'a>(&'a self, prefix: Option<&'a ValidatedPath>) -> FileInfoStream<'a> {
        self.inner.list_stream(prefix)
    }

    async fn exists(&self, path: &ValidatedPath) -> Result<bool> {
        self.inner.exists(path).await
    }

    async fn read(&self, path: &ValidatedPath) -> Result<Vec<u8>> {
        self.inner.read(path).await
    }

    async fn read_head(&self, path: &ValidatedPath, bytes: usize) -> Result<Vec<u8>> {
        self.inner.read_head(path, bytes).await
    }

    async fn write(&self, path: &ValidatedPath, data: &[u8]) -> Result<()> {
        tracing::info!(path = %path, bytes = data.len(), "Skipping write during read-only mode");
        Ok(())
    }

    async fn delete(&self, path: &ValidatedPath) -> Result<()> {
        tracing::info!(path = %path, "Skipping delete during read-only mode");
        Ok(())
    }

    async fn rename(&self, from: &ValidatedPath, _to: &ValidatedPath) -> Result<()> {
        tracing::info!(path = %from, "Skipping rename/move during read-only mode");
        Ok(())
    }

    async fn stat(&self, path: &ValidatedPath) -> Result<FileInfo> {
        self.inner.stat(path).await
    }

    async fn reader(&self, path: &ValidatedPath) -> Result<BoxedReader> {
        self.inner.reader(path).await
    }

    async fn writer(&self, path: &ValidatedPath) -> Result<BoxedWriter> {
        tracing::info!(path = %path, "Skipping writer during read-only mode");
        Ok(Box::new(futures::io::sink()))
    }
}
