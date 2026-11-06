import Foundation

/// `key` in the language shown, from this module's catalog
/// (`Localizable.xcstrings`; docs/APPLE.md, "Languages"): for words made
/// in code. Each module with words has its own, naming its own bundle.
func localized(_ key: String.LocalizationValue) -> String {
    ShownLanguage.string(key, bundle: .module)
}

/// `localized`, for an English string that needs two translations: a key
/// of its own, its English the default value, in this module's bundle:
/// `localized(keyed: LocalizedStringResource("audio.automatic", defaultValue: "Automatic (\(name))",
/// bundle: .atURL(Bundle.module.bundleURL)))`.
func localized(keyed resource: LocalizedStringResource) -> String {
    ShownLanguage.string(keyed: resource)
}
