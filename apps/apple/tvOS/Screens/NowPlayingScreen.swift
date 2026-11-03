import PleyPlayer
import PleyScreens
import SwiftUI

/// Apple TV's Now Playing tab, there while music has a queue (docs/APPLE.md,
/// Music): the album art, the title, artist and album, how far the track is,
/// Previous, Play/Pause and Next, Shuffle and the sleep timer, then Up next
/// as a row of cards (Select plays that track). The remote's Play/Pause
/// plays and pauses the music from any tab; AirPlay is the system's.
struct NowPlayingScreen: View {
    let app: AppModel
    @ObservedObject private var music: MusicPlayback

    init(app: AppModel) {
        self.app = app
        music = app.music
    }

    var body: some View {
        let page = NowPlayingModel(music.state)
        Group {
            if music.state.active {
                ScrollView {
                    VStack(alignment: .leading, spacing: 50) {
                        HStack(alignment: .center, spacing: 70) {
                            ServerImage(app: app, path: page.artPath, width: 520, symbol: "music.note")
                                .frame(width: 520, height: 520)
                                .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
                                .accessibilityHidden(true)
                            VStack(alignment: .leading, spacing: 30) {
                                heading(page)
                                TVProgress(progress: music.progress)
                                controls(page)
                                extras(page)
                            }
                            .focusSection()
                        }
                        .padding(.horizontal, Metrics.gutter)
                        upNext(page)
                    }
                    .padding(.vertical, 60)
                }
                .scrollClipDisabled()
            } else {
                Text("Nothing is playing")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .accessibilityIdentifier("music.nowPlaying")
    }

    private func heading(_ page: NowPlayingModel) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(page.title)
                .font(.title2.bold())
                .lineLimit(3)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("music.nowPlaying.title")
            if !page.byline.isEmpty {
                Text(page.byline).foregroundStyle(.secondary).lineLimit(2)
            }
            if let error = page.error {
                Text(error).foregroundStyle(.red).accessibilityIdentifier("music.nowPlaying.error")
            }
        }
    }

    private func controls(_ page: NowPlayingModel) -> some View {
        HStack(spacing: 30) {
            Button { music.previous() } label: {
                Image(systemName: "backward.fill")
            }
            .accessibilityLabel("Previous")
            .accessibilityIdentifier("music.previous")
            Button { music.toggle() } label: {
                Image(systemName: page.playing ? "pause.fill" : "play.fill")
                    .frame(width: 60)
            }
            .accessibilityLabel(page.playing ? "Pause" : "Play")
            .accessibilityIdentifier("music.toggle")
            Button { music.next() } label: {
                Image(systemName: "forward.fill")
            }
            .disabled(!page.hasNext)
            .accessibilityLabel("Next")
            .accessibilityIdentifier("music.next")
            if page.loading {
                ProgressView().accessibilityLabel("Loading")
            }
        }
    }

    private func extras(_ page: NowPlayingModel) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 30) {
                Button { music.setShuffle(!page.shuffle) } label: {
                    Label("Shuffle", systemImage: "shuffle")
                }
                .accessibilityValue(page.shuffle ? "On" : "Off")
                .accessibilityAddTraits(page.shuffle ? .isSelected : [])
                .accessibilityIdentifier("music.shuffle")
                .background(page.shuffle ? Color.white.opacity(0.18) : .clear, in: Capsule())
                let choices = SleepChoice.all
                ChoiceButton(title: "Sleep Timer", value: page.sleepSummary ?? "Off",
                             symbol: page.sleep == .off ? "moon" : "moon.fill",
                             text: page.sleepSummary ?? "Sleep Timer",
                             choices: choices.indices.map { Choice(label: choices[$0].label(item: "track"), tag: $0) },
                             selected: choices.firstIndex(of: page.sleep)) { picked in
                    music.setSleep(picked.map { choices[$0] } ?? .off)
                }
                .accessibilityIdentifier("music.sleep")
            }
            if let notice = page.sleepNotice {
                HStack(spacing: 24) {
                    Text(notice).monospacedDigit()
                    Button("Keep listening") { music.keepListening() }
                        .accessibilityIdentifier("music.keepListening")
                }
            }
        }
    }

    @ViewBuilder private func upNext(_ page: NowPlayingModel) -> some View {
        if !page.upNext.isEmpty {
            VStack(alignment: .leading, spacing: 20) {
                Text("Up next")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                    .padding(.horizontal, Metrics.gutter)
                ScrollView(.horizontal) {
                    LazyHStack(spacing: 40) {
                        ForEach(page.upNext) { row in
                            Button { music.jump(to: row.position) } label: {
                                VStack(alignment: .leading, spacing: 8) {
                                    ServerImage(app: app, path: row.artPath, width: 220, symbol: "music.note")
                                        .frame(width: 220, height: 220)
                                        .clipShape(RoundedRectangle(cornerRadius: Metrics.corner))
                                    Text(row.title).font(.caption).lineLimit(1)
                                    Text(row.duration).font(.caption2).foregroundStyle(.secondary)
                                }
                                .frame(width: 220)
                            }
                            .buttonStyle(.borderless)
                            .accessibilityLabel(row.spoken)
                            .accessibilityHint("Plays it")
                            .accessibilityIdentifier("music.upNext.\(row.position)")
                        }
                    }
                    .padding(.horizontal, Metrics.gutter)
                    .padding(.vertical, 20)
                }
                .scrollClipDisabled()
            }
            .focusSection()
        }
    }
}

/// How far the track is: a bar, with the time played and left.
private struct TVProgress: View {
    @ObservedObject var progress: MusicProgress

    var body: some View {
        VStack(spacing: 8) {
            ProgressView(value: progress.fraction)
            HStack {
                Text(NowPlayingModel.elapsed(progress.positionMs))
                Spacer()
                Text(NowPlayingModel.left(progress.positionMs, progress.durationMs))
            }
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        .frame(maxWidth: 900)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Position")
        .accessibilityValue(NowPlayingModel.spokenTime(progress.positionMs, progress.durationMs))
        .accessibilityIdentifier("music.progress")
    }
}
