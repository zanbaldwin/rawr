use std::fmt::{Display, Formatter, Result as FmtResult};

/// A work's position within a series.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SeriesPosition {
    /// AO3 Series ID
    pub id: u64,
    /// Series name
    pub name: String,
    /// Position in series (1-indexed)
    #[cfg_attr(feature = "serde", serde(rename = "pos"))]
    pub position: u32,
}
impl SeriesPosition {
    pub fn new(id: u64, name: impl Into<String>, position: u32) -> Self {
        Self { id, name: name.into(), position }
    }
}
impl Display for SeriesPosition {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "Part {} of \"{}\"", self.position, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::{from_str as from_json, to_string as to_json};

    #[rstest]
    #[case(SeriesPosition{id: 123, name: "Series".to_string(), position: 5}, r#"{"id":123,"name":"Series","pos":5}"#)]
    fn test_series_position_serialize(#[case] input: SeriesPosition, #[case] expected: impl AsRef<str>) {
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), expected.as_ref());
    }

    #[rstest]
    #[case(SeriesPosition{id: 123, name: "Series".to_string(), position: 5}, r#"{"id":123,"name":"Series","pos":5}"#)]
    fn test_series_position_deserialize(#[case] expected: SeriesPosition, #[case] input: impl AsRef<str>) {
        let obj = from_json::<SeriesPosition>(input.as_ref()).unwrap();
        assert_eq!(obj, expected);
    }

    #[test]
    fn test_series_position_vec_serialize() {
        let input = vec![
            SeriesPosition {
                id: 1,
                name: "S1".to_string(),
                position: 3,
            },
            SeriesPosition {
                id: 2,
                name: "S2".to_string(),
                position: 7,
            },
        ];
        let json = to_json(&input).unwrap();
        assert_eq!(json.as_str(), r#"[{"id":1,"name":"S1","pos":3},{"id":2,"name":"S2","pos":7}]"#);
    }

    #[test]
    fn test_series_position_vec_deserialize() {
        let expected = vec![
            SeriesPosition {
                id: 1,
                name: "S1".to_string(),
                position: 3,
            },
            SeriesPosition {
                id: 2,
                name: "S2".to_string(),
                position: 7,
            },
        ];
        let obj =
            from_json::<Vec<SeriesPosition>>(r#"[{"id":1,"name":"S1","pos":3},{"id":2,"name":"S2","pos":7}]"#).unwrap();
        assert_eq!(obj, expected);
    }
}
