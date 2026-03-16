//! Bzip3 compression via `libbzip3-sys` FFI.
//!
//! Provides sync [`Read`]/[`Write`] wrappers around the block-level bzip3 C API.
//!
//! > **WARNING:** Bzip3 support is **experimental**; use the `bzip3` feature at
//! > your own risk.

mod decoder;
mod encoder;

pub(crate) use self::decoder::Bz3Decoder;
pub(crate) use self::encoder::Bz3Encoder;
use std::borrow::Cow;
use std::ffi::CStr;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::ptr::NonNull;

pub(crate) const MAGIC: [u8; 5] = *b"BZ3v1";
/// Use a Bzip3 block size of 8MiB. ~99% of all HTML files will fit inside this.
pub(crate) const DEFAULT_BLOCK_SIZE: usize = 8 * 1_024 * 1_024;

/// Returns the maximum compressed size for a given block size (worst-case expansion).
fn bound(block_size: usize) -> usize {
    unsafe { libbzip3_sys::bz3_bound(block_size) }
}

struct Bz3State {
    raw: NonNull<libbzip3_sys::bz3_state>,
    block_size: i32,
}
impl Bz3State {
    fn new(block_size: usize) -> IoResult<Self> {
        // Safety: If anyone's stupid enough to pass in a block size of
        // more than two gigabytes, then they deserve to have their value
        // silently modified without being informed.
        let block_size = i32::try_from(block_size).unwrap_or(i32::MAX);
        // Look, ma! My first unsafe!
        let raw = NonNull::new(unsafe { libbzip3_sys::bz3_new(block_size) })
            .ok_or_else(|| IoError::new(ErrorKind::InvalidInput, "bz3_new failed: invalid block size or OOM"))?;
        Ok(Self { raw, block_size })
    }

    fn last_error_message(&self) -> Cow<'_, str> {
        let msg = unsafe { libbzip3_sys::bz3_strerror(self.raw.as_ptr()) };
        // God I miss ternary operators :'(
        if msg.is_null() {
            return Cow::Borrowed("unknown bzip3 error");
        }
        unsafe { CStr::from_ptr(msg) }.to_string_lossy()
    }

    fn encode_block(&mut self, buf: &mut [u8], size: i32) -> IoResult<i32> {
        let new_size = unsafe { libbzip3_sys::bz3_encode_block(self.raw.as_ptr(), buf.as_mut_ptr(), size) };
        match new_size {
            n if n < 0 => Err(IoError::other(self.last_error_message())),
            n => Ok(n),
        }
    }

    fn decode_block(&mut self, buf: &mut [u8], compressed_size: i32, orig_size: i32) -> IoResult<i32> {
        let rc = unsafe {
            libbzip3_sys::bz3_decode_block(self.raw.as_ptr(), buf.as_mut_ptr(), buf.len(), compressed_size, orig_size)
        };
        match rc {
            n if n < 0 => Err(IoError::new(ErrorKind::InvalidData, self.last_error_message())),
            n => Ok(n),
        }
    }
}
impl Drop for Bz3State {
    fn drop(&mut self) {
        unsafe { libbzip3_sys::bz3_free(self.raw.as_ptr()) };
    }
}
// SAFETY: bz3_state is a single-owner heap allocation with no thread-local
// storage. Access is gated through &mut self, preventing concurrent use.
unsafe impl Send for Bz3State {}
