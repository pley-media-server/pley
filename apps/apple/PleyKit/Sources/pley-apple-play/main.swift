// Plays a real, disposable Pley server's test media through the Apple
// apps' player, and checks what the server made of it.
// scripts/check-playback.sh makes the media, starts the server and runs
// this; never point it at a server people use.
//
//   pley-apple-play --server http://127.0.0.1:7712 --admin <username> --media <dir> --caps-fixtures <dir>
//
// `--media` is what make-test-media.sh wrote; `--caps-fixtures` the server's
// recorded Apple caps (apps/server/tests/fixtures/caps). The administrator's
// password comes from PLEY_CHECK_PASSWORD. It prints one line per check and
// stops at the first failure. Nothing it prints holds a token, a code, a
// password or a keyed media address.

#if os(macOS)
import AVFoundation
import Foundation
import PleyAuth
import PleyCaps
import PleyCommon
import PleyModel
import PleyNetwork
import PleyPlayer
import PleyTesting

struct CheckFailure: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

func say(_ line: String) {
    print(line)
    fflush(stdout)
}

func check(_ condition: Bool, _ what: @autoclosure () -> String) throws {
    if !condition { throw CheckFailure(what()) }
}

/// Waits until `condition` holds, polling every 250 ms, `seconds` at most.
@MainActor
func wait(_ what: String, seconds: Double, _ condition: @MainActor () async throws -> Bool) async throws {
    let deadline = Date().addingTimeInterval(seconds)
    while Date() < deadline {
        if try await condition() { return }
        try await Task.sleep(for: .milliseconds(250))
    }
    throw CheckFailure("timed out after \(Int(seconds)) s waiting for \(what)")
}

func pause(seconds: Double) async throws {
    try await Task.sleep(for: .milliseconds(Int(seconds * 1000)))
}

func short(_ id: String) -> String { String(id.prefix(8)) }

// MARK: Setting up

/// Signs the app in with a device code, allowed by the administrator.
func signIn(_ origin: String, page: AdminPage, transport: HTTPTransport) async throws -> PleyAPI {
    let endpoints = EndpointClient(transport: transport)
    let metadata = try await ServerMetadata.fetch(origin, endpoints: endpoints)
    let store = MemoryTokenStore()
    let signIn = DeviceSignIn(metadata: metadata, client: .ios, store: store, endpoints: endpoints)
    let code = try await signIn.start()
    let (verifyStatus, consent) = try await page.send("POST", "/api/oauth/device/verify",
                                                      json: ["user_code": code.userCode])
    guard verifyStatus == 200, let id = consent["id"] else {
        throw CheckFailure("the consent page refused the code (HTTP \(verifyStatus))")
    }
    let (allowStatus, _) = try await page.send("POST", "/api/oauth/requests/\(id)", json: ["allow": true])
    try check(allowStatus == 200, "allowing the app failed (HTTP \(allowStatus))")
    _ = try await signIn.waitForApproval(code)
    let tokens = Tokens(issuer: metadata.issuer, store: store, endpoints: endpoints)
    return PleyAPI(client: APIClient(origin: origin, tokens: tokens, transport: transport))
}

/// Makes a library of `kind` at `path` and waits for `count` items in it; their ids by title.
func library(_ page: AdminPage, name: String, kind: String, path: String, count: Int) async throws -> [String: Int] {
    let (status, made) = try await page.send("POST", "/api/libraries", json: ["name": name, "kind": kind, "paths": [path]])
    guard status == 201, let id = made["id"] as? Int else {
        throw CheckFailure("the \(name) library wasn't made (HTTP \(status))")
    }
    var titles: [String: Int] = [:]
    try await wait("\(count) item(s) in \(name)", seconds: 180) {
        let (_, data) = try await page.raw("GET", "/api/libraries/\(id)/items?limit=100")
        let items = ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?["items"] as? [[String: Any]] ?? []
        titles = Dictionary(items.compactMap { item in
            (item["title"] as? String).flatMap { title in (item["id"] as? Int).map { (title, $0) } }
        }, uniquingKeysWith: { first, _ in first })
        return titles.count >= count
    }
    return titles
}

