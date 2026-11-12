//! The system's notifications in the languages Pley is in. Their words are
//! the `notices` of the picker's catalogs (`src/locales/<language>.json`),
//! built in, so that each language's words are in one place, which
//! `src/i18n.test.ts` checks against English. They are the web app's words
//! for the same news (`requests` in its catalogs).

use std::sync::OnceLock;

use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Fr,
    Es,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
            Self::Es => "es",
        }
    }

    /// The language Pley has for a tag ("fr-CA" → French), if any.
    pub fn of(tag: &str) -> Option<Self> {
        let base = tag.trim().split(['-', '_']).next()?.to_ascii_lowercase();
        match base.as_str() {
            "en" => Some(Self::En),
            "fr" => Some(Self::Fr),
            "es" => Some(Self::Es),
            _ => None,
        }
    }

    /// The first of the computer's languages that Pley has.
    pub fn of_device(tags: &[String]) -> Option<Self> {
        tags.iter().find_map(|tag| Self::of(tag))
    }

    /// The language an account sees, as the web app picks it (docs/API.md
    /// "Language"): its own, else the computer's, else the server's, else
    /// English.
    pub fn pick(account: Option<&str>, device: &[String], server: Option<&str>) -> Self {
        account
            .and_then(Self::of)
            .or_else(|| Self::of_device(device))
            .or_else(|| server.and_then(Self::of))
            .unwrap_or(Self::En)
    }

    fn notices(self) -> &'static Map<String, Value> {
        static CATALOGS: OnceLock<[Map<String, Value>; 3]> = OnceLock::new();
        let catalogs = CATALOGS.get_or_init(|| {
            let notices = |json: &str| match serde_json::from_str::<Value>(json) {
                Ok(Value::Object(mut catalog)) => match catalog.remove("notices") {
                    Some(Value::Object(notices)) => notices,
                    _ => Map::new(),
                },
                _ => Map::new(),
            };
            [
                notices(include_str!("../../src/locales/en.json")),
                notices(include_str!("../../src/locales/fr.json")),
                notices(include_str!("../../src/locales/es.json")),
            ]
        });
        &catalogs[self as usize]
    }

    /// The words of `key`, English's when this language lacks them.
    fn words(self, key: &str) -> &'static str {
        [self, Self::En].into_iter().find_map(|language| language.notices().get(key)?.as_str()).unwrap_or_default()
    }

    /// `key` with its `{{values}}` filled in.
    pub fn say(self, key: &str, values: &[(&str, &str)]) -> String {
        fill(self.words(key), values)
    }

    /// `key` counted `count` times: its form for one, many or others, as
    /// i18next picks it in the language (`_one`, `_many`, `_other`), with
    /// `count` among its values.
    pub fn say_counted(self, key: &str, count: usize, values: &[(&str, &str)]) -> String {
        let count_text = count.to_string();
        let mut all = vec![("count", count_text.as_str())];
        all.extend_from_slice(values);
        let form = format!("{key}_{}", self.plural(count));
        let words =
            self.notices().get(&form).and_then(Value::as_str).unwrap_or_else(|| self.words(&format!("{key}_other")));
        fill(words, &all)
    }

    /// The plural form of `count`, as `Intl.PluralRules` says it for whole
    /// numbers: French counts 0 as one, and French and Spanish have a form
    /// for millions.
    fn plural(self, count: usize) -> &'static str {
        match self {
            Self::En => {
                if count == 1 {
                    "one"
                } else {
                    "other"
                }
            }
            Self::Fr | Self::Es => {
                if count == 1 || (self == Self::Fr && count == 0) {
                    "one"
                } else if count != 0 && count.is_multiple_of(1_000_000) {
                    "many"
                } else {
                    "other"
                }
            }
        }
    }
}

/// `words` with each `{{name}}` replaced by its value, in one pass: a value
/// with braces in it (a title) stays as it is.
fn fill(words: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(words.len());
    let mut rest = words;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after[..end].trim();
        match values.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_is_the_accounts_else_the_computers_else_the_servers() {
        let device = |tags: &[&str]| tags.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
        assert_eq!(Language::pick(Some("fr"), &device(&["es-MX"]), Some("en")), Language::Fr);
        assert_eq!(Language::pick(None, &device(&["de-DE", "es-MX"]), Some("fr")), Language::Es);
        assert_eq!(Language::pick(Some("de"), &device(&["de-DE"]), Some("fr")), Language::Fr);
        assert_eq!(Language::pick(None, &device(&["de-DE"]), None), Language::En);
        assert_eq!(Language::of("FR_ca"), Some(Language::Fr));
        assert_eq!(Language::of(""), None);
    }

    #[test]
    fn each_language_has_its_words_and_english_fills_in() {
        for language in [Language::En, Language::Fr, Language::Es] {
            for key in ["quoted", "andSpecials", "specials", "ready", "specialsReady", "approved", "declined"] {
                assert!(!language.notices().get(key).and_then(Value::as_str).unwrap_or_default().is_empty(), "{key}");
            }
        }
        assert_eq!(Language::Fr.say("ready", &[("title", "Heat")]), "Heat est prêt à regarder.");
        assert_eq!(Language::Es.say("missing", &[]), "", "nothing for a key no language has");
    }

    #[test]
    fn counts_take_the_languages_forms() {
        assert_eq!(Language::En.say_counted("news", 3, &[]), "3 of your requests have news.");
        assert_eq!(Language::En.say_counted("news", 1, &[]), "1 of your requests has news.");
        assert_eq!(Language::Fr.say_counted("news", 0, &[]), "0 de vos demandes a du nouveau.");
        assert_eq!(Language::Es.say_counted("news", 2, &[]), "2 de tus solicitudes tienen novedades.");
        assert_eq!(Language::Fr.plural(2_000_000), "many");
        assert_eq!(Language::En.plural(2_000_000), "other");
    }

    #[test]
    fn values_are_filled_in_once() {
        assert_eq!(fill("{{a}} and {{ b }}", &[("a", "{{b}}"), ("b", "x")]), "{{b}} and x");
        assert_eq!(fill("{{unknown}} {{", &[]), "{{unknown}} {{");
    }
}
