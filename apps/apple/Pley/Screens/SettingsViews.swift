import PleyData
import PleyPlayer
import PleyScreens
import SwiftUI
#if os(iOS)
import UIKit
#else
import AppKit
#endif

/// Settings on iPhone and iPad, from Me: Playback, Subtitles, Browsing,
/// Sign-in, then Diagnostics and About. Each change is kept at once.
struct SettingsView: View {
    @StateObject private var model: SettingsModel

    init(app: AppModel) {
        _model = StateObject(wrappedValue: SettingsModel(app: app, offersCellular: SettingsSections.offersCellular))
    }

    var body: some View {
        Form {
            SettingsSections.Playback(model: model)
            SettingsSections.Subtitles(model: model)
            SettingsSections.Browsing(model: model)
            SettingsSections.SigningIn(model: model)
            Section {
                NavigationLink(value: Screen.diagnostics) { Text("Diagnostics") }
                    .accessibilityIdentifier("settings.diagnostics")
                NavigationLink(value: Screen.about) { Text("About Pley") }
                    .accessibilityIdentifier("settings.about")
            }
        }
        .navigationTitle("Settings")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
    }
}

#if os(macOS)
/// The Mac's Settings window (⌘,): Playback, Subtitles and Browsing tabs.
struct MacSettingsView: View {
    @StateObject private var model: SettingsModel

    init(app: AppModel) {
        _model = StateObject(wrappedValue: SettingsModel(app: app, offersCellular: false))
    }

    var body: some View {
        TabView {
            Form { SettingsSections.Playback(model: model) }
                .formStyle(.grouped)
                .tabItem { Label("Playback", systemImage: "play.rectangle") }
            Form { SettingsSections.Subtitles(model: model) }
                .formStyle(.grouped)
                .tabItem { Label("Subtitles", systemImage: "captions.bubble") }
            Form {
                SettingsSections.Browsing(model: model)
                SettingsSections.SigningIn(model: model)
            }
            .formStyle(.grouped)
            .tabItem { Label("Browsing", systemImage: "square.grid.2x2") }
        }
        .frame(width: 520, height: 460)
    }
}
#endif

/// The parts of Settings, shared by the iPhone and iPad page and the Mac's window.
enum SettingsSections {
    /// iPhone always has mobile data; an iPad only with cellular; the Mac never.
    @MainActor static var offersCellular: Bool {
        #if os(iOS)
        UIDevice.current.userInterfaceIdiom == .phone || NetworkKind.shared.cellularAvailable
        #else
        false
        #endif
    }

    struct Playback: View {
        @ObservedObject var model: SettingsModel

        var body: some View {
            Section {
                quality(cellular: false)
                if model.offersCellular {
                    quality(cellular: true)
                }
                Toggle("Play the next episode automatically", isOn: Binding(get: { model.autoplayNext },
                                                                            set: { model.setAutoplayNext($0) }))
                    .accessibilityIdentifier("settings.autoplay")
            } header: {
                Text("Playback")
            } footer: {
                Text(model.autoplayNext ? "Up Next counts down, then plays the next episode."
                     : "Up Next waits for you to choose Play Next.")
            }
        }

        private func quality(cellular: Bool) -> some View {
            Picker(model.qualityTitle(cellular: cellular),
                   selection: Binding(get: { cellular ? model.qualityCellular : model.qualityOther },
                                      set: { model.setQuality($0, cellular: cellular) })) {
                ForEach(SettingsModel.qualities) { choice in
                    Text(choice.label).tag(choice.value)
                }
            }
            .accessibilityIdentifier(cellular ? "settings.quality.cellular" : "settings.quality")
        }
    }

    struct Subtitles: View {
        @ObservedObject var model: SettingsModel

