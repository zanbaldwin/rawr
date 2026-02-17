mod chrome;
pub mod error;
mod render;
mod style;

use crate::chrome::Chrome;
use crate::error::{Error, Result};
pub use crate::render::Output;
pub use crate::style::StyleConfig;

pub type TempFile = tempfile::NamedTempFile;

pub struct Renderer {
    chrome: Chrome,
    styles: StyleConfig,
}
impl Renderer {
    pub fn new(styles: StyleConfig) -> Result<Self> {
        Ok(Self { chrome: Chrome::discover()?, styles })
    }
}
impl TryFrom<StyleConfig> for Renderer {
    type Error = Error;
    fn try_from(styles: StyleConfig) -> std::result::Result<Self, Self::Error> {
        Renderer::new(styles)
    }
}
