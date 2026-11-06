import Foundation
import os

/// The languages Pley is in (docs/API.md, "Language"), English first, the
/// fallback. The raw value is the ISO 639-1 code the server takes and
/// answers.
public enum Language: String, CaseIterable, Codable, Hashable, Sendable, Identifiable {
    case english = "en"
    case french = "fr"
    case spanish = "es"

    public var id: String { rawValue }

    /// "en", "fr", "es".
    public var code: String { rawValue }

    /// The language's name in itself, the same in every language: "Français".
    public var autonym: String {
        switch self {
        case .english: "English"
        case .french: "Français"
        case .spanish: "Español"
        }
    }

    /// The two letters the globe shows: "EN", "FR", "ES".
    public var short: String { rawValue.uppercased() }

    /// How a choice of the Language row and the globe reads: a language by
    /// its own name ("Français"), Automatic (`nil`) by the language it
    /// stands for here, in itself: "Automatic (Français)".
    public static func label(_ choice: Language?, automatic: Language) -> String {
        choice?.autonym ?? localized("Automatic (\(automatic.autonym))")
    }

    /// The language Pley has for `tag` ("fr", "fr-CA", "es_ES", " FR_ca"), `nil` for any other.
    public static func of(_ tag: String?) -> Language? {
        guard let base = tag?.trimmingCharacters(in: .whitespaces).split(whereSeparator: { $0 == "-" || $0 == "_" })
            .first?.lowercased()
        else { return nil }
        return Language(rawValue: base)
    }

    /// The first of the device's languages (`device`, the one it prefers first) that Pley has.
    public static func ofDevice(_ device: [String]) -> Language? {
        device.lazy.compactMap(of).first
    }

    /// The language to show: the account's own, else this device's (the
    /// one `picked` on it before signing in, else the first of the
    /// system's that Pley has), else the server language, else English. A
    /// code Pley doesn't have counts as none. The Android app's
    /// `Language.pick`, tested against the same fixture.
    public static func pick(account: String?, server: String?, device: [String], picked: Language? = nil) -> Language {
        of(account) ?? picked ?? ofDevice(device) ?? of(server) ?? .english
    }

    /// The locale `language` is shown in: the device's own where the
    /// device is in that language, so its region's ways stay (en-GB's
    /// 24-hour clock, fr-CA's dates), else the language alone.
    public static func locale(of language: Language, device: [String]) -> Locale {
        if let tag = device.first(where: { of($0) == language }) { return Locale(identifier: tag) }
        return Locale(identifier: language.code)
    }
}

/// The language the app is shown in, and the locale it's shown in
/// (docs/APPLE.md, "Languages"). One for the whole process, read from any
/// thread: strings made in code (`localized`), the `Accept-Language` of
/// every request, and dates and numbers (`ShownLanguage.locale`).
///
/// `AppModel` decides it and sets it (`show`); SwiftUI's own lookups
/// follow the locale the apps put in the environment at their root.
public enum ShownLanguage {
    private struct State {
        var language: Language
        var locale: Locale
        /// Each catalog's bundle in one language: `<bundle path>|<code>` → its `.lproj`.
        var bundles: [String: Bundle] = [:]
    }

    private static let state = OSAllocatedUnfairLock(initialState: State(language: .english,
                                                                         locale: Locale(identifier: "en")))

    /// The language shown.
    public static var language: Language { state.withLock { $0.language } }

    /// The locale shown: the one dates, times and numbers are written in.
    public static var locale: Locale { state.withLock { $0.locale } }

    /// The device's languages, the one it prefers first.
    public static var deviceLanguages: [String] { Locale.preferredLanguages }

    /// Shows `language` from now on, its dates and numbers written in
    /// `locale` (`Language.locale(of:device:)`); whether that changed anything.
    @discardableResult
    public static func show(_ language: Language, locale: Locale) -> Bool {
        state.withLock { state in
            guard state.language != language || state.locale != locale else { return false }
            state.language = language
            state.locale = locale
            return true
        }
    }

    /// `key` from `bundle`'s `Localizable` catalog in the language shown,
    /// its values written in the locale shown. Each module calls it
    /// through its own `localized(_:)`, which names its bundle.
    public static func string(_ key: String.LocalizationValue, bundle: Bundle, table: String? = nil) -> String {
        let (language, locale) = state.withLock { ($0.language, $0.locale) }
        return string(key, bundle: bundle, table: table, language: language, locale: locale)
    }

    /// `string`, for an English string that needs two translations: a key
    /// of its own ("audio.automatic"), its English the default value, in
    /// the resource's bundle; found in the language shown, its values
    /// written in the locale shown.
    public static func string(keyed resource: LocalizedStringResource) -> String {
        var resource = resource
        resource.locale = locale
        return String(localized: resource)
    }

    /// `key` from `bundle` in `language`, whatever is shown: for the tests.
    package static func string(_ key: String.LocalizationValue, bundle: Bundle, table: String? = nil,
                               language: Language, locale: Locale) -> String {
        String(localized: key, table: table, bundle: self.bundle(bundle, language), locale: locale)
    }

    /// `bundle`'s `.lproj` for `language`: a lookup there finds that
    /// language's words whatever the system's language is. A bundle
    /// without it (English, whose words are the keys) is itself.
    private static func bundle(_ bundle: Bundle, _ language: Language) -> Bundle {
        let key = bundle.bundlePath + "|" + language.code
        if let found = state.withLock({ $0.bundles[key] }) { return found }
        let found = bundle.path(forResource: language.code, ofType: "lproj").flatMap(Bundle.init(path:)) ?? bundle
        state.withLock { $0.bundles[key] = found }
        return found
    }
}
