use regex::Regex;
use scraper::Selector;
use std::sync::LazyLock;

const URL_SEGMENT: &str = "([^/]+)";
const SAFE_END: &str = "(?:$|\\?|#|/)";
const SCHEME_HOST: &str = "^https?://archiveofourown\\.org";

macro_rules! selector {
    ($name:ident, $css:expr) => {
        pub(crate) static $name: LazyLock<Selector> = LazyLock::new(|| Selector::parse($css).unwrap());
    };
}

macro_rules! regex {
    ($name:ident, $regex:expr) => {
        pub(crate) static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($regex).unwrap());
    };
}

// Selector for the work URL in the preface. This is used to determine if the document is valid.
selector!(WORK_URL_SELECTOR, "div#preface p.message a[href]");
regex!(WORK_URL_REGEX, format!(r"{}/works/(\d+){}", SCHEME_HOST, SAFE_END).as_str());
selector!(TITLE_SELECTOR, "#preface .meta h1");
regex!(SERIES_URL_REGEX, format!(r"{}/series/(\d+){}", SCHEME_HOST, SAFE_END).as_str());
selector!(BYLINE_SELECTOR, "#preface .byline a[rel='author']");
regex!(
    AUTHOR_REGEX,
    format!(r"{}/users/{}/pseuds/{}{}", SCHEME_HOST, URL_SEGMENT, URL_SEGMENT, SAFE_END).as_str()
);
selector!(TAGS_DL_SELECTOR, "#preface dl.tags");
selector!(DT_SELECTOR, "dt");
selector!(DD_SELECTOR, "dd");
selector!(SUMMARY_SELECTOR, "#preface .meta blockquote.userstuff");
regex!(CHAPTERS_REGEX, r"Chapters:\s*(\d{1,3}(?:,?\d{3})*)/(\d{1,3}(?:,?\d{3})*|\?)");
regex!(WORDS_REGEX, r"Words:\s*(\d{1,3}(?:,?\d{3})*)");
regex!(DATE_REGEX, r"(Updated|Completed|Published):\s*(\d{4})-(\d{1,2})-(\d{1,2})");
selector!(ANCHOR_SELECTOR, "a");