/// The session as the administrator's activity shows it, or nil.
func activity(_ page: AdminPage, _ sessionId: String) async throws -> [String: Any]? {
    let (status, body) = try await page.send("GET", "/api/activity")
    try check(status == 200, "activity answered HTTP \(status)")
    let sessions = body["sessions"] as? [[String: Any]] ?? []
    return sessions.first { $0["session_id"] as? String == sessionId }
}

// MARK: Playing

/// One playback through the real player: a `PlaybackController` with
/// `AVPlayerEngine` (decoding without a view) and this Mac's caps.
@MainActor
final class Run {
    let controller: PlaybackController
    private(set) var engine: AVPlayerEngine?

    init(api: PleyAPI, caps: DeviceCaps, reporter: SessionReporter, itemId: Int) {
        var made: AVPlayerEngine?
        let env = PlaybackEnvironment(api: api, caps: caps, clientId: "apple-play-check", reporter: reporter, late: nil,
                                      engines: {
                                          let engine = AVPlayerEngine()
                                          made = engine
                                          return engine
                                      })
        controller = PlaybackController(request: PlayRequest(itemId: itemId, fromStart: true), environment: env)
        controller.start()
        engine = made
    }

    var sample: EngineSample { engine?.sample() ?? EngineSample() }

    var session: ServerPlaybackSession {
        get throws {
            guard let session = controller.session else { throw CheckFailure("no session") }
            return session
        }
    }

    /// Waits for frames and the time to move past `ms`.
    func playing(past ms: Int, seconds: Double) async throws {
        try await wait("frames past \(ms / 1000) s", seconds: seconds) {
            if case .failed(let message, _) = self.controller.state.phase {
                throw CheckFailure("playback failed: \(message)")
            }
            let sample = self.sample
            return sample.positionMs > ms && (sample.videoFrames ?? 0) > 0
        }
    }

    func describe() -> String {
        let state = controller.state
        let reasons = state.reasons.isEmpty ? "" : ", " + state.reasons.joined(separator: " ")
        return "\(state.method?.rawValue ?? "?")\(state.videoRange.map { ", \($0)" } ?? "")\(reasons)"
    }
}

/// (a) File 1 plays as it is; reports reach the dashboard; the end saves the place and ends the session.
@MainActor
func checkDirect(api: PleyAPI, page: AdminPage, caps: DeviceCaps, reporter: SessionReporter, itemId: Int) async throws {
    let run = Run(api: api, caps: caps, reporter: reporter, itemId: itemId)
    try await wait("the first frame", seconds: 30) { run.controller.state.phase == .playing }
    let session = try run.session
    try check(session.info.method == .direct, "file 1 plays by \(session.info.method.rawValue), not direct")
    try await pause(seconds: 22)
    let sample = run.sample
    try check((sample.videoFrames ?? 0) > 0, "no frame decoded after 22 s")
    try check(sample.positionMs >= 20_000, "at \(sample.positionMs) ms after 22 s")
    guard let shown = try await activity(page, session.id) else { throw CheckFailure("the session isn't in activity") }
    let shownAt = shown["position_ms"] as? Int ?? -1
    try check(shown["state"] as? String == "playing", "activity shows it \(shown["state"] ?? "nil")")
    try check(abs(shownAt - sample.positionMs) <= 11_000, "activity shows \(shownAt) ms, the player \(sample.positionMs)")
    say("(a) direct: \(sample.videoFrames ?? 0) frames, at \(sample.positionMs / 1000) s; activity shows it playing at \(shownAt / 1000) s")

    run.controller.pause()
    try await pause(seconds: 35)
    guard let paused = try await activity(page, session.id) else {
        throw CheckFailure("the session left activity after 35 s paused")
    }
    try check(paused["state"] as? String == "paused", "activity shows it \(paused["state"] ?? "nil") while paused")
    say("(a) paused 35 s: the session is still there, paused")

    run.controller.play()
    run.controller.seek(toMs: 45_000)
    try await run.playing(past: 47_000, seconds: 30)
    let last = run.sample.positionMs
    await run.controller.close()
    let saved = try await api.item(itemId).userState.positionMs
    try check(abs(saved - last) <= 1_500, "the place saved is \(saved) ms, the player's last \(last) ms")
    do {
        _ = try await api.playbackStatus(session.id)
        throw CheckFailure("the session still answers after closing")
    } catch PleyError.api(404, _, _) {}
    try check(try await activity(page, session.id) == nil, "the session is still in activity after closing")
    say("(a) closed at \(last / 1000) s: the place is saved (\(saved) ms), the session answers 404 and left activity")
}

