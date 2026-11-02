#if os(macOS)
import AVFoundation
import Foundation
import PleyAuth
import PleyCaps
import PleyData
import PleyDownloads
import PleyModel
import PleyNetwork
import PleyPlayer
import PleyTesting

/// (l) Downloads, through a foreground session in 1 MiB pieces (the check
/// has no bundle for a background one): Direct H264's original comes
/// byte for byte, Remux H264 AC3's MKV comes converted to an MP4 that
/// AVFoundation plays; then Direct H264 plays from the device with the
/// server out of reach (an address nothing answers on), its reports kept,
/// and sent to the server once it is reached, with their age.
@MainActor
func checkDownloads(api: PleyAPI, tokens: Tokens, transport: HTTPTransport, page: AdminPage, caps: DeviceCaps,
                    reporter: SessionReporter, media: String, direct: Int, converted: Int) async throws {
    let base = workDir ?? FileManager.default.temporaryDirectory.appendingPathComponent("pley-apple-play-\(UUID())")
    let root = base.appendingPathComponent("downloads", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let suite = "pley-apple-play-\(UUID().uuidString)"
    let defaults = try required(UserDefaults(suiteName: suite), "no settings")
    defer { defaults.removePersistentDomain(forName: suite) }
    let settings = AppSettings(defaults: defaults)
    let (downloads, transfer) = Downloads.foreground(root: root, settings: settings, gate: transport.gate,
                                                     pieceBytes: 1 << 20)
    defer { transfer.invalidate() }
    let me = try await api.me()
    let origin = api.client.origin
    downloads.signedIn(DownloadScope(origin: origin, profileId: me.id, api: api, tokens: tokens,
                                     caps: { await caps.warmUp().report }))
    let local = try required(downloads.local, "no downloads to play")

    // The original, as it is.
    try await downloads.download([direct])
    try await waitDone(downloads, direct, seconds: 120)
    let detail = try await api.item(direct)
    let file = try required(detail.files.first { local.file($0.id) != nil }, "Direct H264's download isn't playable")
    let saved = try required(local.file(file.id), "no file")
    let original = try required(mediaFile(media, folder: "Movies", prefix: "Direct H264", ext: "mp4"),
                                "no Direct H264 in the media")
    try check(!saved.converted, "Direct H264 was converted, not downloaded as it is")
    try check(try Data(contentsOf: saved.url) == Data(contentsOf: original),
              "Direct H264's download differs from the original")
    let pieces = Int((try FileManager.default.attributesOfItem(atPath: original.path)[.size] as? Int ?? 0) >> 20) + 1
    say("(l) downloads: Direct H264's original, byte for byte, in \(pieces) piece(s)")

    // A file this Mac plays only remuxed: the server makes an MP4 of it.
    try await downloads.download([converted])
    try await waitDone(downloads, converted, seconds: 300)
    let made = try await api.item(converted)
    let mp4 = try required(made.files.compactMap { local.file($0.id) }.first, "Remux H264 AC3's download isn't playable")
    try check(mp4.converted, "Remux H264 AC3 wasn't converted")
    let asset = AVURLAsset(url: mp4.url)
    try check(try await asset.load(.isPlayable), "AVFoundation can't play the converted download")
    try check(!(try await asset.loadTracks(withMediaType: .video)).isEmpty, "the converted download has no video")
    say("(l) downloads: Remux H264 AC3 converted by the server, and AVFoundation plays it")

    // Without the server: it plays from the device, and its reports are kept.
    let away = try closedOrigin()
    let awayApi = PleyAPI(client: APIClient(origin: away, tokens: tokens, transport: transport))
    let lateFolder = base.appendingPathComponent("late", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: lateFolder) }
    let late = LateReports(origin: origin, profileId: me.id, directory: lateFolder)
    var engine: AVPlayerEngine?
    let env = PlaybackEnvironment(api: awayApi, caps: caps, clientId: "apple-play-check", reporter: reporter,
                                  late: late, engines: {
                                      let made = AVPlayerEngine()
                                      engine = made
                                      return made
                                  }, pictureFolder: base.appendingPathComponent("picture-subtitles"), local: local)
    let controller = PlaybackController(request: PlayRequest(itemId: direct, startMs: 40_000), environment: env)
    controller.start()
    try await wait("the download playing past 43 s", seconds: 30) {
        if case .failed(let message, _) = controller.state.phase { throw CheckFailure("offline play failed: \(message)") }
        let sample = engine?.sample() ?? EngineSample()
        return sample.positionMs > 43_000 && (sample.videoFrames ?? 0) > 0
    }
    try check(controller.state.methodLabel == "Downloaded", "Details say \(controller.state.methodLabel ?? "nothing")")
    try check(controller.session?.local == true, "the download played through a session")
    let sessions = try await page.send("GET", "/api/activity").1["sessions"] as? [[String: Any]] ?? []
    try check(!sessions.contains { ($0["item"] as? [String: Any])?["id"] as? Int == direct },
              "the server has a session for the download")
    controller.pause()
    let stoppedAt = engine?.sample().positionMs ?? 0
    await controller.close()
    await reporter.flush()
    let kept = await late.pending()
    try check(kept.count == 1 && kept.first?.state == .stopped, "the reports kept: \(kept.map(\.state.rawValue))")
    let keptAt = kept.first?.positionMs ?? 0
    try check(abs(keptAt - stoppedAt) <= 1_500, "the report kept is at \(keptAt) ms, the player stopped at \(stoppedAt)")
    say("(l) downloads: played without the server (Downloaded, no session), its stop kept at \(keptAt / 1000) s")

    // Reached again: sent late, with its age.
    try await pause(seconds: 1)
    await late.send(using: api)
    try check(await late.pending().isEmpty, "the late report wasn't sent")
    let place = try await api.item(direct).userState.positionMs
    try check(abs(place - keptAt) <= 1_500, "the server has \(place) ms, not the late report's \(keptAt)")
    say("(l) downloads: the late report reached the server with its age")

    downloads.removeAll()
    try check(downloads.entries.isEmpty, "Delete all left downloads")
}

