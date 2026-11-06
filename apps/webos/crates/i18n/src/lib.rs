//! The app's words in the language shown (docs/LG.md, "Language").
//!
//! The words are in catalogs, `locales/<language>/<area>.json`, in the format of the server's,
//! the web app's and the Samsung apps' (i18next's): nested keys, read as `area.key.subkey`;
//! `{{name}}` for what goes in, `{{name, number}}` for a number grouped by thousands; and `_one`,
//! `_many` and `_other` after a key whose words depend on a count, chosen by CLDR's rules. The
//! catalogs are built into the binary and a language's are read the first time it shows.
//!
//! [`t!`] looks words up, checked as the code builds: `t!("signin.servers.title")`,
//! `t!("signin.servers.forgetTitle", name = row.name)`, `t!("library.movies", count = n)`. A
//! word a language lacks is English's.
//!
//! The language shown is the app's ([`set_language`] on the interface's thread, which
//! [`share_from_this_thread`] names), read from any thread. A thread that sets one without being
//! the interface's, such as a test's, shows it on that thread alone.

// t! names this crate by its path, which then works inside it too.
extern crate self as pley_i18n;

mod languages;
mod numbers;
#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread::ThreadId;

pub use languages::{Language, of_device, of_system, pick, supported};
pub use numbers::{decimal, number};
pub use pley_i18n_macros::t;

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

/// The language shown for every thread that hasn't set one of its own.
static SHOWN: AtomicU8 = AtomicU8::new(0);
/// The interface's thread: its [`set_language`] is the app's.
static UI_THREAD: OnceLock<ThreadId> = OnceLock::new();

thread_local! {
    static OWN: Cell<Option<Language>> = const { Cell::new(None) };
}

/// The language shown: this thread's own when it set one, else the app's.
pub fn language() -> Language {
    OWN.with(Cell::get).unwrap_or_else(|| Language::from_index(SHOWN.load(Ordering::Relaxed)))
}

/// Shows `language` from now on: on this thread, and on every thread when this is the interface's
/// ([`share_from_this_thread`]).
pub fn set_language(language: Language) {
    OWN.with(|own| own.set(Some(language)));
    if UI_THREAD.get() == Some(&std::thread::current().id()) {
        SHOWN.store(language.index(), Ordering::Relaxed);
    }
}

/// Makes this thread the interface's: the language it sets is the app's. The app calls it once,
/// from its main loop, before the first frame.
pub fn share_from_this_thread() {
    let _ = UI_THREAD.set(std::thread::current().id());
    SHOWN.store(language().index(), Ordering::Relaxed);
}

/// `language`'s words by their whole key, read from its catalogs the first time.
fn words(language: Language) -> &'static HashMap<String, String> {
    static WORDS: [OnceLock<HashMap<String, String>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    WORDS[usize::from(language.index())].get_or_init(|| read(language))
}

fn read(language: Language) -> HashMap<String, String> {
    let mut words = HashMap::new();
    for (code, area, json) in CATALOGS {
        if *code != language.code() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(json) {
            Ok(value) => flatten(area, &value, &mut words),
            // The tests read every catalog: one that isn't JSON never ships.
            Err(e) => log_bad(code, area, &e),
        }
    }
    words
}

#[cold]
fn log_bad(code: &str, area: &str, e: &serde_json::Error) {
    eprintln!("locales/{code}/{area}.json isn't JSON: {e}");
}

/// `{"a": {"b": "x"}}` under `area` → `area.a.b` = "x".
pub(crate) fn flatten(prefix: &str, value: &serde_json::Value, into: &mut HashMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                flatten(&format!("{prefix}.{k}"), v, into);
            }
        }
        serde_json::Value::String(s) => {
            into.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

/// `language`'s words for `key`, without English's.
fn own(language: Language, key: &str) -> Option<&'static str> {
    words(language).get(key).map(String::as_str)
}

/// The words for `key` in the language shown, else in English, else the key itself. [`t!`] calls
/// it; a key made while the app runs (a language's name by its code) goes through [`lookup`].
pub fn text(key: &str) -> &'static str {
    lookup(key).unwrap_or_else(|| {
        // Only a key t! didn't check can be missing.
        Box::leak(key.to_string().into_boxed_str())
    })
}

