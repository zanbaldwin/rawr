use super::{Bz3State, MAGIC, bound};
use std::io::{Error as IoError, Result as IoResult, Write};

pub(crate) struct Bz3Encoder<W: Write> {
    writer: W,
    state: Bz3State,
    buffer: Vec<u8>,
    pos: usize,
    block_size: usize,
    finished: bool,
}
impl<W: Write> Bz3Encoder<W> {
    pub(crate) fn new(mut writer: W, block_size: usize) -> IoResult<Self> {
        let state = Bz3State::new(block_size)?;
        writer.write_all(&MAGIC)?;
        writer.write_all(&state.block_size.to_le_bytes())?;
        Ok(Self {
            writer,
            state,
            buffer: vec![0u8; bound(block_size)],
            pos: 0,
            block_size,
            finished: false,
        })
    }

    fn compress_block(&mut self) -> IoResult<()> {
        if self.pos == 0 {
            return Ok(());
        }
        let orig_size = self.pos as i32;
        let new_size = self.state.encode_block(&mut self.buffer, orig_size)?;
        let mut header = [0u8; 8];
        header[..4].copy_from_slice(&new_size.to_le_bytes());
        header[4..].copy_from_slice(&orig_size.to_le_bytes());
        self.writer.write_all(&header)?;
        self.writer.write_all(&self.buffer[..new_size as usize])?;
        self.pos = 0;
        Ok(())
    }

    /// Flush any remaining buffered data and finalize the bzip3 stream.
    ///
    /// Prefer calling this explicitly for proper error handling. If omitted,
    /// `Drop` will attempt finalization but errors will be silently ignored.
    pub(crate) fn finish(&mut self) -> IoResult<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        self.compress_block()?;
        self.writer.flush()
    }
}
impl<W: Write> Drop for Bz3Encoder<W> {
    fn drop(&mut self) {
        _ = self.finish();
    }
}
impl<W: Write> Write for Bz3Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        if self.finished {
            return Err(IoError::other("write after finish"));
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let space_remaining = self.block_size.saturating_sub(self.pos);
        let to_copy = buf.len().min(space_remaining);
        self.buffer[self.pos..self.pos + to_copy].copy_from_slice(&buf[..to_copy]);
        self.pos += to_copy;
        if self.pos == self.block_size {
            self.compress_block()?;
        }
        Ok(to_copy)
    }

    /// Bzip3 operates on complete blocks; partial data cannot be flushed
    /// without finalizing. Call `finish()` explicitly to seal the stream.
    fn flush(&mut self) -> IoResult<()> {
        self.writer.flush()
    }
}