/// Plays `itemId` until frames pass 3 s, checks it, and closes.
@MainActor
func play(_ name: String, api: PleyAPI, caps: DeviceCaps, reporter: SessionReporter, itemId: Int,
          _ checks: (Run, ServerPlaybackSession) async throws -> String) async throws {
    let run = Run(api: api, caps: caps, reporter: reporter, itemId: itemId)
    do {
        try await run.playing(past: 3_000, seconds: 90)
        let session = try run.session
        let note = try await checks(run, session)
        say("\(name): \(run.describe()); \(run.sample.videoFrames ?? 0) frames\(note.isEmpty ? "" : "; " + note)")
        await run.controller.close()
    } catch {
        await run.controller.close()
        throw error
    }
}

/// The keyed playlist the session plays, and its first media playlist.
func playlists(_ api: PleyAPI, _ info: PlaybackInfo) async throws -> (master: String, media: String) {
    let url = try await api.mediaURL(info.url)
    let master = try await text(url)
    guard let first = master.split(whereSeparator: \.isNewline).first(where: { !$0.hasPrefix("#") && !$0.isEmpty }),
          let mediaURL = URL(string: String(first), relativeTo: url)
    else { throw CheckFailure("the master playlist names no media playlist") }
    try check(mediaURL.absoluteURL.host() == url.host() && mediaURL.absoluteURL.path().hasPrefix("/api/media/"),
              "the media playlist is elsewhere")
    return (master, try await text(mediaURL.absoluteURL))
}

func text(_ url: URL) async throws -> String {
    let (data, response) = try await URLSession.shared.data(from: url)
    let status = (response as? HTTPURLResponse)?.statusCode ?? 0
    try check(status == 200, "a keyed playlist answered HTTP \(status)")
    return String(decoding: data, as: UTF8.self)
}

// MARK: (j) Decisions for each recorded device

/// What `playback_apple.rs` expects for each file, by device class:
/// the method, whether the audio is converted, and the reasons.
enum Table {
    static let mkv = "MKV not supported."
    static let dts = "DTS audio not supported."
    static let av1 = "AV1 video not supported."
    static let mpeg2 = "MPEG-2 video not supported."
    static let mp2 = "MP2 audio not supported."
    static let hdrDevices: Set<String> = ["iphone_a16_hdr", "tv_4k_dv", "mac_m3_xdr", "tv_4k_hdr10"]

    struct Cell: Equatable, CustomStringConvertible {
        let method: PlaybackMethod
        let audioConverted: Bool
        let reasons: [String]
        var description: String {
            "\(method.rawValue)\(audioConverted ? " (audio converted)" : "") [\(reasons.joined(separator: " "))]"
        }
    }