/// The words for a key made while the app runs, in the language shown, else in English; none when
/// English has none either.
pub fn lookup(key: &str) -> Option<&'static str> {
    let shown = language();
    own(shown, key).or_else(|| if shown == Language::En { None } else { own(Language::En, key) })
}

/// `text` with each `{{name}}` filled in from `values`; `{{name, number}}` groups a number by
/// thousands as the language shown does. A name without a value stays as it is.
pub fn fill(text: &str, values: &[(&str, &dyn Display)]) -> String {
    if values.is_empty() || !text.contains("{{") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let Some(close) = rest[open + 2..].find("}}") else { break };
        let inside = &rest[open + 2..open + 2 + close];
        let (name, format) = match inside.split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),
            None => (inside.trim(), None),
        };
        out.push_str(&rest[..open]);
        match values.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => {
                let written = value.to_string();
                match (format, written.parse::<i64>()) {
                    (Some("number"), Ok(n)) => out.push_str(&number(n)),
                    _ => out.push_str(&written),
                }
            }
            None => out.push_str(&rest[open..open + 2 + close + 2]),
        }
        rest = &rest[open + 2 + close + 2..];
    }
    out.push_str(rest);
    out
}

/// The words for `count` of something, in the language shown: `key`'s plural form for it
/// (`key_one`, `key_many`, `key_other`), else its `_other`, else English's, with `{{count}}` and
/// `values` filled in.
pub fn count(key: &str, count: i64, values: &[(&str, &dyn Display)]) -> String {
    let shown = language();
    let text = own(shown, &format!("{key}_{}", plural(shown, count)))
        .or_else(|| own(shown, &format!("{key}_other")))
        .or_else(|| own(Language::En, &format!("{key}_{}", plural(Language::En, count))))
        .or_else(|| own(Language::En, &format!("{key}_other")))
        .unwrap_or(key);
    let mut all: Vec<(&str, &dyn Display)> = Vec::with_capacity(values.len() + 1);
    all.push(("count", &count));
    all.extend_from_slice(values);
    fill(text, &all)
}

/// The plural form `count` takes in `language`, as CLDR has it: "one" for 1 (in French 0 and 1
/// too), "many" in French and Spanish for a whole number of millions, "other" for the rest.
pub fn plural(language: Language, count: i64) -> &'static str {
    let n = count.unsigned_abs();
    let millions = n != 0 && n.is_multiple_of(1_000_000);
    match language {
        Language::Fr if n <= 1 => "one",
        Language::Es | Language::En if n == 1 => "one",
        Language::Fr | Language::Es if millions => "many",
        _ => "other",
    }
}

/// The plural forms a counted key has in `language`: English "one" and "other", French and
/// Spanish "many" too.
pub fn plural_forms(language: Language) -> &'static [&'static str] {
    match language {
        Language::En => &["one", "other"],
        Language::Fr | Language::Es => &["one", "many", "other"],
    }
}

/// What [`t!`] refuses as the code builds: a key English hasn't,
///
/// ```compile_fail
/// let _ = pley_i18n::t!("settings.language.nothing");
/// ```
///
/// a value its words don't hold,
///
/// ```compile_fail
/// let _ = pley_i18n::t!("settings.language.label", name = "x");
/// ```
///
/// and words without the value they hold.
///
/// ```compile_fail
/// let _ = pley_i18n::t!("settings.language.automatic");
/// ```
///
/// ```
/// let _ = pley_i18n::t!("settings.language.automatic", language = "Español");
/// ```
#[doc(hidden)]
pub mod refused {}

/// A count as [`count`] takes it, from any integer [`t!`] is given.
#[doc(hidden)]
pub fn __n<T: TryInto<i64>>(n: T) -> i64 {
    n.try_into().unwrap_or(i64::MAX)
}

/// `language`'s words by their whole key, every area's: for tests that hold the catalogs to
/// English's.
pub fn catalog(language: Language) -> &'static HashMap<String, String> {
    words(language)
}

/// Runs `f` with `language` shown on this thread, then puts back what was: for tests.
pub fn with_language<T>(language: Language, f: impl FnOnce() -> T) -> T {
    let before = OWN.with(|own| own.replace(Some(language)));
    let out = f();
    OWN.with(|own| own.set(before));
    out
}