        var body: some View {
            Section {
                Picker("Size", selection: Binding(get: { model.subtitles.size }, set: { model.setSubtitleSize($0) })) {
                    ForEach(SettingsModel.sizes) { Text($0.label).tag($0.value) }
                }
                .accessibilityIdentifier("settings.subtitleSize")
                Picker("Background", selection: Binding(get: { model.subtitles.background },
                                                        set: { model.setSubtitleBackground($0) })) {
                    ForEach(SettingsModel.backgrounds) { Text($0.label).tag($0.value) }
                }
                .accessibilityIdentifier("settings.subtitleBackground")
                Picker("Font", selection: Binding(get: { model.subtitles.font }, set: { model.setSubtitleFont($0) })) {
                    ForEach(SettingsModel.fonts) { Text($0.label).tag($0.value) }
                }
                .accessibilityIdentifier("settings.subtitleFont")
                Picker("Position", selection: Binding(get: { model.subtitles.raise },
                                                      set: { model.setSubtitleRaise($0) })) {
                    ForEach(SettingsModel.raises) { Text($0.label).tag($0.value) }
                }
                .accessibilityIdentifier("settings.subtitlePosition")
                SubtitlePreview(style: model.subtitles)
            } header: {
                Text("Subtitles")
            } footer: {
                Text(SettingsModel.systemStyleNote + " Position moves text subtitles only.")
            }
        }
    }

    struct Browsing: View {
        @ObservedObject var model: SettingsModel

        var body: some View {
            Section("Browsing") {
                Picker("Grid size", selection: Binding(get: { model.gridDensity }, set: { model.setGridDensity($0) })) {
                    ForEach(SettingsModel.densities) { Text($0.label).tag($0.value) }
                }
                .accessibilityIdentifier("settings.gridSize")
                Button("Use the server's library order") { Task { await model.useServerOrder() } }
                    .accessibilityIdentifier("settings.serverOrder")
            }
        }
    }

    struct SigningIn: View {
        @ObservedObject var model: SettingsModel

        var body: some View {
            Section {
                Toggle("Sign in privately", isOn: Binding(get: { model.privateSignIn },
                                                          set: { model.setPrivateSignIn($0) }))
                    .accessibilityIdentifier("settings.privateSignIn")
            } header: {
                Text("Signing in")
            } footer: {
                Text(SettingsModel.privateSignInNote)
            }
        }
    }
}

/// A line of subtitles as the choices draw it, over a dark picture. System
/// choices show Pley's own look.
struct SubtitlePreview: View {
    let style: SubtitleOverrides

    var body: some View {
        let scale = style.size.scale ?? 1
        Text(SettingsModel.previewText)
            .font(font(size: 17 * scale))
            .foregroundStyle(.white)
            .shadow(color: .black, radius: 1)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(style.background == .darkBox ? Color.black.opacity(0.75) : .clear)
            .padding(.bottom, 12 + 120 * style.raise.share)
            .frame(maxWidth: .infinity, minHeight: 110, alignment: .bottom)
            .background(LinearGradient(colors: [Color(white: 0.35), Color(white: 0.12)], startPoint: .top,
                                       endPoint: .bottom),
                        in: RoundedRectangle(cornerRadius: Metrics.corner))
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Preview: \(SettingsModel.previewText)")
            .accessibilityIdentifier("settings.subtitlePreview")
    }

    private func font(size: Double) -> Font {
        switch style.font {
        case .system, .sansSerif: .system(size: size, weight: .bold)
        case .serif: .system(size: size, weight: .bold, design: .serif)
        case .monospace: .system(size: size, weight: .bold, design: .monospaced)
        case .casual: .custom(SubtitleFont.casualName, size: size)
        }
    }
}

