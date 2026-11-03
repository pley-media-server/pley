@testable import PleyPlayer
import Testing

/// The system's Now Playing as the tests see it.
@MainActor
final class FakeNowPlayingSystem: NowPlayingSystem {
    private(set) var shown: [NowPlayingInfo?] = []
    private(set) var commands: Set<RemoteCommand> = []
    private var handler: (@MainActor @Sendable (RemoteEvent) -> Void)?

    func show(_ info: NowPlayingInfo?) { shown.append(info) }

    func handle(_ commands: Set<RemoteCommand>, _ handler: @escaping @MainActor @Sendable (RemoteEvent) -> Void) {
        self.commands = commands
        self.handler = handler
    }

    /// The person used `event` on the lock screen.
    func use(_ event: RemoteEvent) { handler?(event) }
}

@MainActor
struct NowPlayingTests {
    @Test func theOwnerHearsItsCommands() {
        let system = FakeNowPlayingSystem()
        let center = NowPlayingCenter(system: system)
        var heard: [RemoteEvent] = []
        center.take(.music, info: NowPlayingInfo(title: "Sunson", artist: "Nils Frahm", kind: .audio)) { heard.append($0) }
        #expect(center.owner == .music)
        #expect(system.commands == [.play, .pause, .toggle, .nextTrack, .previousTrack, .changePosition])
        #expect(system.shown.last??.title == "Sunson")
        system.use(.next)
        #expect(heard == [.next])
    }

    @Test func videoTakesOverAndLiveHasNoCommands() {
        let system = FakeNowPlayingSystem()
        let center = NowPlayingCenter(system: system)
        var music: [RemoteEvent] = []
        var video: [RemoteEvent] = []
        center.take(.music, info: NowPlayingInfo(title: "Sunson", kind: .audio)) { music.append($0) }
        center.take(.video, info: NowPlayingInfo(title: "Dune", kind: .video)) { video.append($0) }
        #expect(system.commands.contains(.skipForward) && !system.commands.contains(.nextTrack))
        system.use(.skip(seconds: 10))
        #expect(music.isEmpty && video == [.skip(seconds: 10)])
        // Music's updates don't show while the video owns Now Playing.
        center.update(.music, info: NowPlayingInfo(title: "Other", kind: .audio))
        #expect(system.shown.last??.title == "Dune")
        // Music letting go doesn't take the video's away.
        center.release(.music)
        #expect(center.owner == .video)
        center.take(.live, info: NowPlayingInfo(title: "4 News", kind: .video, isLive: true)) { _ in }
        #expect(system.commands.isEmpty)
        center.release(.live)
        #expect(center.owner == nil)
        #expect(system.shown.last == .some(nil))
    }

    @Test func theSameInfoIsntShownTwice() {
        let system = FakeNowPlayingSystem()
        let center = NowPlayingCenter(system: system)
        let info = NowPlayingInfo(title: "Sunson", elapsedMs: 1_000, rate: 1, kind: .audio)
        center.take(.music, info: info) { _ in }
        center.update(.music, info: info)
        #expect(system.shown.count == 1)
        var moved = info
        moved.elapsedMs = 2_000
        center.update(.music, info: moved)
        #expect(system.shown.count == 2)
    }

    @Test func eachOwnerOffersItsCommands() {
        #expect(NowPlayingCenter.commands(for: .live).isEmpty)
        #expect(NowPlayingCenter.commands(for: .video).contains(.changePosition))
        #expect(!NowPlayingCenter.commands(for: .video).contains(.nextTrack))
    }
}
