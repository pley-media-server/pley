import PleyModel
import PleyScreens
import SwiftUI

/// The globe of the screens before signing in: the language shown by its
/// two letters, small and out of the way (the bottom right of the servers,
/// the top right of the code screen); its menu picks this device's
/// language. Only while no account is signed in.
struct LanguageGlobe: View {
    @ObservedObject var app: AppModel

    var body: some View {
        if app.user == nil {
            Menu {
                Picker("Choose a language", selection: Binding(get: { app.pickedLanguage },
                                                               set: { app.pickLanguage($0) })) {
                    ForEach(AppModel.languageChoices(automatic: app.deviceLanguage ?? .english)) { choice in
                        Text(verbatim: choice.label).tag(choice.value)
                    }
                }
                .pickerStyle(.inline)
            } label: {
                Label {
                    Text(verbatim: app.language.short)
                } icon: {
                    Image(systemName: "globe")
                }
                .font(.callout.weight(.semibold))
            }
            .accessibilityLabel("Choose a language")
            .accessibilityValue(Text(verbatim: app.language.autonym))
            .accessibilityIdentifier("language.globe")
        }
    }
}
