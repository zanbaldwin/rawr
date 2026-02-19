mod conflict;
pub(crate) mod error;
mod file;

use rawr_compress::Compression;
use rawr_storage::BackendHandle;

use crate::PathGenerator;

pub use self::file::{Action, organize_file};

pub struct Context {
    template: PathGenerator,
    compression: Option<Compression>,
    trash: Option<BackendHandle>,
}
impl Context {
    pub fn new(
        template: PathGenerator,
        compression: impl Into<Option<Compression>>,
        trash: impl Into<Option<BackendHandle>>,
    ) -> Self {
        Self {
            template,
            compression: compression.into(),
            trash: trash.into(),
        }
    }
}
