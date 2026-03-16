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
regex!(WORK_URL_REGEX, format!(r"{}/works/([0-9]+){}", SCHEME_HOST, SAFE_END).as_str());
selector!(TITLE_SELECTOR, "#preface .meta h1");
regex!(SERIES_URL_REGEX, format!(r"{}/series/([0-9]+){}", SCHEME_HOST, SAFE_END).as_str());
selector!(BYLINE_SELECTOR, "#preface .byline a[rel='author']");
regex!(
    AUTHOR_REGEX,
    format!(r"{}/users/{}/pseuds/{}{}", SCHEME_HOST, URL_SEGMENT, URL_SEGMENT, SAFE_END).as_str()
);
selector!(TAGS_DL_SELECTOR, "#preface dl.tags");
selector!(DT_SELECTOR, "dt");
selector!(DD_SELECTOR, "dd");
selector!(SUMMARY_SELECTOR, "#preface .meta blockquote.userstuff");
regex!(
    CHAPTERS_REGEX,
    r"Chapters:[ \t\n\r\x{A0}]*([0-9]{1,3}(?:,?[0-9]{3})*)/([0-9]{1,3}(?:,?[0-9]{3})*|\?)"
);
regex!(WORDS_REGEX, r"Words:[ \t\n\r\x{A0}]*([0-9]{1,3}(?:,?[0-9]{3})*)");
regex!(DATE_REGEX, r"(Updated|Completed|Published):[ \t\n\r\x{A0}]*([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})");
selector!(ANCHOR_SELECTOR, "a");
// Need to account for non-breaking spaces in the regex (eg, "\s" but without
// the Unicode Perl feature: "[ \t\n\r\x{A0}]")
regex!(
    SERIES_POSITION_REGEX,
    r"Part[ \t\n\r\x{A0}]+([0-9]{1,3}(?:,?[0-9]{3})*)[ \t\n\r\x{A0}]+of[ \t\n\r\x{A0}]+"
);

#[cfg(feature = "chapters")]
selector!(CHAPTER_META_GROUP_SELECTOR, "div#chapters > div.meta.group");
#[cfg(feature = "chapters")]
selector!(CHAPTER_HEADING_SELECTOR, "h2.heading");
#[cfg(feature = "chapters")]
selector!(SINGLE_CHAPTER_SELECTOR, "div#chapters > div.userstuff");
#[cfg(feature = "chapters")]
selector!(CHAPTER_ENDNOTES_SELECTOR, r#"div#chapters > div[id^="endnotes"]"#);
#[cfg(feature = "chapters")]
selector!(CHAPTER_NOTES_BLOCKQUOTE_SELECTOR, "blockquote.userstuff");
#[cfg(feature = "chapters")]
selector!(PREFACE_META_SELECTOR, "#preface .meta");
#[cfg(feature = "chapters")]
selector!(AFTERWORD_ENDNOTES_SELECTOR, "div#afterword div#endnotes");
