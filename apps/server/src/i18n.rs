//! The words Pley's server writes for people (Home's rows, the tray), in
//! the languages its apps are in.
//!
//! The catalogs are `locales/<language>.json`, in the format of the web
//! app's (i18next's): nested keys, `{{name}}` for what goes in, and
//! `_one`/`_other` after a key whose words depend on a count. They are built
//! into the binary. A word missing from one falls back to English.
//!
//! The language a request is answered in is the account's own choice, else
//! the first one the device asks for (`Accept-Language`) that Pley has,
//! else the server language (Settings → General), which is English until an
//! administrator picks another.

use std::collections::HashMap;
use std::sync::LazyLock;

use axum::http::{HeaderMap, header};

use crate::db::Db;
use crate::error::AppError;

/// The languages Pley is in, as ISO 639-1 codes; English first, the fallback.
pub const LANGUAGES: [&str; 3] = ["en", "fr", "es"];

type Catalog = HashMap<String, String>;

static CATALOGS: LazyLock<HashMap<&'static str, Catalog>> = LazyLock::new(|| {
    let sources = [
        ("en", include_str!("../locales/en.json")),
        ("fr", include_str!("../locales/fr.json")),
        ("es", include_str!("../locales/es.json")),
    ];
    sources
        .into_iter()
        .map(|(language, json)| {
            let value: serde_json::Value = serde_json::from_str(json).expect("a catalog is valid JSON");
            let mut catalog = Catalog::new();
            flatten("", &value, &mut catalog);
            (language, catalog)
        })
        .collect()
});

/// `{"a": {"b": "x"}}` → `"a.b" = "x"`.
fn flatten(prefix: &str, value: &serde_json::Value, into: &mut Catalog) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let key = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                flatten(&key, value, into);
            }
        }
        serde_json::Value::String(text) => {
            into.insert(prefix.to_owned(), text.clone());
        }
        _ => {}
    }
}

/// The language Pley has for `code` ("fr-CA" → "fr"), if any.
pub fn supported(code: &str) -> Option<&'static str> {
    let base = code.trim().split(['-', '_']).next()?.to_ascii_lowercase();
    LANGUAGES.into_iter().find(|language| *language == base)
}

/// The first language of an `Accept-Language` header, by weight, that Pley has.
pub fn from_accept_language(value: &str) -> Option<&'static str> {
    let mut asked: Vec<(f32, usize, &str)> = value
        .split(',')
        .take(32)
        .enumerate()
        .filter_map(|(i, part)| {
            let mut pieces = part.split(';');
            let tag = pieces.next()?.trim();
            let weight =
                pieces.find_map(|p| p.trim().strip_prefix("q=")).map_or(Some(1.0), |q| q.trim().parse::<f32>().ok())?;
            (weight > 0.0).then_some((weight, i, tag))
        })
        .collect();
    // Heaviest first; the order given among equals.
    asked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    asked.into_iter().find_map(|(_, _, tag)| supported(tag))
}

/// The server language: the administrator's choice, English until there
/// is one.
pub fn server_language(setting: &str) -> &'static str {
    supported(setting).unwrap_or("en")
}

/// The language for a request whose account hasn't chosen one: the
/// device's (`Accept-Language`) where Pley has it, else the server language.
pub fn fallback(server: &str, headers: Option<&HeaderMap>) -> &'static str {
    headers
        .and_then(|headers| headers.get(header::ACCEPT_LANGUAGE)?.to_str().ok())
        .and_then(from_accept_language)
        .unwrap_or_else(|| server_language(server))
}

/// The language to answer `user_id`'s request in (see the module's doc).
pub async fn for_user(db: &Db, user_id: i64, server: &str, headers: &HeaderMap) -> Result<&'static str, AppError> {
    let own: Option<String> = sqlx::query_scalar("SELECT language FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&db.read)
        .await?
        .flatten();
    Ok(own.as_deref().and_then(supported).unwrap_or_else(|| fallback(server, Some(headers))))
}

/// The words for `key` in `language`, English's where it has none, or the
/// key itself where neither has it.
pub fn t(language: &str, key: &str) -> String {
    lookup(language, key).unwrap_or(key).to_owned()
}

