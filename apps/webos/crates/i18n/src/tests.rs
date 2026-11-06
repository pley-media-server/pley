use std::collections::BTreeSet;

use serde_json::Value;

use crate::numbers::{group, write_decimal};
use crate::{CATALOGS, Language, catalog, count, fill, of_system, pick, plural, plural_forms, t, with_language};

/// The order the TV apps choose the language in (packages/tv/test/fixtures/language.json), which
/// the Samsung apps' tests read too.
const FIXTURE: &str = include_str!("../../../../../packages/tv/test/fixtures/language.json");

fn text(v: &Value) -> Option<&str> {
    v.as_str()
}

fn device(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

#[test]
fn the_language_is_picked_in_the_shared_order() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let cases = fixture["pickLanguage"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let out = pick(text(&c["account"]), None, text(&c["server"]), &device(&c["device"]));
        assert_eq!(out.code(), c["out"].as_str().unwrap(), "{c}");
    }
    let cases = fixture["pickLanguageWithChoice"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let out = pick(text(&c["account"]), text(&c["choice"]), text(&c["server"]), &device(&c["device"]));
        assert_eq!(out.code(), c["out"].as_str().unwrap(), "{c}");
    }
}

#[test]
fn automatic_is_the_tvs_language_else_the_servers_else_english() {
    assert_eq!(of_system(Some("es"), &["fr-CA".into()]), Language::Fr);
    assert_eq!(of_system(Some("es"), &["de-DE".into()]), Language::Es);
    assert_eq!(of_system(None, &["de-DE".into()]), Language::En);
    assert_eq!(of_system(Some("pt"), &[]), Language::En);
}

/// Every catalog's words by their whole key.
fn words_of(language: &str) -> std::collections::HashMap<String, String> {
    let mut words = std::collections::HashMap::new();
    for (code, area, json) in CATALOGS {
        if *code == language {
            let value: Value = serde_json::from_str(json).unwrap_or_else(|e| panic!("locales/{code}/{area}.json: {e}"));
            assert!(value.is_object(), "locales/{code}/{area}.json is an object");
            crate::flatten(area, &value, &mut words);
        }
    }
    words
}

/// A key's base, and its plural form when it has one: `a.b_one` → (`a.b`, Some("one")).
fn split_plural(key: &str) -> (&str, Option<&str>) {
    for form in ["zero", "one", "two", "few", "many", "other"] {
        if let Some(base) = key.strip_suffix(&format!("_{form}")) {
            return (base, Some(form));
        }
    }
    (key, None)
}

/// The names of a text's `{{…}}`, sorted.
fn names(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let Some(close) = rest[open + 2..].find("}}") else { break };
        out.insert(rest[open + 2..open + 2 + close].split(',').next().unwrap().trim().to_string());
        rest = &rest[open + 2 + close + 2..];
    }
    out
}

#[test]
fn every_language_has_every_word_with_the_same_values() {
    let english = words_of("en");
    assert!(!english.is_empty());
    let areas = |code: &str| -> BTreeSet<&str> { CATALOGS.iter().filter(|c| c.0 == code).map(|c| c.1).collect() };
    for language in [Language::Fr, Language::Es] {
        let code = language.code();
        assert_eq!(areas(code), areas("en"), "{code} has the same areas as English");
        let words = words_of(code);
        // Counted keys: each language's own forms.
        let counted: BTreeSet<&str> =
            english.keys().filter_map(|k| split_plural(k).1.map(|_| split_plural(k).0)).collect();
        for base in &counted {
            let english_names: BTreeSet<String> =
                plural_forms(Language::En).iter().flat_map(|f| names(&english[&format!("{base}_{f}")])).collect();
            for form in plural_forms(language) {
                let key = format!("{base}_{form}");
                let text = words.get(&key).unwrap_or_else(|| panic!("{code} has no {key}"));
                let own = names(text);
                assert!(own.is_subset(&english_names), "{code} {key}: {own:?} against {english_names:?}");
                // "other" and "many" always write the number; "one" may say it in words.
                if *form != "one" {
                    assert_eq!(own, names(&english[&format!("{base}_other")]), "{code} {key}");
                }
            }
        }
        for (key, english_text) in &english {
            if split_plural(key).1.is_some() {
                continue;
            }
            let text = words.get(key).unwrap_or_else(|| panic!("{code} has no {key}"));
            assert_eq!(names(text), names(english_text), "{code} {key}");
        }
        for key in words.keys() {
            let base = split_plural(key).0;
            assert!(english.contains_key(key) || counted.contains(base), "{code} has {key}, which English hasn't");
        }
    }
}

