use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fs;
use std::ops::Deref;

/// A configuration value that can be loaded from a file using the `file://` scheme.
///
/// When deserializing, if the value starts with `file://`, the remainder is treated
/// as a filesystem path and the file contents are read and trimmed. Otherwise, the
/// value is used directly.
///
/// # Examples
///
/// ```yaml
/// # Direct value
/// key_secret: "my-secret-key"
///
/// # Load from file (Docker secrets pattern)
/// key_secret: "file:///run/secrets/aws_secret_key"
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaybeFile {
    value: String,
    file: Option<String>,
}
impl MaybeFile {
    /// Create a new `MaybeFile` with a direct value.
    pub fn new(value: impl Into<String>, file: Option<impl Into<String>>) -> Self {
        Self {
            value: value.into(),
            file: file.map(|f| f.into()),
        }
    }

    /// Consume and return the inner string value.
    pub fn into_inner(self) -> String {
        self.value
    }
}
impl Deref for MaybeFile {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl AsRef<str> for MaybeFile {
    fn as_ref(&self) -> &str {
        &self.value
    }
}
impl<'d> Deserialize<'d> for MaybeFile {
    fn deserialize<D: Deserializer<'d>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.strip_prefix("file://") {
            Some(path) => MaybeFile::new(
                fs::read_to_string(path)
                    .map_err(|e| serde::de::Error::custom(format!("failed to read value from file '{path}': {e}")))?
                    .trim()
                    .to_string(),
                Some(path),
            ),
            None => MaybeFile::new(value, None::<String>),
        })
    }
}
impl Serialize for MaybeFile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.file {
            Some(path) => format!("file://{}", path).serialize(serializer),
            None => self.value.serialize(serializer),
        }
    }
}
