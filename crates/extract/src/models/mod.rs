mod author;
mod chapters;
mod fandom;
mod lang;
mod metadata;
mod rating;
mod series;
mod tag;
mod version;
mod warning;

pub use self::author::Author;
pub use self::chapters::Chapters;
pub use self::fandom::Fandom;
pub use self::lang::Language;
pub use self::metadata::Metadata;
pub use self::rating::Rating;
pub use self::series::SeriesPosition;
pub use self::tag::{Tag, TagKind};
pub use self::version::Version;
pub use self::warning::Warning;

fn sanitize(s: impl AsRef<str>) -> String {
    s.as_ref().trim().to_lowercase().replace('/', "").replace('-', "").replace('_', "").replace(' ', "")
}
