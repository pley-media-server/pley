import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// An item's page: its poster, title, year and runtime, the file's video
/// and sound, Play (or Resume and Play from start), and the version, audio
/// and subtitle pickers. A show or season plays its on-deck episode.
struct ItemView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ItemModel
    /// A player was open: once it closes, the page reads the item again
    /// for its new resume point.
    @State private var sawPlayer = false
    @Environment(\.displayScale) private var displayScale
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

    init(app: AppModel, itemId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: ItemModel(app: app, itemId: itemId))
    }

    var body: some View {
        content
            .navigationTitle(model.title)
            .task { await model.load() }
            .onReceive(app.$player) { player in
                if player != nil {
                    sawPlayer = true
                } else if sawPlayer {
                    sawPlayer = false
                    Task { await model.load() }
                }
            }
    }

    @ViewBuilder private var content: some View {
        switch model.state {
        case .loading:
            ProgressView()
                .controlSize(.large)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .failed(let why):
            VStack(spacing: 12) {
                Text(asSentence(why)).multilineTextAlignment(.center)
                Button("Try again") { Task { await model.load() } }
            }
            .padding()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .loaded:
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    header
                    actions
                    pickers
                    if let notice = model.notice {
                        Text(notice).font(.footnote).foregroundStyle(.red)
                    }
                    if let summary = model.summary {
                        Text(summary)
                    }
                }
                .padding()
                .frame(maxWidth: 760, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 16) {
            if let item = model.item {
                ServerImage(app: app, path: HomeModel.posterPath(item.summary),
                            widthPx: HomeModel.pixels(120, scale: Double(displayScale)))
                    .frame(width: 120, height: 180)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .accessibilityHidden(true)
            }
            VStack(alignment: .leading, spacing: 6) {
                Text(model.title).font(.title2.bold())
                if !model.facts.isEmpty {
                    Text(model.facts).foregroundStyle(.secondary)
                }
                if let episode = model.episodeLine {
                    Text(episode).font(.headline)
                }
                if let line = model.videoLine {
                    Text(line).font(.footnote).foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder private var actions: some View {
        if model.playable == nil || model.request() == nil {
            Text("Nothing to play here yet.").foregroundStyle(.secondary)
        } else {
            HStack(spacing: 12) {
                if let resume = model.resumeLabel {
                    Button {
                        play(model.request())
                    } label: {
                        Label(resume, systemImage: "play.fill")
                    }
                    .buttonStyle(.borderedProminent)
                    Button("Play from start") { play(model.request(fromStart: true)) }
                        .buttonStyle(.bordered)
                } else {
                    Button {
                        play(model.request())
                    } label: {
                        Label("Play", systemImage: "play.fill")
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
            .controlSize(.large)
        }
    }

    @ViewBuilder private var pickers: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !model.versions.isEmpty {
                LabeledContent("Version") {
                    Picker("Version", selection: Binding(get: { model.file?.id ?? 0 },
                                                         set: { model.chooseVersion($0) })) {
                        ForEach(model.versions) { Text($0.label).tag($0.id) }
                    }
                    .labelsHidden()
                }
            }
            if model.audioTracks.count > 1 {
                LabeledContent("Audio") {
                    Picker("Audio", selection: Binding(get: { model.audioIndex ?? -1 },
                                                       set: { index in Task { await model.chooseAudio(index) } })) {
                        ForEach(model.audioTracks) { Text($0.label).tag($0.id) }
                    }
                    .labelsHidden()
                }
            }
            if !model.subtitleTracks.isEmpty {
                let subtitles = Binding<Int?>(get: { model.subtitleIndex },
                                              set: { index in Task { await model.chooseSubtitle(index) } })
                LabeledContent("Subtitles") {
                    Picker("Subtitles", selection: subtitles) {
                        Text("Off").tag(Int?.none)
                        ForEach(model.subtitleTracks) { Text($0.label).tag(Int?.some($0.id)) }
                    }
                    .labelsHidden()
                }
            }
        }
        .pickerStyle(.menu)
        .frame(maxWidth: 420, alignment: .leading)
    }

    /// The player: full screen on iPhone and iPad, its own window on the Mac.
    private func play(_ request: PlayRequest?) {
        guard let request else { return }
        #if os(macOS)
        openWindow(id: PlayerWindow.id, value: request)
        #else
        Task { await app.play(request) }
        #endif
    }
}