/// History: the profile's plays, newest first, 15 at a time with Show
/// more. A row opens its page; a swipe (iPhone and iPad) or the context
/// menu removes it.
struct HistoryView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: HistoryModel

    init(app: AppModel) {
        self.app = app
        _model = StateObject(wrappedValue: HistoryModel(app: app))
    }

    var body: some View {
        content
            .navigationTitle("History")
            .task { await model.load() }
            .reloads(app) { await model.load() }
            #if os(iOS)
            .refreshable { await model.load() }
            #endif
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            StateView(kind: .loading)
        case .failed(let why):
            StateView(kind: .failed(title: "Couldn't load your history", why: why,
                                    retry: { Task { await model.load() } }))
        case .loaded where model.rows.isEmpty:
            StateView(kind: .empty(symbol: "clock", text: HistoryModel.emptyText, action: nil))
                .accessibilityIdentifier("history.empty")
        case .loaded:
            List {
                ForEach(model.rows) { row in
                    HistoryRowView(app: app, row: row) { app.shell.open(row.opens) }
                        .swipeActions {
                            Button("Remove", role: .destructive) { Task { await model.remove(row.id) } }
                        }
                        .contextMenu {
                            Button(role: .destructive) {
                                Task { await model.remove(row.id) }
                            } label: {
                                Label("Remove from History", systemImage: "trash")
                            }
                        }
                        .accessibilityAction(named: "Remove from History") { Task { await model.remove(row.id) } }
                }
                if model.canLoadMore {
                    Button {
                        Task { await model.loadMore() }
                    } label: {
                        HStack {
                            Text("Show more")
                            if model.loadingMore { ProgressView().padding(.leading, 6) }
                        }
                    }
                    .disabled(model.loadingMore)
                    .accessibilityIdentifier("history.more")
                }
            }
            #if os(macOS)
            .listStyle(.inset)
            #endif
            .accessibilityIdentifier("history.list")
        }
    }
}

/// A play: its picture, title, what it was and when and where it played.
private struct HistoryRowView: View {
    let app: AppModel
    let row: HistoryRow
    let open: () -> Void
    @ScaledMetric(relativeTo: .body) private var width: CGFloat = 44

    var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                ServerImage(app: app, paths: row.pictures, width: min(width, 70), symbol: Metrics.symbol(row.kind))
                    .frame(width: min(width, 70), height: min(width, 70) * (row.square ? 1 : 1.5))
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                VStack(alignment: .leading, spacing: 2) {
                    Text(row.title).font(.body.weight(.medium)).lineLimit(2)
                    if !row.subtitle.isEmpty {
                        Text(row.subtitle).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                    }
                    Text([row.time, row.device].compactMap { $0 }.joined(separator: " · "))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(row.spoken)
        .accessibilityHint("Opens its page")
        .accessibilityAddTraits(.isButton)
        .accessibilityIdentifier("history.row.\(row.id)")
    }
}

/// Diagnostics: the device's class and what it plays, and the report the
/// app sends with each playback request, which can be selected and copied.
struct DiagnosticsView: View {
    @StateObject private var model: DiagnosticsModel

    init(app: AppModel) {
        _model = StateObject(wrappedValue: DiagnosticsModel(app: app))
    }

    var body: some View {
        List {
            Section("This device") {
                LabeledContent("Device class", value: model.deviceClass)
                ForEach(model.lines, id: \.self) { line in
                    Text(line).font(.callout)
                }
            }
            Section {
                Text(model.json)
                    .font(.system(.footnote, design: .monospaced))
                    .textSelection(.enabled)
                    .accessibilityIdentifier("diagnostics.json")
                Button("Copy") { copy(model.json) }
                    .accessibilityIdentifier("diagnostics.copy")
            } header: {
                Text("What Pley sends with each playback request")
            }
        }
        #if os(macOS)
        .listStyle(.inset)
        #endif
        .navigationTitle("Diagnostics")
        .task { await model.load() }
    }

    private func copy(_ text: String) {
        #if os(iOS)
        UIPasteboard.general.string = text
        #else
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #endif
    }
}

/// About: Pley's version, that the app holds no third-party code, and Pley's licence.
struct AboutView: View {
    private let model = AboutModel()

    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Pley").font(.title.bold())
                    Text(model.title)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("about.version")
                }
                .padding(.vertical, 6)
            }
            Section {
                Text(AboutModel.licences)
                Text(AboutModel.licence)
            }
        }
        #if os(macOS)
        .listStyle(.inset)
        #endif
        .navigationTitle("About Pley")
    }
}