#[test]
fn french_and_spanish_are_punctuated_as_they_write() {
    for language in ["fr", "es"] {
        for (key, text) in &words_of(language) {
            assert!(!text.contains('\''), "{language} {key} uses a straight apostrophe: {text}");
            assert!(!text.contains("..."), "{language} {key} writes three points rather than …: {text}");
        }
    }
    // French sets its double punctuation apart with a no-break space, never one a line could
    // break at.
    for (key, text) in &words_of("fr") {
        for mark in [" ?", " !", " :", " ;", "« ", " »"] {
            assert!(!text.contains(mark), "fr {key}: a breaking space in \"{mark}\": {text}");
        }
    }
}

#[test]
fn plural_forms_follow_cldr() {
    assert_eq!(plural(Language::En, 0), "other");
    assert_eq!(plural(Language::En, 1), "one");
    assert_eq!(plural(Language::En, 2), "other");
    assert_eq!(plural(Language::En, 1_000_000), "other");
    assert_eq!(plural(Language::Fr, 0), "one");
    assert_eq!(plural(Language::Fr, 1), "one");
    assert_eq!(plural(Language::Fr, 2), "other");
    assert_eq!(plural(Language::Fr, 1_000_000), "many");
    assert_eq!(plural(Language::Fr, 2_000_000), "many");
    assert_eq!(plural(Language::Fr, 1_000_001), "other");
    assert_eq!(plural(Language::Es, 0), "other");
    assert_eq!(plural(Language::Es, 1), "one");
    assert_eq!(plural(Language::Es, 3_000_000), "many");
}

#[test]
fn numbers_are_grouped_as_each_language_writes_them() {
    assert_eq!(group(Language::En, 12345), "12,345");
    assert_eq!(group(Language::En, 1234), "1,234");
    assert_eq!(group(Language::En, -1234567), "-1,234,567");
    assert_eq!(group(Language::En, 999), "999");
    assert_eq!(group(Language::Fr, 1234), "1\u{A0}234");
    assert_eq!(group(Language::Es, 1234), "1234");
    assert_eq!(group(Language::Es, 12345), "12.345");
    assert_eq!(write_decimal(Language::En, 1.25, 1), "1.3");
    assert_eq!(write_decimal(Language::Fr, 1.5, 1), "1,5");
    assert_eq!(write_decimal(Language::Es, 12345.5, 1), "12.345,5");
    assert_eq!(write_decimal(Language::En, -0.04, 1), "0.0");
    assert_eq!(write_decimal(Language::En, 3.0, 0), "3");
}

#[test]
fn words_fill_in_their_values_and_counts() {
    assert_eq!(fill("Forget {{name}}?", &[("name", &"Den")]), "Forget Den?");
    assert_eq!(fill("{{a}} and {{b}}", &[("a", &1)]), "1 and {{b}}");
    assert_eq!(fill("{{n, number}} songs", &[("n", &12345)]), "12,345 songs");
    with_language(Language::Fr, || assert_eq!(fill("{{n, number}}", &[("n", &12345)]), "12\u{A0}345"));
    // A counted key the catalogs have, through t!.
    let language = t!("settings.language.automatic", language = "Français");
    assert_eq!(language, "Automatic (Français)");
    with_language(Language::Fr, || {
        assert_eq!(t!("settings.language.automatic", language = "Français"), "Automatique (Français)");
        assert_eq!(t!("settings.language.label"), "Langue");
    });
    assert_eq!(count("nothing.here", 2, &[]), "nothing.here");
}

#[test]
fn a_thread_shows_its_own_language_until_the_interfaces_is_shared() {
    // Not the interface's thread: setting a language shows it here alone.
    let other = std::thread::spawn(|| {
        crate::set_language(Language::Es);
        crate::language()
    })
    .join()
    .unwrap();
    assert_eq!(other, Language::Es);
    assert_eq!(crate::language(), Language::En);
    assert_eq!(catalog(Language::En).get("settings.language.label").map(String::as_str), Some("Language"));
}
