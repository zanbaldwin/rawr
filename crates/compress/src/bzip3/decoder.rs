use super::{Bz3State, MAGIC, bound};
use std::io::{Error as IoError, ErrorKind, Read, Result as IoResult};

pub(crate) struct Bz3Decoder<R> {
    reader: R,
    state: Bz3State,
    buffer: Vec<u8>,
    pos: usize,
    len: usize,
}
impl<R: Read> Bz3Decoder<R> {
    pub(crate) fn new(mut reader: R) -> IoResult<Self> {
        let mut magic_bytes = [0u8; 5];
        reader.read_exact(&mut magic_bytes)?;
        if magic_bytes != MAGIC {
            return Err(IoError::new(ErrorKind::InvalidData, "invalid bzip3 magic bytes"));
        }
        let mut block_size_bytes = [0u8; 4];
        reader.read_exact(&mut block_size_bytes)?;
        let block_size = usize::try_from(i32::from_le_bytes(block_size_bytes))
            .map_err(|_| IoError::new(ErrorKind::InvalidData, "negative bzip3 block size"))?;
        Ok(Self {
            reader,
            // Safety: usize constructed from i32 cannot exceed i32::MAX.
            state: Bz3State::new(block_size)?,
            buffer: vec![0u8; bound(block_size)],
            pos: 0,
            len: 0,
        })
    }

    fn fill_buffer(&mut self) -> IoResult<bool> {
        let mut new_size_bytes = [0u8; 4];
        match self.reader.read_exact(&mut new_size_bytes) {
            Ok(()) => {},
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(false),
            Err(e) => return Err(e),
        }
        let mut orig_size_bytes = [0u8; 4];
        self.reader.read_exact(&mut orig_size_bytes)?;
        // Why, oh why, does Bzip3 use i32?! A negative value is semantically incorrect.
        // Is this a C thing? An FFI thing?
        let new_size = i32::from_le_bytes(new_size_bytes);
        let orig_size = i32::from_le_bytes(orig_size_bytes);
        self.reader.read_exact(&mut self.buffer[..new_size as usize])?;
        let decoded_size = self.state.decode_block(&mut self.buffer, new_size, orig_size)?;
        self.pos = 0;
        self.len = decoded_size as usize;
        Ok(true)
    }
}
impl<R: Read> Read for Bz3Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if self.pos >= self.len && !self.fill_buffer()? {
            return Ok(0);
        }
        let to_copy = buf.len().min(self.len - self.pos);
        buf[..to_copy].copy_from_slice(&self.buffer[self.pos..self.pos + to_copy]);
        self.pos += to_copy;
        Ok(to_copy)
    }
}