/// [`t`], with each `{{name}}` replaced by its value.
pub fn t_with(language: &str, key: &str, values: &[(&str, &str)]) -> String {
    let mut text = t(language, key);
    for (name, value) in values {
        text = text.replace(&format!("{{{{{name}}}}}"), value);
    }
    text
}

/// [`t_with`] for words that depend on `count` (`{{count}}` in them):
/// `key_one` or `key_other`, as the language counts.
pub fn t_count(language: &str, key: &str, count: u64, values: &[(&str, &str)]) -> String {
    // French counts 0 as one ("0 lecture"); English and Spanish don't.
    let one = match language {
        "fr" => count <= 1,
        _ => count == 1,
    };
    let key = format!("{key}_{}", if one { "one" } else { "other" });
    let count = count.to_string();
    let mut all = vec![("count", count.as_str())];
    all.extend_from_slice(values);
    t_with(language, &key, &all)
}

fn lookup(language: &str, key: &str) -> Option<&'static str> {
    let catalogs = &*CATALOGS;
    catalogs
        .get(language)
        .and_then(|catalog| catalog.get(key))
        .or_else(|| catalogs.get("en")?.get(key))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_key_with_the_same_values_in_it() {
        let english = &CATALOGS["en"];
        let names = |text: &str| {
            let mut names: Vec<String> =
                text.split("{{").skip(1).filter_map(|rest| rest.split_once("}}")).map(|(n, _)| n.to_owned()).collect();
            names.sort();
            names
        };
        for language in LANGUAGES {
            let catalog = &CATALOGS[language];
            let mut missing: Vec<&String> = english.keys().filter(|key| !catalog.contains_key(*key)).collect();
            let mut extra: Vec<&String> = catalog.keys().filter(|key| !english.contains_key(*key)).collect();
            missing.sort();
            extra.sort();
            assert!(missing.is_empty(), "{language} lacks {missing:?}");
            assert!(extra.is_empty(), "{language} has keys English hasn't: {extra:?}");
            for (key, text) in english {
                assert_eq!(names(&catalog[key]), names(text), "{language}'s {key} takes other values");
            }
        }
    }

    #[test]
    fn languages_match_by_their_base() {
        assert_eq!(supported("fr-CA"), Some("fr"));
        assert_eq!(supported("ES_mx"), Some("es"));
        assert_eq!(supported("en"), Some("en"));
        assert_eq!(supported("de-DE"), None);
        assert_eq!(supported(""), None);
    }

    #[test]
    fn accept_language_goes_by_weight_then_order() {
        assert_eq!(from_accept_language("de-DE,de;q=0.9,fr;q=0.8,en;q=0.7"), Some("fr"));
        assert_eq!(from_accept_language("en;q=0.5, es"), Some("es"));
        assert_eq!(from_accept_language("es;q=0, de"), None);
        assert_eq!(from_accept_language("*"), None);
        assert_eq!(from_accept_language(""), None);
    }

    #[test]
    fn the_device_comes_before_the_server_language() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT_LANGUAGE, "es-ES,es;q=0.9".parse().unwrap());
        assert_eq!(fallback("fr", Some(&headers)), "es");
        let mut german = HeaderMap::new();
        german.insert(header::ACCEPT_LANGUAGE, "de-DE".parse().unwrap());
        assert_eq!(fallback("fr", Some(&german)), "fr");
        assert_eq!(fallback("fr", None), "fr");
        assert_eq!(fallback("", None), "en");
    }

    #[test]
    fn words_fill_in_and_count() {
        assert_eq!(t_with("fr", "hubs.recentlyAddedIn", &[("library", "Films")]), "Ajouts récents dans Films");
        assert_eq!(t_count("en", "tray.status.streams", 1, &[]), "1 stream");
        assert_eq!(t_count("en", "tray.status.streams", 0, &[]), "0 streams");
        assert_eq!(t_count("fr", "tray.status.streams", 0, &[]), "0 lecture");
        assert_eq!(t_count("es", "tray.status.streams", 3, &[]), "3 reproducciones");
        assert_eq!(t("de", "hubs.myList"), "My List");
        assert_eq!(t("en", "no.such.key"), "no.such.key");
    }
}