/// Waits for the download of `itemId` to be whole.
@MainActor
private func waitDone(_ downloads: Downloads, _ itemId: Int, seconds: Double) async throws {
    try await wait("item \(itemId) downloaded", seconds: seconds) {
        guard let entry = downloads.entries.first(where: { $0.itemId == itemId }) else { return false }
        if entry.state == .failed { throw CheckFailure("the download failed: \(entry.error ?? "?")") }
        return entry.state == .done
    }
}

/// The test media's file whose name starts with `prefix`, in `folder`.
private func mediaFile(_ media: String, folder: String, prefix: String, ext: String) -> URL? {
    let root = URL(fileURLWithPath: media).appendingPathComponent(folder)
    let walk = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)
    while let url = walk?.nextObject() as? URL {
        if url.lastPathComponent.hasPrefix(prefix), url.pathExtension == ext { return url }
    }
    return nil
}

/// An address on this Mac that nothing answers on: a port taken, then let go.
private func closedOrigin() throws -> String {
    let fd = socket(AF_INET, SOCK_STREAM, 0)
    guard fd >= 0 else { throw CheckFailure("no socket") }
    defer { close(fd) }
    var address = sockaddr_in()
    address.sin_family = sa_family_t(AF_INET)
    address.sin_addr.s_addr = inet_addr("127.0.0.1")
    address.sin_port = 0
    var length = socklen_t(MemoryLayout<sockaddr_in>.size)
    let bound = withUnsafeMutablePointer(to: &address) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            bind(fd, $0, length) == 0 && getsockname(fd, $0, &length) == 0
        }
    }
    guard bound else { throw CheckFailure("no free port") }
    return "http://127.0.0.1:\(UInt16(bigEndian: address.sin_port))"
}
#endif
