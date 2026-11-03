import Foundation
import PleyData
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleySubtitles
import PleyTesting
import Testing

/// Downloads as the player takes them: files in a folder of the test's,
/// details given as saved.
@MainActor
private final class FakeLocal: LocalMedia {
    var files: [Int: LocalFile] = [:]
    var details: [Int: ItemDetail] = [:]
    var next: [Int: ItemSummary] = [:]
    private(set) var played: [(itemId: Int, positionMs: Int)] = []

    func file(_ fileId: Int) -> LocalFile? { files[fileId] }
    func detail(_ itemId: Int) async -> ItemDetail? { details[itemId] }
    func played(_ itemId: Int, positionMs: Int, durationMs: Int?) { played.append((itemId, positionMs)) }
    func nextAfter(_ itemId: Int) async -> ItemSummary? { next[itemId] }
}

/// Two 75 s episodes, the first with English SRT (2) and PGS (3); both downloaded.
@MainActor
private func downloadRig(saveDetails: Bool = true) async throws -> (Rig, FakeLocal, URL) {
    let server = FixtureServer()
    try await server.start()
    for (id, number) in [(31, 1), (32, 2)] {
        var episode = FixtureServer.Playable(itemId: id, fileId: id * 10, title: "Episode \(number)", container: "mp4",
                                             durationMs: 75_000, positionMs: number == 1 ? 12_000 : 0)
        episode.episode = (show: "Pley Test Show", season: 1, number: number)
        episode.next = number == 1 ? 32 : nil
        if number == 1 {
            var srt = MediaStream(index: 2, kind: .subtitle, codec: "subrip", language: "eng")
            srt.default = true
            episode.subtitles = [srt, MediaStream(index: 3, kind: .subtitle, codec: "hdmv_pgs_subtitle", language: "eng")]
            episode.selectedSubtitle = 2
        }
        server.addPlayable(episode)
    }
    let folder = FileManager.default.temporaryDirectory.appendingPathComponent("pley-local-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    let local = FakeLocal()
    let vtt = folder.appendingPathComponent("2.vtt")
    try Data("WEBVTT\n\n00:00:12.000 --> 00:00:14.000\nSaved beside it.\n".utf8).write(to: vtt)
    for id in [31, 32] {
        let media = folder.appendingPathComponent("\(id).mp4")
        try Data(count: 64).write(to: media)
        local.files[id * 10] = LocalFile(url: media, subtitles: id == 31 ? [2: vtt] : [:])
        if saveDetails { local.details[id] = try await api(server).item(id) }
    }
    let rig = try Rig(server: server, request: PlayRequest(itemId: 31), local: local)
    return (rig, local, folder)
}

@MainActor
private func started(_ rig: Rig) async throws {
    rig.controller.start()
    try await until("the media loaded") { rig.engine.loads.count == 1 }
    rig.engine.emit(.ready)
    try await until("the first frame") { rig.controller.state.phase == .playing }
}

@MainActor
private func at(_ rig: Rig, _ positionMs: Int, after ms: Int64 = 500) {
    rig.engine.playing(at: positionMs)
    rig.clock.advance(ms: ms)
    rig.controller.tick()
}

private func texts(_ cues: ShownSubtitles) -> [String] {
    if case .text(let shown) = cues { return shown.map(\.plain) }
    return []
}

@Suite("Downloads in the controller", .serialized)
@MainActor
struct LocalPlaybackTests {
    @Test func aDownloadPlaysFromThisDeviceWithoutASession() async throws {
        let (rig, local, folder) = try await downloadRig()
        defer {
            rig.cleanUp()
            try? FileManager.default.removeItem(at: folder)
        }
        try await started(rig)
        let load = try #require(rig.engine.loads.first)
        #expect(load.url == local.files[310]?.url && load.url.isFileURL && load.startMs == 12_000)
        #expect(rig.server.playbackRequests.isEmpty)
        let state = rig.controller.state
        #expect(state.downloaded && state.methodLabel == "Downloaded")
        #expect(state.qualities.map(\.label) == ["Downloaded"] && state.quality == "Downloaded")
        #expect(!state.canFindSubtitles)
        // Only the text track saved beside it is offered, and it shows from the file.
        #expect(state.subtitles.map(\.id) == ["off", "2"])
        try await until("the saved line") {
            at(rig, 12_500)
            return texts(rig.controller.state.cues) == ["Saved beside it."]
        }
        #expect(rig.server.journal.allSatisfy { !$0.path.contains("/subtitles/") })
        // A quality can't be picked.
        rig.controller.selectQuality(Qualities.automatic.id)
        #expect(rig.controller.state.quality == "Downloaded")

        rig.controller.pause()
        at(rig, 20_000)
        await rig.controller.close()
        let reports = await rig.reports()
        #expect(!reports.isEmpty && reports.allSatisfy { $0.sessionId == nil && $0.ageMs == nil })
        #expect(reports.last?.state == .stopped && reports.last?.positionMs == 20_000)
        #expect(!rig.server.journal.contains { $0.method == "DELETE" })
        #expect(local.played.last?.itemId == 31 && local.played.last?.positionMs == 20_000)
    }

    @Test func withoutTheServerItPlaysFromWhatWasSavedAndKeepsItsReports() async throws {
        let (rig, local, folder) = try await downloadRig()
        defer {
            rig.cleanUp()
            try? FileManager.default.removeItem(at: folder)
        }
        rig.server.stop()
        try await started(rig)
        #expect(rig.controller.state.title == "Pley Test Show")
        #expect(rig.engine.loads.first?.url == local.files[310]?.url)
        rig.controller.pause()
        at(rig, 30_000)
        await rig.controller.close()
        await rig.reporter.flush()
        let kept = await rig.late.pending()
        #expect(kept.count == 1 && kept.first?.itemId == 31)
        #expect(kept.first?.state == .stopped && kept.first?.positionMs == 30_000)
        #expect(local.played.last?.positionMs == 30_000)
    }

    @Test func withoutTheServerSomethingNotDownloadedSaysSo() async throws {
        let (rig, local, folder) = try await downloadRig()
        defer {
            rig.cleanUp()
            try? FileManager.default.removeItem(at: folder)
        }
        local.files = [:]
        rig.server.stop()
        rig.controller.start()
        try await until("the failure") { rig.controller.state.isFailed }
        #expect(rig.engine.loads.isEmpty)
    }

    @Test func upNextIsTheNextEpisodeDownloaded() async throws {
        let (rig, local, folder) = try await downloadRig()
        defer {
            rig.cleanUp()
            try? FileManager.default.removeItem(at: folder)
        }
        var next = try #require(local.details[32]?.summary)
        next.title = "The Next Downloaded"
        local.next[31] = next
        try await started(rig)
        rig.engine.emit(.ended)
        try await until("the card") { rig.controller.state.upNext != nil }
        #expect(rig.controller.state.upNext?.title == "The Next Downloaded")
        rig.controller.playNext()
        try await until("the next one loaded") { rig.engine.loads.count == 2 }
        #expect(rig.engine.loads.last?.url == local.files[320]?.url)
        #expect(rig.server.playbackRequests.isEmpty)
    }

    @Test func aTitleNotDownloadedStreamsAsBefore() async throws {
        let (rig, local, folder) = try await downloadRig()
        defer {
            rig.cleanUp()
            try? FileManager.default.removeItem(at: folder)
        }
        local.files = [:]
        try await started(rig)
        #expect(!rig.controller.state.downloaded && rig.controller.state.canFindSubtitles)
        #expect(rig.server.playbackRequests.count == 1)
    }
}
