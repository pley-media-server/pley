//! The languages Pley is in, and which one the app shows (docs/API.md, "Language"): the account's
//! own, else the one picked on this TV with the globe, else the TV's own when Pley has it, else
//! the server's, else English. The Samsung apps pick the same
//! (packages/tv/test/fixtures/language.json).

/// A language Pley is in; English by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    #[default]
    En,
    Fr,
    Es,
}

impl Language {
    /// Every language, English first: the fallback.
    pub const ALL: [Language; 3] = [Language::En, Language::Fr, Language::Es];

    /// Its ISO 639-1 code, as the server and `Accept-Language` write it: "fr".
    pub const fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Fr => "fr",
            Language::Es => "es",
        }
    }

    /// Its name in itself, as the language lists show it: "Français".
    pub const fn name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Fr => "Français",
            Language::Es => "Español",
        }
    }

    /// Its two letters as the globe shows them: "FR".
    pub const fn letters(self) -> &'static str {
        match self {
            Language::En => "EN",
            Language::Fr => "FR",
            Language::Es => "ES",
        }
    }

    pub(crate) const fn index(self) -> u8 {
        self as u8
    }

    pub(crate) const fn from_index(i: u8) -> Language {
        match i {
            1 => Language::Fr,
            2 => Language::Es,
            _ => Language::En,
        }
    }
}

/// The language Pley has for a tag ("fr-CA", "fr_FR", "FR" → French); none for one it hasn't, or
/// none given.
pub fn supported(tag: Option<&str>) -> Option<Language> {
    let code = tag?.trim().split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
    Language::ALL.into_iter().find(|l| l.code() == code)
}

/// The first of the TV's languages Pley has; none for none.
pub fn of_device(device: &[String]) -> Option<Language> {
    device.iter().find_map(|tag| supported(Some(tag)))
}

/// The first of the TV's languages Pley has, else the server's, else English: what the globe's
/// Automatic is.
pub fn of_system(server: Option<&str>, device: &[String]) -> Language {
    of_device(device).or_else(|| supported(server)).unwrap_or(Language::En)
}

/// The language to show: the account's, else the one picked on this TV (`choice`), else the first
/// of the TV's languages Pley has, else the server's, else English.
pub fn pick(account: Option<&str>, choice: Option<&str>, server: Option<&str>, device: &[String]) -> Language {
    supported(account).or_else(|| supported(choice)).unwrap_or_else(|| of_system(server, device))
}
