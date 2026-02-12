//! Language name to ISO code mapping based on AO3's official language dropdown.
//!
//! The mappings are derived from the `<select>` element in AO3's work search form,
//! where each `<option>` has a `lang` attribute containing the ISO code and the
//! element text contains the display name.

use std::collections::HashMap;
use std::convert::Infallible;
use std::str::FromStr;
use std::sync::LazyLock;

/// Language information for a work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Language {
    /// Language name as displayed on AO3 (e.g., "English")
    pub name: String,
    /// ISO 639 code (2 or 3 letters) if determinable (e.g., "en")
    pub iso_code: Option<String>,
}
impl Language {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let iso_code = Self::name_to_iso(&name).map(|s| s.to_string());
        Self { name, iso_code }
    }
    /// Returns the ISO-639 code for a given AO3 language display name.
    ///
    /// # Examples
    ///
    /// ```
    /// use rawr_extract::models::Language;
    /// assert_eq!(Language::name_to_iso("English"), Some("en"));
    /// assert_eq!(Language::name_to_iso("Unknown Language"), None);
    /// ```
    pub fn name_to_iso(name: &str) -> Option<&'static str> {
        LANGUAGES_REVERSED.get(name).copied()
    }

    /// Returns the AO3 language display name for a given ISO-639 code.
    ///
    /// # Examples
    ///
    /// ```
    /// use rawr_extract::models::Language;
    /// assert_eq!(Language::iso_to_name("en"), Some("English"));
    /// assert_eq!(Language::iso_to_name("Unknown ISO"), None);
    /// ```
    pub fn iso_to_name(iso: &str) -> Option<&'static str> {
        LANGUAGES.get(iso).copied()
    }
}
impl FromStr for Language {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}
impl From<String> for Language {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Map of AO3 language ISO codes to their display names.
///
/// Built from  AO3's official language dropdown.
static LANGUAGES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("so", "af Soomaali"),
        ("afr", "Afrikaans"),
        ("ain", "Aynu itak | アイヌ イタㇰ"),
        ("akk", "𒀝𒅗𒁺𒌑"),
        ("ar", "العربية"),
        ("amh", "አማርኛ"),
        ("egy", "𓂋𓏺𓈖 𓆎𓅓𓏏𓊖"),
        ("oji", "Anishinaabemowin"),
        ("arc", "ܐܪܡܝܐ | ארמיא"),
        ("hy", "հայերեն"),
        ("ase", "American Sign Language"),
        ("ast", "asturianu"),
        ("azj", "Azərbaycan dili | آذربایجان دیلی"),
        ("id", "Bahasa Indonesia"),
        ("ms", "Bahasa Malaysia"),
        ("bg", "Български"),
        ("bn", "বাংলা"),
        ("jv", "Basa Jawa"),
        ("ba", "Башҡорт теле"),
        ("be", "беларуская"),
        ("bar", "Boarisch"),
        ("bos", "Bosanski"),
        ("br", "Brezhoneg"),
        ("bfi", "British Sign Language"),
        ("bua", "Буряад хэлэн | ᠪᠤᠷᠢᠶᠠᠳ ᠮᠣᠩᠭᠣᠯ ᠬᠡᠯᠡ"),
        ("ca", "Català"),
        ("ceb", "Cebuano"),
        ("cs", "Čeština"),
        ("chn", "Chinuk Wawa"),
        ("crh", "къырымтатар тили | qırımtatar tili"),
        ("cy", "Cymraeg"),
        ("da", "Dansk"),
        ("de", "Deutsch"),
        ("et", "eesti keel"),
        ("el", "Ελληνικά"),
        ("sux", "𒅴𒂠"),
        ("en", "English"),
        ("ang", "Eald Englisċ"),
        ("es", "Español"),
        ("eo", "Esperanto"),
        ("eu", "Euskara"),
        ("fa", "فارسی"),
        ("fil", "Filipino"),
        ("cha", "Finuʼ Chamorro"),
        ("fr", "Français"),
        ("frr", "Friisk"),
        ("fry", "Frysk"),
        ("fur", "Furlan"),
        ("ga", "Gaeilge"),
        ("gd", "Gàidhlig"),
        ("gl", "Galego"),
        ("got", "𐌲𐌿𐍄𐌹𐍃𐌺𐌰"),
        ("gyn", "Creolese"),
        ("hak", "中文-客家话"),
        ("ko", "한국어"),
        ("hau", "Hausa | هَرْشَن هَوْسَ"),
        ("hi", "हिन्दी"),
        ("hr", "Hrvatski"),
        ("haw", "ʻŌlelo Hawaiʻi"),
        ("ia", "Interlingua"),
        ("zu", "isiZulu"),
        ("is", "Íslenska"),
        ("it", "Italiano"),
        ("he", "עברית"),
        ("kal", "Kalaallisut"),
        ("xal", "Хальмг Өөрдин келн"),
        ("kan", "ಕನ್ನಡ"),
        ("kat", "ქართული"),
        ("cor", "Kernewek"),
        ("khm", "ភាសាខ្មែរ"),
        ("qkz", "Khuzdul"),
        ("sw", "Kiswahili"),
        ("ht", "kreyòl ayisyen"),
        ("ku", "Kurdî | کوردی"),
        ("kir", "Кыргызча"),
        ("fcs", "Langue des signes québécoise"),
        ("lv", "Latviešu valoda"),
        ("lb", "Lëtzebuergesch"),
        ("lt", "Lietuvių kalba"),
        ("la", "Lingua latina"),
        ("hu", "Magyar"),
        ("mk", "македонски"),
        ("ml", "മലയാളം"),
        ("mt", "Malti"),
        ("mnc", "ᠮᠠᠨᠵᡠ ᡤᡳᠰᡠᠨ"),
        ("qmd", "Mando'a"),
        ("mr", "मराठी"),
        ("mik", "Mikisúkî"),
        ("mon", "ᠮᠣᠩᠭᠣᠯ ᠪᠢᠴᠢᠭ᠌ | Монгол Кирилл үсэг"),
        ("my", "မြန်မာဘာသာ"),
        ("myv", "Эрзянь кель"),
        ("nah", "Nāhuatl"),
        ("nan", "中文-闽南话 臺語"),
        ("ppl", "Nawat"),
        ("nl", "Nederlands"),
        ("ja", "日本語"),
        ("no", "Norsk"),
        ("ce", "Нохчийн мотт"),
        ("ood", "O’odham Ñiok"),
        ("ota", "لسان عثمانى"),
        ("ps", "پښتو"),
        ("nds", "Plattdüütsch"),
        ("pl", "Polski"),
        ("ptBR", "Português brasileiro"),
        ("ptPT", "Português europeu"),
        ("fuc", "Pulaar"),
        ("pa", "ਪੰਜਾਬੀ"),
        ("kaz", "qazaqşa | қазақша"),
        ("qlq", "Uncategorized Constructed Languages"),
        ("qya", "Quenya"),
        ("ro", "Română"),
        ("rom", "RRomani Ćhib"),
        ("ru", "Русский"),
        ("smi", "Sámi"),
        ("sah", "саха тыла"),
        ("sco", "Scots"),
        ("sq", "Shqip"),
        ("sjn", "Sindarin"),
        ("si", "සිංහල"),
        ("sk", "Slovenčina"),
        ("slv", "Slovenščina"),
        ("sla", "Slověnьskъ Językъ"),
        ("gem", "Sprēkō Þiudiskō"),
        ("sr", "Српски"),
        ("fi", "suomi"),
        ("sv", "Svenska"),
        ("ta", "தமிழ்"),
        ("tat", "татар теле"),
        ("mri", "te reo Māori"),
        ("tel", "తెలుగు"),
        ("tir", "ትግርኛ"),
        ("th", "ไทย"),
        ("tqx", "Thermian"),
        ("bod", "བོད་སྐད་"),
        ("vi", "Tiếng Việt"),
        ("cop", "ϯⲙⲉⲧⲣⲉⲙⲛ̀ⲭⲏⲙⲓ"),
        ("tlh", "tlhIngan-Hol"),
        ("tok", "toki pona"),
        ("trf", "Trinidadian Creole"),
        ("tsd", "τσακώνικα"),
        ("chr", "ᏣᎳᎩ ᎦᏬᏂᎯᏍᏗ"),
        ("tr", "Türkçe"),
        ("uk", "Українська"),
        ("ale", "Unangam Tunuu"),
        ("urd", "اُردُو"),
        ("uig", "ئۇيغۇر تىلى"),
        ("vol", "Volapük"),
        ("wuu", "中文-吴语"),
        ("yi", "יידיש"),
        ("yua", "maayaʼ tʼàan"),
        ("yue", "中文-广东话 粵語"),
        ("zh", "中文-普通话 國語"),
    ])
});
static LANGUAGES_REVERSED: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| LANGUAGES.iter().map(|(k, v)| (*v, *k)).collect());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_returns_code() {
        assert_eq!(Language::name_to_iso("English"), Some("en"));
        assert_eq!(Language::name_to_iso("Deutsch"), Some("de"));
        assert_eq!(Language::name_to_iso("en"), None);
        assert_eq!(Language::name_to_iso("de"), None);
        assert_eq!(Language::name_to_iso("Not A Real Language"), None);
        assert_eq!(Language::name_to_iso(""), None);
    }

    #[test]
    fn code_returns_language() {
        assert_eq!(Language::iso_to_name("en"), Some("English"));
        assert_eq!(Language::iso_to_name("de"), Some("Deutsch"));
        assert_eq!(Language::iso_to_name("English"), None);
        assert_eq!(Language::iso_to_name("Deutsch"), None);
        assert_eq!(Language::iso_to_name("Not A Real ISO"), None);
        assert_eq!(Language::iso_to_name(""), None);
    }
}
