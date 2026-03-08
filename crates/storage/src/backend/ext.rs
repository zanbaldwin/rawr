//! Okay, so I'm gonna try an idea that I thought when loudly complaining "urgh,
//! I forgot to import exn's OptionExt AGAIN".
//!
//! I can't have generic `impl TryValidatePath` in StorageBackend, otherwise the
//! trait will stop being dyn, and I'm too lazy to figure out the non-dyn way of
//! doing things. So I've hardcoded `&ValidatedPath` into the trait, because
//! that's what the majority of the methods need.
//!
//! But I want to get `impl TryValidatePath` working, because fuck you Rust,
//! that's why. I also don't want to do this in the main trait, because honestly
//! they never need to take ownership. I'm just doing this because the compiler
//! told me no. It'll be a learning exercise.

use crate::TryValidatePath;
use crate::backend::{BoxedReader, BoxedWriter, FileInfoStream, StorageBackend};
use crate::error::Result;
use crate::file::FileInfo;
use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;

impl<T: StorageBackend + ?Sized> StorageBackendExt for T {}

#[async_trait]
pub trait StorageBackendExt: StorageBackend {
    /// See [`StorageBackend::list`]
    async fn path_list(&self, prefix: Option<impl TryValidatePath + Send>) -> Result<Vec<FileInfo>> {
        self.list(prefix.map(|p| p.try_validate()).transpose()?.as_ref()).await
    }

    /// See [`StorageBackend::list_stream`]
    fn path_list_stream<'a>(&'a self, prefix: Option<impl TryValidatePath>) -> Result<FileInfoStream<'a>> {
        let validated = prefix.map(|p| p.try_validate()).transpose()?;
        Ok(Box::pin(stream! {
            while let Some(item) = self.list_stream(validated.as_ref()).next().await {
                yield item;
            }
        }))
    }

    /// See [`StorageBackend::exists`]
    async fn path_exists(&self, path: impl TryValidatePath + Send) -> Result<bool> {
        self.exists(&path.try_validate()?).await
    }

    /// See [`StorageBackend::read`]
    async fn path_read(&self, path: impl TryValidatePath + Send) -> Result<Vec<u8>> {
        self.read(&path.try_validate()?).await
    }

    /// See [`StorageBackend::read_head`]
    async fn path_read_head(&self, path: impl TryValidatePath + Send, bytes: usize) -> Result<Vec<u8>> {
        self.read_head(&path.try_validate()?, bytes).await
    }

    /// See [`StorageBackend::write`]
    async fn path_write(&self, path: impl TryValidatePath + Send, data: &[u8]) -> Result<()> {
        self.write(&path.try_validate()?, data).await
    }

    /// See [`StorageBackend::delete`]
    async fn path_delete(&self, path: impl TryValidatePath + Send) -> Result<()> {
        self.delete(&path.try_validate()?).await
    }

    /// See [`StorageBackend::rename`]
    async fn path_rename(&self, from: impl TryValidatePath + Send, to: impl TryValidatePath + Send) -> Result<()> {
        self.rename(&from.try_validate()?, &to.try_validate()?).await
    }

    /// See [`StorageBackend::stat`]
    async fn path_stat(&self, path: impl TryValidatePath + Send) -> Result<FileInfo> {
        self.stat(&path.try_validate()?).await
    }

    /// See [`StorageBackend::reader`]
    async fn path_reader(&self, path: impl TryValidatePath + Send) -> Result<BoxedReader> {
        self.reader(&path.try_validate()?).await
    }

    /// See [`StorageBackend::writer`].
    async fn path_writer(&self, path: impl TryValidatePath + Send) -> Result<BoxedWriter> {
        self.writer(&path.try_validate()?).await
    }
}