    static func expected(_ file: String, device: String) -> Cell {
        switch file {
        case "Direct H264": Cell(method: .direct, audioConverted: false, reasons: [])
        case "Remux H264 AC3": Cell(method: .remux, audioConverted: false, reasons: [mkv])
        case "HEVC HDR10":
            hdrDevices.contains(device)
                ? Cell(method: .remux, audioConverted: false, reasons: [mkv])
                : Cell(method: .transcode, audioConverted: false, reasons: [mkv, "HDR10 converted to SDR."])
        case "MPEG2": Cell(method: .transcode, audioConverted: true, reasons: [mkv, mpeg2, mp2])
        case "DTS": Cell(method: .remux, audioConverted: true, reasons: [mkv, dts])
        default:
            device == "mac_m3_xdr"
                ? Cell(method: .remux, audioConverted: false, reasons: [mkv])
                : Cell(method: .transcode, audioConverted: false, reasons: [mkv, av1])
        }
    }
}

func checkDecisions(api: PleyAPI, fixtures: URL, items: [String: Int]) async throws {
    let names = try FileManager.default.contentsOfDirectory(atPath: fixtures.path)
        .filter { $0.hasPrefix("apple_") && $0.hasSuffix("_v2.json") }.sorted()
    try check(names.count == 7, "\(names.count) Apple caps fixtures, not 7")
    let files = ["Direct H264", "Remux H264 AC3", "HEVC HDR10", "MPEG2", "DTS", "AV1"]
    var differences: [String] = []
    for name in names {
        let device = String(name.dropFirst("apple_".count).dropLast("_v2.json".count))
        let caps = try PleyJSON.decoder.decode(ClientCaps.self, from: Data(contentsOf: fixtures.appendingPathComponent(name)))
        for file in files {
            guard let itemId = items[file] else { throw CheckFailure("no item \(file)") }
            let info = try await api.startPlayback(PlaybackRequest(itemId: itemId, clientId: "apple-play-check",
                                                                   keyedUrls: true, caps: caps))
            try await api.stopPlayback(info.sessionId)
            let got = Table.Cell(method: info.method, audioConverted: info.audioConverted, reasons: info.reasons)
            let want = Table.expected(file, device: device)
            if got != want { differences.append("\(device), \(file): got \(got), the table says \(want)") }
        }
    }
    if !differences.isEmpty {
        differences.forEach { say("(j) differs: \($0)") }
        throw CheckFailure("\(differences.count) decision(s) differ from playback_apple.rs")
    }
    say("(j) decisions: \(names.count) devices × \(files.count) files match playback_apple.rs")
}

// MARK: The run

