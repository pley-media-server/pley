import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer
@testable import PleyScreens
import PleyTesting
import Testing

/// A music output that plays nothing: what it was asked to load, and
/// whether it is meant to play.
@MainActor
final class QuietMusicOutput: MusicOutput {
    let events: AsyncStream<MusicOutputEvent>
    private let continuation: AsyncStream<MusicOutputEvent>.Continuation
    private(set) var loads: [Int] = []
    var positionMs = 0
    var durationMs = 0
    private(set) var playWhenReady = false

    init() {
        (events, continuation) = AsyncStream.makeStream(of: MusicOutputEvent.self)
    }

    func load(url: URL, startMs: Int, play: Bool) {
        loads.append(startMs)
        positionMs = startMs
        playWhenReady = play
        continuation.yield(.playWhenReady(play))
        if play { continuation.yield(.playing(true)) }
    }

    func play() {
        playWhenReady = true
        continuation.yield(.playWhenReady(true))
    }

    func pause() {
        playWhenReady = false
        continuation.yield(.playWhenReady(false))
    }

    func seek(toMs positionMs: Int) { self.positionMs = positionMs }
    func stop() {}
    func release() { continuation.finish() }
    func emit(_ event: MusicOutputEvent) { continuation.yield(event) }
}

/// The system's Now Playing, kept to look at.
@MainActor
final class QuietNowPlaying: NowPlayingSystem {
    private(set) var info: NowPlayingInfo?
    private(set) var commands: Set<RemoteCommand> = []
    private var handler: (@MainActor @Sendable (RemoteEvent) -> Void)?

    func show(_ info: NowPlayingInfo?) { self.info = info }

    func handle(_ commands: Set<RemoteCommand>, _ handler: @escaping @MainActor @Sendable (RemoteEvent) -> Void) {
        self.commands = commands
        self.handler = handler
    }

    func use(_ event: RemoteEvent) { handler?(event) }
}

/// A signed-in app playing music into `output`, its Now Playing in `system`.
@MainActor
private func musicApp(_ server: FixtureServer, output: QuietMusicOutput, system: QuietNowPlaying,
                      tv: Bool = false) async throws -> AppModel {
    server.addMusic()
    let store = MemoryTokenStore()
    try await keepSignIn(server, in: store)
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                  music: { output }, nowPlaying: { system }, tv: tv)
    await app.start()
    try #require(app.root == .signedIn)
    return app
}

@MainActor
@Suite struct MusicScreensTests {
    @Test func anAlbumPlaysFromItsFirstTrack() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        #expect(album.canPlay && album.canShuffle)
        await album.play()
        #expect(app.music.state.track?.id == 304)
        #expect(app.music.state.upNext.map(\.track.id) == [303, 305])
        #expect(app.shell.nowPlaying)
        #expect(await eventually { output.loads.count == 1 && app.music.state.playing })
        // The system's Now Playing shows the track, with music's commands.
        #expect(await eventually { system.info?.title == "Sunson" })
        #expect(system.info?.artist == "Nils Frahm" && system.info?.album == "All Melody")
        #expect(system.info?.kind == .audio && system.info?.rate == 1)
        #expect(system.commands.contains(.nextTrack))
        system.use(.next)
        #expect(await eventually { app.music.state.track?.id == 303 })
    }

    @Test func aTrackRowPlaysFromItOrPausesTheOnePlaying() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.playOrToggle(305)
        #expect(app.music.state.track?.id == 305)
        #expect(!app.music.state.hasNext)
        #expect(await eventually { app.music.state.playing })
        await album.playOrToggle(305)
        #expect(await eventually { !app.music.state.playing })
        await album.playOrToggle(305)
        #expect(await eventually { app.music.state.playing })
        #expect(output.loads.count == 1)
    }

    @Test func shuffleStartsAnywhereAndShuffles() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.play(shuffle: true)
        #expect(app.music.state.shuffle)
        #expect(Set(app.music.state.queue.map(\.id)) == [303, 304, 305])
        #expect(app.music.state.index == 0)
    }

    @Test func anArtistPlaysEveryAlbumsTracks() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system)
        let artist = ArtistModel(app: app, itemId: 301)
        await artist.load()
        #expect(artist.canPlay)
        await artist.play()
        #expect(!artist.starting)
        #expect(app.music.state.queue.map(\.id) == [304, 303, 305])
        await artist.play(shuffle: true)
        #expect(app.music.state.shuffle)
    }

    @Test func nowPlayingSaysWhatPlays() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.play()
        #expect(await eventually { app.music.state.playing })
        let page = NowPlayingModel(app.music.state)
        #expect(page.title == "Sunson")
        #expect(page.byline == "Nils Frahm · All Melody")
        #expect(page.spoken == "Now playing, Sunson, Nils Frahm")
        #expect(page.upNext.map(\.title) == ["The Whole Universe Wants to Be Touched", "Forever Changeless"])
        #expect(page.upNext.last?.byline == "Guest Player · All Melody")
        #expect(page.upNext.first?.duration == "4:05")
        #expect(page.albumId == 302 && page.artistId == 301)
        app.music.setSleep(.minutes(15))
        #expect(NowPlayingModel(app.music.state).sleepSummary == "15 min left")
        #expect(NowPlayingModel.left(65_000, 190_000) == "−2:05")
        #expect(NowPlayingModel.elapsed(65_000) == "1:05")
    }

    @Test func aVideoPausesTheMusicAndGivesNowPlayingBack() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.addPlayable(.init(itemId: 7, fileId: 70, positionMs: 0))
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.play()
        #expect(await eventually { app.music.state.playing })
        let controller = try #require(await app.play(PlayRequest(itemId: 7)))
        #expect(!app.music.state.playing)
        #expect(app.music.active)
        #expect(app.nowPlaying.owner == .video)
        #expect(system.commands.contains(.skipForward))
        await app.closePlayer(controller)
        #expect(app.nowPlaying.owner == .music)
        #expect(system.info?.title == "Sunson")
        // Music playing closes the video.
        _ = try #require(await app.play(PlayRequest(itemId: 7)))
        await album.playOrToggle(303)
        #expect(app.player == nil)
        #expect(app.nowPlaying.owner == .music)
    }

    @Test func theServerStoppingTheMusicsSessionStopsIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system)
        #expect(await eventually { server.openEventStreams == 1 })
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.play()
        #expect(await eventually { app.music.sessionId != nil })
        let id = try #require(app.music.sessionId)
        server.pushEvent(#"{"type":"session_stopped","session_id":"\#(id)","message":null,"by_admin":true}"#)
        #expect(await eventually { app.music.state.error == .stopped })
        #expect(NowPlayingModel(app.music.state).error == "Playback was stopped by the server.")
    }

    @Test func signingOutStopsTheMusic() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.play()
        #expect(app.music.active)
        await app.signOut()
        #expect(!app.music.active)
        #expect(!app.shell.nowPlaying)
        #expect(app.nowPlaying.owner == nil && system.info == nil)
    }

    @Test func appleTVOpensTheNowPlayingTab() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let (output, system) = (QuietMusicOutput(), QuietNowPlaying())
        let app = try await musicApp(server, output: output, system: system, tv: true)
        let album = AlbumModel(app: app, itemId: 302)
        await album.load()
        await album.play()
        #expect(app.shell.sections.contains(.nowPlaying))
        #expect(app.shell.section == .nowPlaying)
        app.music.stop()
        #expect(!app.shell.sections.contains(.nowPlaying))
        #expect(app.shell.section != .nowPlaying)
    }
}
