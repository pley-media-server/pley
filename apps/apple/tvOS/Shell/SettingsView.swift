import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// Settings on Apple TV, which holds what Me holds on the other devices:
/// the profile signed in and its server, Switch profile, History, Servers
/// and Sign out, the profile's language and its audio and subtitle defaults; then this
/// device's settings (Playback, Subtitles, Browsing, Always start as),
/// Diagnostics and About. Each row opens its choices; a change is kept at
/// once.
struct SettingsView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var me: MeModel
    @StateObject private var settings: SettingsModel
    @State private var confirmSignOut = false

    init(app: AppModel) {
        self.app = app
        _me = StateObject(wrappedValue: MeModel(app: app))
        _settings = StateObject(wrappedValue: SettingsModel(app: app, offersCellular: false))
    }

    var body: some View {
        HStack(alignment: .top, spacing: 80) {
            VStack(spacing: 24) {
                if let user = app.user {
                    AvatarView(app: app, name: me.name, path: user.avatarUrl, color: user.avatarColor, size: 240)
                }
                Text(me.name)
                    .font(.title2.bold())
                    .accessibilityIdentifier("me.name")
                if let kind = me.kind {
                    Text(kind).foregroundStyle(.secondary)
                }
                VStack(spacing: 8) {
                    Text(me.serverName).font(.callout)
                    Text(me.serverAddress).font(.caption).foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("me.server")
                Text(MeModel.webNote)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            .frame(width: 480)
            ScrollView {
                VStack(alignment: .leading, spacing: 48) {
                    account
                    language
                    trackDefaults
                    playback
                    subtitles
                    browsing
                    more
                }
                // Room for a row to grow with the focus, inside the column,
                // which clips so nothing scrolls over the tabs.
                .padding(.vertical, 40)
                .padding(.horizontal, 24)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, Metrics.gutter)
        .task(id: app.user?.id) {
            await me.load()
            await settings.load()
        }
        .alert("Sign out of \(app.server?.name ?? localized("this server"))?", isPresented: $confirmSignOut) {
            Button("Sign out", role: .destructive) { Task { await me.signOut() } }
            Button("Cancel", role: .cancel) {}
        }
    }

    // Buttons of their own, in columns the focus moves down.

    private var account: some View {
        VStack(alignment: .leading, spacing: 24) {
            if me.canSwitch {
                Button("Switch profile") { me.switchProfile() }
                    .accessibilityIdentifier("me.switchProfile")
            }
            Button("History") { app.shell.open(.history) }
                .accessibilityIdentifier("me.history")
            Button("Servers") { me.showServers() }
                .accessibilityIdentifier("me.servers")
            Button("Sign out", role: .destructive) { confirmSignOut = true }
                .accessibilityIdentifier("me.signOut")
        }
        .focusSection()
    }

    /// The account's language: Automatic (each device's), or one of its own,
    /// for every device it signs in on, saved on the server at once; on a
    /// Pley from before languages, this device's, as the globe picks it.
    private var language: some View {
        let chosen = app.languageRowChoice
        let choices = AppModel.languageChoices(automatic: app.languageRowAutomatic)
        let note = app.keepsAccountLanguage
            ? localized("""
                What Pley is in for you, on every device you sign in on. \
                Automatic follows each device's language.
                """)
            : localized("What Pley is in on this device.")
        return SettingsGroup(title: localized("Language"), note: note) {
            SettingRow(title: localized("Language"),
                       value: choices.first { $0.value == chosen }?.label ?? "", id: "settings.language",
                       choices: choices, selected: chosen) { choice in Task { await app.setAccountLanguage(choice) } }
        }
    }

    private var trackDefaults: some View {
        SettingsGroup(title: localized("Audio and subtitles"),
                      note: me.editable ? MeModel.modeHint(me.trackDefaults.subtitleMode) : MeModel.readOnlyNote) {
            SettingRow(title: localized("Audio language"), value: me.audioLabel, id: "me.audioLanguage",
                       choices: [SettingChoice<String?>(nil, localized("The file's default"))]
                           + MeModel.languages.map { SettingChoice<String?>($0.code, $0.name) },
                       selected: me.trackDefaults.audioLanguage) { code in Task { await me.setAudioLanguage(code) } }
            SettingRow(title: localized("Subtitles"), value: MeModel.modeLabel(me.trackDefaults.subtitleMode),
                       id: "me.subtitleMode",
                       choices: MeModel.modes.map { SettingChoice($0, MeModel.modeLabel($0)) },
                       selected: me.trackDefaults.subtitleMode) { mode in Task { await me.setSubtitleMode(mode) } }
            if me.showsSubtitleLanguage {
                SettingRow(title: localized("Subtitle language"), value: me.subtitleLanguageLabel,
                           id: "me.subtitleLanguage",
                           choices: [SettingChoice<String?>(nil, localized("Same as the audio language"))]
                               + MeModel.languages.map { SettingChoice<String?>($0.code, $0.name) },
                           selected: me.trackDefaults.subtitleLanguage) { code in
                    Task { await me.setSubtitleLanguage(code) }
                }
            }
        }
        .disabled(!me.editable || me.saving)
    }

    private var playback: some View {
        SettingsGroup(title: localized("Playback"), note: nil) {
            SettingRow(title: localized("Quality"), value: SettingsModel.qualityLabel(settings.qualityOther),
                       id: "settings.quality", choices: SettingsModel.qualities, selected: settings.qualityOther) {
                settings.setQuality($0, cellular: false)
            }
            SettingRow(title: localized("Play the next episode automatically"),
                       value: settings.autoplayNext ? localized("On") : localized("Off"),
                       id: "settings.autoplay",
                       choices: [SettingChoice(true, localized("On")), SettingChoice(false, localized("Off"))],
                       selected: settings.autoplayNext) { settings.setAutoplayNext($0) }
        }
    }

    private var subtitles: some View {
        SettingsGroup(title: localized("Subtitles"), note: localized("""
                          System uses your caption style from the Apple TV's Settings → \
                          Accessibility → Subtitles and Captioning. Position moves text subtitles only.
                          """)) {
            SettingRow(title: localized("Size"), value: settings.subtitles.size.label, id: "settings.subtitleSize",
                       choices: SettingsModel.sizes, selected: settings.subtitles.size) { settings.setSubtitleSize($0) }
            SettingRow(title: localized("Background"), value: settings.subtitles.background.label,
                       id: "settings.subtitleBackground", choices: SettingsModel.backgrounds,
                       selected: settings.subtitles.background) { settings.setSubtitleBackground($0) }
            SettingRow(title: localized("Font"), value: settings.subtitles.font.label, id: "settings.subtitleFont",
                       choices: SettingsModel.fonts, selected: settings.subtitles.font) { settings.setSubtitleFont($0) }
            SettingRow(title: localized("Position"), value: settings.subtitles.raise.label,
                       id: "settings.subtitlePosition", choices: SettingsModel.raises,
                       selected: settings.subtitles.raise) {
                settings.setSubtitleRaise($0)
            }
        }
    }

    private var browsing: some View {
        SettingsGroup(title: localized("Browsing"), note: nil) {
            SettingRow(title: localized("Grid size"), value: settings.gridDensity.label, id: "settings.gridSize",
                       choices: SettingsModel.densities, selected: settings.gridDensity) { settings.setGridDensity($0) }
            Button("Use the server's library order") { Task { await settings.useServerOrder() } }
                .accessibilityIdentifier("settings.serverOrder")
            if settings.offersStartAs {
                SettingRow(title: localized("Always start as"), value: settings.startAsLabel, id: "settings.startAs",
                           choices: settings.startChoices, selected: settings.alwaysStartAs) {
                    settings.setAlwaysStartAs($0)
                }
            }
        }
    }

    private var more: some View {
        VStack(alignment: .leading, spacing: 24) {
            Button("Diagnostics") { app.shell.open(.diagnostics) }
                .accessibilityIdentifier("settings.diagnostics")
            Button("About Pley") { app.shell.open(.about) }
                .accessibilityIdentifier("settings.about")
        }
        .focusSection()
    }
}

/// A group of rows under its heading, with a note under it: words in the
/// language shown (`localized`).
private struct SettingsGroup<Content: View>: View {
    let title: String
    let note: String?
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text(title)
                .font(.headline)
                .foregroundStyle(.secondary)
                .accessibilityAddTraits(.isHeader)
            content
            if let note {
                Text(note).font(.caption).foregroundStyle(.secondary)
            }
        }
        .focusSection()
    }
}

/// A setting as a wide button saying its value; Select opens its choices
/// in the system's list, the current one ticked.
private struct SettingRow<Value: Hashable & Sendable>: View {
    let title: String
    let value: String
    let id: String
    let choices: [SettingChoice<Value>]
    let selected: Value
    let pick: (Value) -> Void
    @State private var open = false

    var body: some View {
        Button {
            open = true
        } label: {
            HStack {
                Text(title)
                Spacer(minLength: 40)
                Text(value).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity)
        }
        .accessibilityLabel(title)
        .accessibilityValue(value)
        .accessibilityIdentifier(id)
        .confirmationDialog(title, isPresented: $open, titleVisibility: .visible) {
            ForEach(choices) { choice in
                Button(choice.value == selected ? "\(choice.label) ✓" : choice.label) { pick(choice.value) }
            }
        }
    }
}