@MainActor
func run() async throws {
    var options: [String: String] = [:]
    var arguments = CommandLine.arguments.dropFirst()
    while let name = arguments.popFirst() {
        guard name.hasPrefix("--"), let value = arguments.popFirst() else { throw CheckFailure("unknown argument \(name)") }
        options[String(name.dropFirst(2))] = value
    }
    guard let server = options["server"], let admin = options["admin"], let media = options["media"],
          let fixtures = options["caps-fixtures"]
    else {
        throw CheckFailure("usage: pley-apple-play --server <origin> --admin <username> --media <dir> --caps-fixtures <dir>")
    }
    guard let password = ProcessInfo.processInfo.environment["PLEY_CHECK_PASSWORD"], !password.isEmpty else {
        throw CheckFailure("set PLEY_CHECK_PASSWORD to the administrator's password")
    }
    let version = ProcessInfo.processInfo.operatingSystemVersion
    let agent = UserAgent.format(version: "0.1.0", osName: "macOS",
                                 osVersion: "\(version.majorVersion).\(version.minorVersion)", device: "Mac")
    let transport = HTTPTransport(userAgent: agent)
    guard case .pley(let origin, let info) = await ServerProbe(transport: transport).probe(server) else {
        throw CheckFailure("\(server) isn't a Pley server")
    }
    say("server \(info.name) \(info.version) at \(origin)")

    var page = AdminPage(origin: origin)
    page.userAgent = "pley-apple-play"
    try await page.login(username: admin, password: password)
    let movies = try await library(page, name: "Movies", kind: "movies", path: media + "/Movies", count: 7)
    let shows = try await library(page, name: "Shows", kind: "shows", path: media + "/Shows", count: 1)
    say("libraries: \(movies.count) movies, \(shows.count) show(s)")
    let items = Dictionary(uniqueKeysWithValues: movies.map { title, id in
        // Titles from the folders' names, without the year.
        (title.replacingOccurrences(of: #" \(\d{4}\)$"#, with: "", options: .regularExpression), id)
    })
    func item(_ title: String) throws -> Int {
        guard let id = items[title] else { throw CheckFailure("no movie \(title) (found \(items.keys.sorted()))") }
        return id
    }

    let api = try await signIn(origin, page: page, transport: transport)
    say("signed in as the app")
    let caps = DeviceCaps()
    let snapshot = await caps.warmUp()
    let hdr = snapshot.report.hdrTypes?.contains(.hdr10) ?? false
    say("this Mac: device class \(snapshot.deviceClass), HDR10 \(hdr ? "shown" : "not shown")")
    let reporter = SessionReporter()

    try await checkDirect(api: api, page: page, caps: caps, reporter: reporter, itemId: try item("Direct H264"))

    try await play("(b) remux", api: api, caps: caps, reporter: reporter, itemId: try item("Remux H264 AC3")) { _, session in
        try check(session.info.method == .remux, "file 3 plays by \(session.info.method.rawValue), not remux")
        let (_, media) = try await playlists(api, session.info)
        try check(media.contains(#"#EXT-X-MAP:URI="init.mp4""#), "the media playlist has no fMP4 init segment")
        try check(media.contains(".m4s"), "the media playlist has no .m4s segments")
        return "fMP4 segments (init.mp4, .m4s)"
    }

    try await play("(c) HDR10", api: api, caps: caps, reporter: reporter, itemId: try item("HEVC HDR10")) { _, session in
        let info = session.info
        if hdr {
            try check(info.method == .remux, "file 4 plays by \(info.method.rawValue) on an HDR screen, not remux")
            let (master, _) = try await playlists(api, info)
            try check(master.contains("VIDEO-RANGE=PQ"), "the master playlist doesn't say VIDEO-RANGE=PQ")
            return "PQ in the master playlist"
        }
        try check(info.method == .transcode, "file 4 plays by \(info.method.rawValue) on an SDR screen, not transcode")
        try check(info.reasons.contains("HDR10 converted to SDR."), "the reasons don't say the HDR10 is converted")
        return "converted to SDR, as this screen shows no HDR"
    }

    try await play("(d) MPEG-2", api: api, caps: caps, reporter: reporter, itemId: try item("MPEG2")) { _, session in
        try check(session.info.method == .transcode, "file 5 plays by \(session.info.method.rawValue), not transcode")
        try check(session.info.reasons.contains(Table.mpeg2), "the reasons don't name MPEG-2")
        return ""
    }

    try await play("(e) DTS", api: api, caps: caps, reporter: reporter, itemId: try item("DTS")) { _, session in
        try check(session.info.method == .remux, "file 6 plays by \(session.info.method.rawValue), not remux")
        try check(session.info.audioConverted, "file 6's DTS isn't converted")
        return "audio converted"
    }

    try await checkDecisions(api: api, fixtures: URL(fileURLWithPath: fixtures), items: items)
    await reporter.flush()
    say("ok")
}

do {
    try await run()
} catch {
    let words = (error as? CheckFailure)?.description ?? (error as? LocalizedError)?.errorDescription
        ?? String(describing: type(of: error))
    FileHandle.standardError.write(Data("pley-apple-play: \(words)\n".utf8))
    exit(1)
}
#else
// Only the Mac runs the check; elsewhere the package still builds.
print("pley-apple-play runs on macOS only")
#endif
