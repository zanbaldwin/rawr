use regex::Regex;
use scraper::Selector;
use std::sync::LazyLock;

const URL_SEGMENT: &str = "([^/]+)";
const SAFE_END: &str = "(?:$|\\?|#|/)";
const SCHEME_HOST: &str = "^https?://archiveofourown\\.org";

/// Selector for the work URL in the preface. This is used to determine if the document is valid.
pub(crate) static WORK_URL_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("div#preface p.message a[href]").unwrap());
pub(crate) static WORK_URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    let regex = format!(r"{}/works/(\d+){}", SCHEME_HOST, SAFE_END);
    Regex::new(regex.as_str()).unwrap()
});

pub(crate) static TITLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("#preface .meta h1").unwrap());

pub(crate) static SERIES_URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    let regex = format!(r"{}/series/(\d+){}", SCHEME_HOST, SAFE_END);
    Regex::new(regex.as_str()).unwrap()
});

pub(crate) static BYLINE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#preface .byline a[rel='author']").unwrap());
pub(crate) static AUTHOR_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    let regex = format!(r"{}/users/{}/pseuds/{}{}", SCHEME_HOST, URL_SEGMENT, URL_SEGMENT, SAFE_END);
    Regex::new(regex.as_str()).unwrap()
});

pub(crate) static TAGS_DL_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("#preface dl.tags").unwrap());

pub(crate) static DT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("dt").unwrap());

pub(crate) static DD_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("dd").unwrap());

pub(crate) static SUMMARY_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#preface .meta blockquote.userstuff").unwrap());

pub(crate) static CHAPTERS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Chapters:\s*(\d{1,3}(?:,?\d{3})*)/(\d{1,3}(?:,?\d{3})*|\?)").unwrap());

pub(crate) static WORDS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Words:\s*(\d{1,3}(?:,?\d{3})*)").unwrap());

pub(crate) static DATE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(Updated|Completed|Published):\s*(\d{4})-(\d{1,2})-(\d{1,2})").unwrap());

pub(crate) static ANCHOR_SELECTOR: LazyLock<Selector> = LazyLock::new(|| Selector::parse("a").unwrap());
