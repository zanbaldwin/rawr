use rawr_extract::models::Metadata;
use std::collections::HashMap;
use std::fmt::{Display, Formatter, Result as FmtResult};

pub(crate) struct CssVariables {
    variables: HashMap<&'static str, String>,
}
impl From<&Metadata> for CssVariables {
    fn from(m: &Metadata) -> Self {
        let mut map = HashMap::new();
        // Some values that might be useful in creating title pages, page headers/footers, etc.
        map.insert("work-id", m.work_id.to_string());
        map.insert("summary", m.summary.as_ref().map(|s| s.to_string()).unwrap_or_default());
        map.insert("words", human_number(m.words));
        map.insert("chapters-written", m.chapters.written.to_string());
        map.insert("chapters-total", m.chapters.total.map_or("?".into(), |t| t.to_string()));
        map.insert("rating", m.rating.map(|r| r.as_str().to_string()).unwrap_or_default());
        map.insert("published", m.published.to_string());
        map.insert("updated", m.last_modified.to_string());
        Self { variables: map }
    }
}
impl Display for CssVariables {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        writeln!(f, "<style>\n:root {{")?;
        for (key, value) in self.variables.iter() {
            writeln!(f, "    --meta-{}: \"{}\";", key, css_escape_string(value))?;
        }
        write!(f, "}}\n</style>")
    }
}

/// https://www.w3.org/TR/css-syntax-3/#consume-string-token
fn css_escape_string(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\a ")
        .replace('\r', "\\d ")
        .replace('\x0C', "\\c ")
        .replace('\0', "\\fffd ")
}

/// Format a number with comma separators: 21837 → "21,837"
fn human_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}
