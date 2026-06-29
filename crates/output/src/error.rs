use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Entering the alternative screen requires an interactive terminal.
    /// The held value names the backend that refused the request.
    AltScreenUnavailable(&'static str),
    /// An I/O error from the terminal or an interactive prompt.
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AltScreenUnavailable(what) => {
                write!(f, "Cannot enter alternative screen; {what} is not an interactive terminal")
            }
            Error::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::AltScreenUnavailable(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(feature = "confirm")]
impl From<dialoguer::Error> for Error {
    fn from(e: dialoguer::Error) -> Self {
        // dialoguer::Error is effectively an io wrapper; stringify to stay
        // robust against its #[non_exhaustive] shape rather than matching.
        Self::Io(std::io::Error::other(e.to_string()))
    }
}
