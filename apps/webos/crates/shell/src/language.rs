//! Which language the app shows as it learns what decides it (docs/LG.md, "Language"): the
//! account's, else the one picked on this TV with the globe, else the TV's own when Pley has it,
//! else the server's, else English ([`pley_i18n::pick`], the order the Samsung apps keep too).
//!
//! At a start the app shows the language it showed last, never another while it waits, which
//! would flash: signed in, it waits for the account; on a TV in a language Pley doesn't have, for
//! the server to say its own.

use pley_i18n::{Language, of_device, supported};

/// What the app knows of what decides the language.
#[derive(Clone, Copy, Debug, Default)]
pub struct Known<'a> {
    /// The TV holds a sign-in for the server in use.
    pub signed_in: bool,
    /// The account was heard in this launch, and its language (none: Automatic).
    pub account: Option<Option<&'a str>>,
    /// The language picked on this TV with the globe.
    pub choice: Option<Language>,
    /// The TV's languages.
    pub tv: &'a [String],
    /// A server is in use.
    pub server_in_use: bool,
    /// The server was heard, and its language.
    pub server: Option<Option<&'a str>>,
}

/// The language to show now; none while what decides it hasn't been heard yet.
pub fn wanted(k: &Known) -> Option<Language> {
    let account = match (k.signed_in, k.account) {
        (true, None) => return None,
        (true, Some(language)) => supported(language),
        (false, _) => None,
    };
    if let Some(language) = account.or(k.choice).or_else(|| of_device(k.tv)) {
        return Some(language);
    }
    match (k.server_in_use, k.server) {
        (false, _) => Some(Language::En),
        (true, Some(language)) => Some(supported(language).unwrap_or(Language::En)),
        (true, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tv(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn signed_in_it_waits_for_the_account() {
        let french = tv(&["fr-FR"]);
        let k = Known { signed_in: true, tv: &french, server_in_use: true, ..Known::default() };
        assert_eq!(wanted(&k), None);
        assert_eq!(wanted(&Known { account: Some(Some("es")), ..k }), Some(Language::Es));
        // Automatic: the TV's language.
        assert_eq!(wanted(&Known { account: Some(None), ..k }), Some(Language::Fr));
        // A choice on this TV comes before it; the account's own before that.
        let chosen = Known { account: Some(None), choice: Some(Language::Es), ..k };
        assert_eq!(wanted(&chosen), Some(Language::Es));
        assert_eq!(wanted(&Known { account: Some(Some("fr")), ..chosen }), Some(Language::Fr));
    }

    #[test]
    fn on_a_tv_in_another_language_it_waits_for_the_server() {
        let german = tv(&["de-DE"]);
        let k = Known { tv: &german, server_in_use: true, ..Known::default() };
        assert_eq!(wanted(&k), None);
        assert_eq!(wanted(&Known { server: Some(Some("es")), ..k }), Some(Language::Es));
        // A server without a language, or one in a language Pley hasn't: English.
        assert_eq!(wanted(&Known { server: Some(None), ..k }), Some(Language::En));
        assert_eq!(wanted(&Known { server: Some(Some("de")), ..k }), Some(Language::En));
        // No server chosen yet: nothing to wait for.
        assert_eq!(wanted(&Known { server_in_use: false, ..k }), Some(Language::En));
        // The TV's language needs no server.
        let canadian = tv(&["fr-CA"]);
        assert_eq!(wanted(&Known { tv: &canadian, ..k }), Some(Language::Fr));
    }
}
