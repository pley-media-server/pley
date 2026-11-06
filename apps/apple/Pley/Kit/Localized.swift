import Foundation
import PleyModel

/// `key` in the language shown, from this app's catalog
/// (`Localizable.xcstrings`; docs/APPLE.md, "Languages"): for words made
/// in code. SwiftUI's own lookups (`Text("Servers")`) need nothing.
func localized(_ key: String.LocalizationValue) -> String {
    ShownLanguage.string(key, bundle: .main)
}

/// `localized`, for an English string that needs two translations: a key
/// of its own, its English the default value:
/// `localized(keyed: LocalizedStringResource("audio.automatic", defaultValue: "Automatic (\(name))"))`.
func localized(keyed resource: LocalizedStringResource) -> String {
    ShownLanguage.string(keyed: resource)
}
