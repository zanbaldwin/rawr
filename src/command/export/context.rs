use std::str::FromStr;

/// A user-supplied reference to a work for export.
#[derive(Clone, Debug)]
pub(crate) enum WorkRef {
    /// Just a work ID — export the best version.
    BestWork(u64),
    /// Work ID with CRC32 hash — export a specific version.
    /// The u32 is parsed from 8 hex digits (e.g., `12345@37bc3355`).
    /// Matched against `Version.crc32` during resolution.
    WorkVersion(u64, u32),
    /// File path — export the file at this path.
    FilePath(String),
}
impl FromStr for WorkRef {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Ok(id) = s.parse::<u64>() {
            return Ok(WorkRef::BestWork(id));
        }
        if let Some((id_str, hex)) = s.split_once('@')
            && let Ok(id) = id_str.parse::<u64>()
            && let Ok(crc) = u32::from_str_radix(hex, 16)
        {
            return Ok(WorkRef::WorkVersion(id, crc));
        }
        Ok(WorkRef::FilePath(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_work_id() {
        let r: WorkRef = "12345".parse().unwrap();
        assert!(matches!(r, WorkRef::BestWork(12345)));
    }

    #[test]
    fn parse_work_id_version() {
        let r: WorkRef = "12345@37bc3355".parse().unwrap();
        assert!(matches!(r, WorkRef::WorkVersion(12345, 0x37bc3355)));
    }

    #[test]
    fn parse_file_path() {
        let r: WorkRef = "fandom/work.html.bz2".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(p) if p == "fandom/work.html.bz2"));
    }

    #[test]
    fn parse_bare_at() {
        let r: WorkRef = "12345@".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(_)));
    }

    #[test]
    fn parse_non_hex_hash() {
        let r: WorkRef = "12345@zzzz".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(_)));
    }

    #[test]
    fn parse_non_numeric_id() {
        let r: WorkRef = "abc@1234".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(_)));
    }

    #[test]
    fn parse_overflow_hash() {
        let r: WorkRef = "12345@fffffffff".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(_)));
    }
}
