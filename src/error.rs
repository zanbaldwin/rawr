use miette::Diagnostic;
use rawr_cache::error::Error as CacheError;
use rawr_compress::error::Error as CompressError;
use rawr_config::error::Error as ConfigError;
use rawr_library::error::Error as LibraryError;
use rawr_storage::error::Error as StorageError;
use std::fmt;

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// CLI error wrapper that bridges `exn::Exn<E>` errors from library crates
/// into miette's diagnostic reporting by walking the exn frame tree.
pub(crate) struct Error(miette::Report);

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl From<miette::Report> for Error {
    fn from(report: miette::Report) -> Self {
        Self(report)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self(miette::Report::msg(e.to_string()))
    }
}

impl From<ConfigError> for Error {
    fn from(e: ConfigError) -> Self {
        Self(miette::Report::new(ExnDiagnostic::from_frame(e.frame())))
    }
}

impl From<LibraryError> for Error {
    fn from(e: LibraryError) -> Self {
        Self(miette::Report::new(ExnDiagnostic::from_frame(e.frame())))
    }
}

impl From<CacheError> for Error {
    fn from(e: CacheError) -> Self {
        Self(miette::Report::new(ExnDiagnostic::from_frame(e.frame())))
    }
}

impl From<StorageError> for Error {
    fn from(e: StorageError) -> Self {
        Self(miette::Report::new(ExnDiagnostic::from_frame(e.frame())))
    }
}

impl From<CompressError> for Error {
    fn from(e: CompressError) -> Self {
        Self(miette::Report::new(ExnDiagnostic::from_frame(e.frame())))
    }
}

/// A miette Diagnostic built by recursively walking an exn Frame tree.
///
/// The first child of each frame maps to miette's cause chain (via `source()`),
/// while any additional children surface as miette `related()` diagnostics.
/// This preserves the full tree structure that exn captures, rather than
/// flattening it to a linear chain.
#[derive(Debug)]
struct ExnDiagnostic {
    message: String,
    children: Vec<ExnDiagnostic>,
}

impl ExnDiagnostic {
    fn from_frame(frame: &exn::Frame) -> Self {
        Self {
            message: frame.error().to_string(),
            children: frame.children().iter().map(Self::from_frame).collect(),
        }
    }
}

impl fmt::Display for ExnDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ExnDiagnostic {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.children.first().map(|c| c as &(dyn std::error::Error + 'static))
    }
}

impl Diagnostic for ExnDiagnostic {
    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        if self.children.len() > 1 {
            Some(Box::new(self.children[1..].iter().map(|c| c as &dyn Diagnostic)))
        } else {
            None
        }
    }
}
