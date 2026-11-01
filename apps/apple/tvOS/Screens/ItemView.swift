import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// An item's page for the sofa: the poster beside the title, its facts and
/// the file's video and sound, then Play (or Resume and Play from start)
/// and the version, audio and subtitle pickers. A show or season plays its
/// on-deck episode.
struct ItemView: View {
    @ObservedObject private var app: AppModel
    @StateObject private var model: ItemModel
    /// A player was open: once it closes, the page reads the item again
    /// for its new resume point.
    @State private var sawPlayer = false
    @Environment(\.displayScale) private var displayScale

    init(app: AppModel, itemId: Int) {
        self.app = app
        _model = StateObject(wrappedValue: ItemModel(app: app, itemId: itemId))
    }

    var body: some View {
        content
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
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        case .failed(let why):
            VStack(spacing: 24) {
                Text(asSentence(why)).multilineTextAlignment(.center)
                Button("Try again") { Task { await model.load() } }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .loaded:
            ScrollView {
                HStack(alignment: .top, spacing: 60) {
                    if let item = model.item {
                        ServerImage(app: app, path: HomeModel.posterPath(item.summary),
                                    widthPx: HomeModel.pixels(400, scale: Double(displayScale)))
                            .frame(width: 400, height: 600)
                            .clipShape(RoundedRectangle(cornerRadius: 12))
                            .accessibilityHidden(true)
                    }
                    VStack(alignment: .leading, spacing: 24) {
                        details
                        actions.focusSection()
                        pickers.focusSection()
                    }
                }
                .padding(80)
            }
        }
    }

    private var details: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(model.title).font(.title2.bold())
            if !model.facts.isEmpty {
                Text(model.facts).foregroundStyle(.secondary)
            }
            if let episode = model.episodeLine {
                Text(episode).font(.headline)
            }
            if let line = model.videoLine {
                Text(line).font(.callout).foregroundStyle(.secondary)
            }
            if let notice = model.notice {
                Text(notice).font(.callout).foregroundStyle(.red)
            }
            if let summary = model.summary {
                Text(summary).font(.callout).lineLimit(6)
            }
        }
    }

    @ViewBuilder private var actions: some View {
        if model.request() == nil {
            Text("Nothing to play here yet.").foregroundStyle(.secondary)
        } else {
            HStack(spacing: 30) {
                if let resume = model.resumeLabel {
                    Button {
                        play(model.request())
                    } label: {
                        Label(resume, systemImage: "play.fill")
                    }
                    Button("Play from start") { play(model.request(fromStart: true)) }
                } else {
                    Button {
                        play(model.request())
                    } label: {
                        Label("Play", systemImage: "play.fill")
                    }
                }
            }
        }
    }

    @ViewBuilder private var pickers: some View {
        VStack(alignment: .leading, spacing: 16) {
            if !model.versions.isEmpty {
                Text("Version").font(.headline)
                Picker("Version", selection: Binding(get: { model.file?.id ?? 0 },
                                                     set: { model.chooseVersion($0) })) {
                    ForEach(model.versions) { Text($0.label).tag($0.id) }
                }
            }
            if model.audioTracks.count > 1 {
                Text("Audio").font(.headline)
                Picker("Audio", selection: Binding(get: { model.audioIndex ?? -1 },
                                                   set: { index in Task { await model.chooseAudio(index) } })) {
                    ForEach(model.audioTracks) { Text($0.label).tag($0.id) }
                }
            }
            if !model.subtitleTracks.isEmpty {
                let subtitles = Binding<Int?>(get: { model.subtitleIndex },
                                              set: { index in Task { await model.chooseSubtitle(index) } })
                Text("Subtitles").font(.headline)
                Picker("Subtitles", selection: subtitles) {
                    Text("Off").tag(Int?.none)
                    ForEach(model.subtitleTracks) { Text($0.label).tag(Int?.some($0.id)) }
                }
            }
        }
        .frame(maxWidth: 900, alignment: .leading)
    }

    private func play(_ request: PlayRequest?) {
        guard let request else { return }
        Task { await app.play(request) }
    }
}
