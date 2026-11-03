//! Language tags as files and providers write them: ISO 639-2 (ffprobe),
//! ISO 639-1 (sidecar files, many providers) and BCP 47 (`pt-BR`).

/// `(ISO 639-2, ISO 639-1, English name)` for a language tag in any of
/// those forms (ffprobe uses 639-2, sidecar files often 639-1).
pub fn language(tag: &str) -> Option<(&'static str, &'static str, &'static str)> {
    const LANGUAGES: &[(&str, &str, &str, &str)] = &[
        ("eng", "en", "English", ""),
        ("fre", "fr", "French", "fra"),
        ("ger", "de", "German", "deu"),
        ("spa", "es", "Spanish", ""),
        ("ita", "it", "Italian", ""),
        ("jpn", "ja", "Japanese", ""),
        ("chi", "zh", "Chinese", "zho"),
        ("kor", "ko", "Korean", ""),
        ("rus", "ru", "Russian", ""),
        ("por", "pt", "Portuguese", ""),
        ("dut", "nl", "Dutch", "nld"),
        ("swe", "sv", "Swedish", ""),
        ("nor", "no", "Norwegian", "nob"),
        ("dan", "da", "Danish", ""),
        ("fin", "fi", "Finnish", ""),
        ("pol", "pl", "Polish", ""),
        ("tur", "tr", "Turkish", ""),
        ("ara", "ar", "Arabic", ""),
        ("heb", "he", "Hebrew", ""),
        ("hin", "hi", "Hindi", ""),
        ("tha", "th", "Thai", ""),
        ("vie", "vi", "Vietnamese", ""),
        ("cze", "cs", "Czech", "ces"),
        ("hun", "hu", "Hungarian", ""),
        ("gre", "el", "Greek", "ell"),
        ("rum", "ro", "Romanian", "ron"),
        ("ukr", "uk", "Ukrainian", ""),
        ("ind", "id", "Indonesian", ""),
        ("may", "ms", "Malay", "msa"),
        ("per", "fa", "Persian", "fas"),
        ("bul", "bg", "Bulgarian", ""),
        ("hrv", "hr", "Croatian", ""),
        ("srp", "sr", "Serbian", ""),
        ("slo", "sk", "Slovak", "slk"),
        ("slv", "sl", "Slovenian", ""),
        ("est", "et", "Estonian", ""),
        ("lav", "lv", "Latvian", ""),
        ("lit", "lt", "Lithuanian", ""),
        ("cat", "ca", "Catalan", ""),
        ("ice", "is", "Icelandic", "isl"),
        ("fil", "tl", "Filipino", "tgl"),
        ("tam", "ta", "Tamil", ""),
        ("tel", "te", "Telugu", ""),
        ("ben", "bn", "Bengali", ""),
        ("urd", "ur", "Urdu", ""),
    ];
    let tag = tag.trim().to_ascii_lowercase();
    let tag = tag.split(['-', '_']).next().unwrap_or_default();
    LANGUAGES
        .iter()
        .find(|(b, one, name, t)| {
            tag == *b || tag == *one || (!t.is_empty() && tag == *t) || tag == name.to_ascii_lowercase()
        })
        .map(|(b, one, name, _)| (*b, *one, *name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages() {
        assert_eq!(language("eng"), Some(("eng", "en", "English")));
        assert_eq!(language("fr"), Some(("fre", "fr", "French")));
        assert_eq!(language("deu"), Some(("ger", "de", "German")));
        assert_eq!(language("pt-BR"), Some(("por", "pt", "Portuguese")));
        assert_eq!(language("xx"), None);
    }
}
