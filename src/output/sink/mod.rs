#[cfg(test)]
mod buffer;
mod print;

#[cfg(test)]
pub(crate) use self::buffer::BufferingOutput;
pub use self::print::PrintingOutput;
