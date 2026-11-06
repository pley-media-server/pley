import PleyModel
import PleyScreens
import SwiftUI

/// `content` in the language the app shows (docs/APPLE.md, "Languages"):
/// SwiftUI looks its words up in the environment's locale, and the views
/// are made again when the language changes, so the words made in code
/// change with them. Each scene's root has one (`RootView` does it itself,
/// leaving the player out).
struct InLanguage<Content: View>: View {
    @ObservedObject var app: AppModel
    @ViewBuilder let content: () -> Content

    var body: some View {
        content()
            .id(app.language)
            .environment(\.locale, app.locale)
    }
}

/// The globe of the screens before signing in: the language shown by its
/// two letters, small and out of the way (top left on iPhone and iPad, in
/// the toolbar on the Mac); its menu picks this device's language.
struct LanguageGlobe: View {
    @ObservedObject var app: AppModel

    var body: some View {
        Menu {
            Picker("Choose a language", selection: Binding(get: { app.pickedLanguage },
                                                           set: { app.pickLanguage($0) })) {
                ForEach(AppModel.languageChoices(automatic: app.deviceLanguage ?? .english)) { choice in
                    Text(verbatim: choice.label).tag(choice.value)
                }
            }
            .pickerStyle(.inline)
        } label: {
            Text(verbatim: app.language.short)
                .font(.footnote.weight(.semibold))
                .monospaced()
        }
        .menuIndicator(.hidden)
        .fixedSize()
        .accessibilityLabel("Choose a language")
        .accessibilityValue(Text(verbatim: app.language.autonym))
        .accessibilityIdentifier("language.globe")
    }
}

extension View {
    /// The globe in the toolbar, while no account is signed in: top left on
    /// iPhone and iPad, at the toolbar's end on the Mac.
    func languageGlobe(_ app: AppModel) -> some View {
        toolbar {
            if app.user == nil {
                #if os(iOS)
                ToolbarItem(placement: .navigationBarLeading) { LanguageGlobe(app: app) }
                #else
                ToolbarItem(placement: .primaryAction) { LanguageGlobe(app: app) }
                #endif
            }
        }
    }
}
