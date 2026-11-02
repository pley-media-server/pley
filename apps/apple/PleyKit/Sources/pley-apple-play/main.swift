// Plays a real, disposable Pley server's test media through the Apple
// apps' player, and checks what the server made of it.
// scripts/check-playback.sh makes the media, starts the server and runs
// this; never point it at a server people use.
//
//   pley-apple-play --server http://127.0.0.1:7712 --admin <username> --media <dir> --caps-fixtures <dir>
//                   [--work-dir <dir>] [--live-source <origin>]
//
// `--media` is what make-test-media.sh wrote; `--caps-fixtures` the server's
// recorded Apple caps (apps/server/tests/fixtures/caps); `--work-dir` where
// the player downloads picture subtitles (inside the check's temporary
// folder, so nothing is left in the user's Caches); `--live-source` where
// live-source.sh serves its channels, added as a Live TV source. The administrator's
// password comes from PLEY_CHECK_PASSWORD. It prints one line per check and
// stops at the first failure. Nothing it prints holds a token, a code, a
// password or a keyed media address.

#if os(macOS)
import AVFoundation
import Foundation
import PleyAuth
import PleyCaps
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer
import PleySubtitles
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

func required<T>(_ value: T?, _ what: @autoclosure () -> String) throws -> T {
    guard let value else { throw CheckFailure(what()) }
    return value
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

/// Where the player works (`--work-dir`); `nil`: the app's own Caches.
nonisolated(unsafe) var workDir: URL?

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
                                      },
                                      pictureFolder: workDir?.appendingPathComponent("picture-subtitles",
                                                                                     isDirectory: true))
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

/// (g) A quality picked while file 1 plays: a new session converting to
/// 720 lines takes over from where it got to, and the old one is gone. The
/// server knows this network as its own, so Automatic measured nothing.
@MainActor
func checkQuality(api: PleyAPI, page: AdminPage, caps: DeviceCaps, reporter: SessionReporter, itemId: Int) async throws {
    let run = Run(api: api, caps: caps, reporter: reporter, itemId: itemId)
    do {
        try await run.playing(past: 3_000, seconds: 30)
        let old = try run.session
        try check(old.info.method == .direct, "file 1 plays by \(old.info.method.rawValue), not direct")
        let network = try await api.playbackNetwork(clientId: "apple-play-check")
        try check(network.local, "the server doesn't take this network for its own")
        try check(run.controller.speedTests == 0, "Automatic ran \(run.controller.speedTests) speed test(s) at home")
        run.controller.selectQuality("4m")
        try await wait("the 4 Mbps session", seconds: 60) { run.controller.session.map { $0.id != old.id } ?? false }
        let new = try run.session
        try check(new.info.method == .transcode, "4 Mbps plays by \(new.info.method.rawValue), not transcode")
        // The new session starts from where the old one got to.
        let from = run.controller.state.positionMs
        try check(from >= 3_000, "the new session starts at \(from) ms, not where the old one got to")
        try await run.playing(past: from + 3_000, seconds: 60)
        let videoOut = try await api.playbackStatus(new.id).transcode?.videoOut ?? ""
        try check(videoOut.range(of: #"[x×]720\b"#, options: .regularExpression) != nil,
                  "the conversion makes \(videoOut), not 720 lines")
        try check(run.controller.state.videoOut == videoOut, "Details don't show the conversion")
        do {
            _ = try await api.playbackStatus(old.id)
            throw CheckFailure("the old session still answers after the change")
        } catch PleyError.api(404, _, _) {}
        try await wait("the old session to leave activity", seconds: 15) { try await activity(page, old.id) == nil }
        try check(run.controller.speedTests == 0, "a speed test ran at home")
        say("(g) quality: 4 Mbps took over at \(from / 1000) s as a transcode (\(videoOut)); the old session answers 404 "
            + "and left activity; the server's own network, no speed test")
        await run.controller.close()
    } catch {
        await run.controller.close()
        throw error
    }
}

/// The show's two episodes, by number; their markers asked for and waited for (120 s at most).
@MainActor
func episodes(_ page: AdminPage, api: PleyAPI, showId: Int) async throws -> (first: Int, second: Int) {
    func children(_ id: Int) async throws -> [[String: Any]] {
        let (status, body) = try await page.send("GET", "/api/items/\(id)/children")
        try check(status == 200, "the children of item \(id) answered HTTP \(status)")
        return body["items"] as? [[String: Any]] ?? []
    }
    guard let season = try await children(showId).first?["id"] as? Int else { throw CheckFailure("the show has no season") }
    let found = try await children(season).compactMap { $0["id"] as? Int }
    try check(found.count == 2, "the season has \(found.count) episode(s), not 2")
    let (status, _) = try await page.send("POST", "/api/items/\(showId)/analyze")
    try check(status == 202, "asking for the show's markers answered HTTP \(status)")
    try await wait("the episodes' intro and credits", seconds: 120) {
        for id in found {
            let kinds = Set(try await api.item(id).markers.map(\.kind))
            if !kinds.isSuperset(of: [.intro, .credits]) { return false }
        }
        return true
    }
    return (found[0], found[1])
}

/// (f) S01E01: Skip Intro lands past the intro; entering the credits brings
/// up the next episode; Play Next ends this session (watched at 90 %) and
/// plays S01E02.
@MainActor
func checkEpisodes(api: PleyAPI, page: AdminPage, caps: DeviceCaps, reporter: SessionReporter,
                   episodes: (first: Int, second: Int)) async throws {
    let first = try await api.item(episodes.first)
    let intro = try required(first.markers.first { $0.kind == .intro }, "S01E01 has no intro marker")
    let credits = try required(first.markers.first { $0.kind == .credits }, "S01E01 has no credits marker")
    try check(abs(intro.startMs - 10_000) <= 1_000 && abs(intro.endMs - 30_000) <= 1_000,
              "the intro runs \(intro.startMs)–\(intro.endMs) ms, not as its chapter")
    try check(abs(credits.startMs - 55_000) <= 1_000, "the credits start at \(credits.startMs) ms, not as their chapter")
    let run = Run(api: api, caps: caps, reporter: reporter, itemId: episodes.first)
    do {
        try await run.playing(past: 11_000, seconds: 60)
        try check(run.controller.state.skipLabel == "Skip Intro",
                  "no Skip Intro at \(run.sample.positionMs) ms (\(run.controller.state.skipLabel ?? "nothing"))")
        run.controller.skip()
        let target = Markers.skipTarget(intro)
        try await wait("the skip", seconds: 10) { run.sample.positionMs >= target }
        let landed = run.sample.positionMs
        try check(landed >= 29_250, "Skip Intro landed at \(landed) ms")
        try check(run.controller.session?.info.method == .remux, "S01E01 isn't a remux")
        let old = try run.session

        run.controller.seek(toMs: credits.startMs - 1_500)
        try await wait("Up Next in the credits", seconds: 15) { run.controller.state.upNext != nil }
        let card = try required(run.controller.state.upNext, "no card")
        try check(card.itemId == episodes.second, "Up Next shows item \(card.itemId), not S01E02")
        try check(card.heading == "Up next · S1 E2", "the card says \(card.heading)")
        try check(run.controller.state.skipLabel == "Skip Credits", "no Skip Credits in the credits")
        // The server marks it watched at 90 %: past it, then Play Next.
        let past = Int(Double(first.durationMs ?? 75_000) * 0.9) + 1_000
        run.controller.seek(toMs: past)
        try await wait("the time past 90 %", seconds: 20) { run.sample.positionMs > past }
        try check(run.controller.state.upNext != nil, "the card went while the credits play")
        run.controller.playNext()
        try await wait("S01E02 playing", seconds: 60) {
            run.controller.state.itemId == episodes.second && run.controller.session.map { $0.id != old.id } ?? false
                && (run.sample.videoFrames ?? 0) > 0 && run.controller.state.phase == .playing
        }
        try check(try await api.item(episodes.first).userState.watched, "S01E01 isn't marked watched")
        do {
            _ = try await api.playbackStatus(old.id)
            throw CheckFailure("S01E01's session still answers after Play Next")
        } catch PleyError.api(404, _, _) {}
        try await wait("S01E01's session to leave activity", seconds: 15) { try await activity(page, old.id) == nil }
        let shown = try await activity(page, try run.session.id)
        try check(shown != nil, "S01E02's session isn't in activity")
        say("(f) episodes: Skip Intro landed at \(landed) ms; Up Next showed S1 E2 in the credits; Play Next marked "
            + "S01E01 watched, its session answers 404, and S01E02 plays (\(run.describe()))")
        await run.controller.close()
    } catch {
        await run.controller.close()
        throw error
    }
}

/// (h) File 2's French AC-3 picked while it plays as it is: AVPlayer
/// switches to it in the same session, and the server keeps the choice.
@MainActor
func checkAudio(api: PleyAPI, caps: DeviceCaps, reporter: SessionReporter, itemId: Int) async throws {
    let run = Run(api: api, caps: caps, reporter: reporter, itemId: itemId)
    do {
        try await run.playing(past: 3_000, seconds: 30)
        let session = try run.session
        try check(session.info.method == .direct, "file 2 plays by \(session.info.method.rawValue), not direct")
        let audio = run.controller.state.audio
        try check(audio.count == 2, "the Audio menu has \(audio.count) track(s), not 2")
        try check(audio.first?.selected == true, "the first track isn't the one playing")
        try check(run.engine?.audioOptions().count == 2, "AVPlayer lists \(run.engine?.audioOptions().count ?? 0) audio options")
        let file = try required(run.controller.file, "no file")
        let french = try required(file.streams(.audio).first { $0.codec == "ac3" }, "file 2 has no AC-3 track")
        run.controller.selectAudio(french.index)
        try await wait("the choice kept by the server", seconds: 15) {
            let saved = try await api.item(itemId).files.first
            return saved?.selectedAudio == french.index && saved?.audioPicked == true
        }
        let position = run.sample.positionMs
        try await run.playing(past: position + 2_000, seconds: 15)
        try check(run.controller.session?.id == session.id, "the audio change started a new session")
        try check(run.controller.state.audio.last?.selected == true, "the menu doesn't mark the AC-3 track")
        let item = try required(run.engine?.player.currentItem, "no item")
        let group = try required(try await item.asset.loadMediaSelectionGroup(for: .audible), "no audible group")
        let selected = item.currentMediaSelection.selectedMediaOption(in: group)
        try check(selected == group.options.last, "AVPlayer doesn't play the second audio track")
        say("(h) audio: the AC-3 track plays in the same session (\(short(session.id))…); the server keeps "
            + "selected_audio \(french.index), picked")
        await run.controller.close()
    } catch {
        await run.controller.close()
        throw error
    }
}

/// (i) Subtitles drawn by the app: file 1's SRT beside it, from the
/// server's WebVTT, while it plays as it is; file 3's PGS from the
/// server's `.mks`, while the session stays a remux (nothing burned in).
/// Each is paused inside a cue, so the check waits for the server however
/// long it takes to read the track.
@MainActor
func checkSubtitles(api: PleyAPI, caps: DeviceCaps, reporter: SessionReporter, direct: Int, remux: Int) async throws {
    var run = Run(api: api, caps: caps, reporter: reporter, itemId: direct)
    do {
        try await run.playing(past: 3_000, seconds: 30)
        let session = try run.session
        let file = try required(run.controller.file, "no file")
        let beside = try required(file.streams(.subtitle).first { $0.external },
                                  "file 1 has no subtitle file beside it")
        run.controller.selectSubtitle(beside.index)
        run.controller.seek(toMs: 8_500)
        run.controller.pause()
        try await wait("the second line", seconds: 30) {
            guard case .text(let cues) = run.controller.state.cues else { return false }
            return cues.map(\.plain) == ["The second line."]
        }
        try check(run.controller.session?.id == session.id, "the subtitles started a new session")
        try check(session.info.method == .direct, "file 1 plays by \(session.info.method.rawValue), not direct")
        say("(i) text: track \(beside.index) shows \"The second line.\" at 8.5 s, drawn by the app; the file plays "
            + "as it is")
        await run.controller.close()
    } catch {
        await run.controller.close()
        throw error
    }

    run = Run(api: api, caps: caps, reporter: reporter, itemId: remux)
    do {
        try await run.playing(past: 3_000, seconds: 60)
        let session = try run.session
        try check(session.info.method == .remux, "file 3 plays by \(session.info.method.rawValue), not remux")
        let file = try required(run.controller.file, "no file")
        let pgs = try required(file.streams(.subtitle).first { $0.codec == "hdmv_pgs_subtitle" }, "file 3 has no PGS")
        run.controller.selectSubtitle(pgs.index)
        run.controller.seek(toMs: 12_500)
        run.controller.pause()
        var drawn: PictureFrame?
        try await wait("the PGS picture", seconds: 60) {
            if case .failed(let message, _) = run.controller.state.phase {
                throw CheckFailure("playback failed: \(message)")
            }
            guard case .picture(let shown) = run.controller.state.cues, !shown.frame.bitmaps.isEmpty else {
                return false
            }
            drawn = shown.frame
            return true
        }
        let frame = try required(drawn, "no picture")
        let box = try required(frame.bitmaps.first, "no bitmap")
        try check(frame.planeWidth == 1920 && frame.planeHeight == 1080,
                  "the picture's plane is \(frame.planeWidth)×\(frame.planeHeight)")
        try check(box.width == 480 && box.height == 96, "the picture is \(box.width)×\(box.height), not 480×96")
        let now = try run.session
        try check(now.id == session.id, "the PGS started a new session")
        try check(now.info.method == .remux && !now.info.burnSubtitles, "the PGS is burned in")
        try check(!run.controller.state.subtitlesBurnedIn, "the player says the PGS is burned in")
        say("(i) pictures: the PGS (track \(pgs.index)) shows a \(box.width)×\(box.height) picture at 12.5 s from "
            + "the server's .mks; the session stays a remux, nothing burned in")
        await run.controller.close()
    } catch {
        await run.controller.close()
        throw error
    }
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
    static let opus = "Opus audio not supported."
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
        // Streams copy only AAC, MP3, AC-3 and E-AC-3: Opus and FLAC become AAC.
        // iPadOS 16 doesn't report Opus, so it is told why.
        case "Opus":
            Cell(method: .remux, audioConverted: true, reasons: device == "ipad_5th_gen" ? [mkv, opus] : [mkv])
        case "FLAC": Cell(method: .remux, audioConverted: true, reasons: [mkv])
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
    let files = ["Direct H264", "Remux H264 AC3", "HEVC HDR10", "MPEG2", "DTS", "AV1", "Opus", "FLAC"]
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

// MARK: (l) Live TV

/// The channel named `name`'s id, from All channels.
func channelId(_ page: AdminPage, _ name: String) async throws -> Int {
    let query = name.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? name
    let (status, body) = try await page.send("GET", "/api/live/browse?category=all&q=\(query)&limit=20")
    try check(status == 200, "browsing Live TV answered HTTP \(status)")
    let channels = body["channels"] as? [[String: Any]] ?? []
    return try required(channels.first { $0["name"] as? String == name }?["id"] as? Int, "no channel \(name)")
}

/// How many sessions watch any channel, for everyone.
func liveViewers(_ page: AdminPage) async throws -> Int {
    let (status, data) = try await page.raw("GET", "/api/live/tuners")
    try check(status == 200, "the tuners answered HTTP \(status)")
    let tuners = (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] ?? []
    return tuners.reduce(0) { $0 + (($1["viewers"] as? [Any])?.count ?? 0) }
}

/// What the keyed playlist at `url` answers now (the address itself is never printed).
func status(of url: URL) async -> Int {
    guard let (_, response) = try? await URLSession.shared.data(from: url) else { return 0 }
    return (response as? HTTPURLResponse)?.statusCode ?? 0
}

/// live-source.sh's channels (one connection at a time) through the real
/// live player: a channel tunes and plays, the next takes its place and
/// the first session ends, and leaving ends the second within 2 seconds.
@MainActor
func checkLive(api: PleyAPI, page: AdminPage, source: String) async throws {
    let (made, _) = try await page.send("POST", "/api/live/sources", json: [
        "name": "Test channels", "kind": "m3u", "url": source + "/list.m3u", "guide_url": source + "/guide.xml",
        "max_connections": 1,
    ])
    try check(made == 200 || made == 201, "the live source wasn't added (HTTP \(made))")
    try await wait("Live TV's four channels", seconds: 120) {
        let (_, live) = try await page.send("GET", "/api/live")
        return live["available"] as? Bool == true && (live["channels"] as? Int ?? 0) >= 4
    }
    let one = try await channelId(page, "Test One")
    let two = try await channelId(page, "Test Two")

    let playback = LivePlayback(live: LiveTvRepository(api: api), clientId: "apple-play-check")
    defer { playback.close() }
    func playing(_ name: String) async throws {
        try await wait("\(name) to play", seconds: 60) {
            if case .failed(let trouble) = playback.state.phase {
                throw CheckFailure("\(name) failed: \(trouble.message)")
            }
            let player = (playback.engine as? AVLiveEngine)?.player
            return playback.state.phase == .playing && player?.timeControlStatus == .playing
        }
    }
    func playlist() throws -> URL {
        let item = (playback.engine as? AVLiveEngine)?.player.currentItem
        return try required((item?.asset as? AVURLAsset)?.url, "the live player has no playlist")
    }

    playback.tune(one)
    try await playing("Test One")
    let first = try playlist()
    let firstSession = try required(playback.sessionId, "Test One has no session")
    say("(l) Live TV: Test One plays, session \(short(firstSession))")

    playback.tune(two)
    try await playing("Test Two")
    let secondSession = try required(playback.sessionId, "Test Two has no session")
    try check(secondSession != firstSession, "Test Two plays in Test One's session")
    try await wait("Test One's playlist to end", seconds: 10) { [404, 410].contains(await status(of: first)) }
    try check(try await liveViewers(page) == 1, "more than one session watches after the zap")
    say("(l) Live TV: Test Two takes its place, session \(short(secondSession)); Test One's playlist answers"
        + " \(await status(of: first))")

    playback.close()
    try await wait("no one to watch", seconds: 2) { try await liveViewers(page) == 0 }
    say("(l) Live TV: leaving ends the session within 2 s")
}

/// (m) Music: the album's tracks through `MusicPlayback` and the AVPlayer
/// output, one session each, the one before gone at a track's end, and the
/// MP2 track converted.
@MainActor
func checkMusic(api: PleyAPI, page: AdminPage, caps: DeviceCaps, reporter: SessionReporter, artistId: Int) async throws {
    let album = try required(try await api.children(artistId).first { $0.kind == .album }, "the artist has no album")
    let tracks = try await api.children(album.id)
    try check(tracks.map(\.title) == ["First", "Second", "Third"],
              "the album's tracks are \(tracks.map(\.title)), not First, Second and Third")
    let music = MusicPlayback(environment: MusicEnvironment(
        api: { api }, caps: { await caps.warmUp().report }, clientId: "apple-play-check-music", reporter: reporter))
    defer { music.stop() }
    func playing(_ title: String, past ms: Int) async throws {
        try await wait("\(title) to play past \(ms / 1000) s", seconds: 60) {
            if let error = music.state.error { throw CheckFailure("\(title): \(error.text)") }
            return music.state.track?.title == title && music.state.playing && music.positionMs > ms
        }
    }
    func session() throws -> ServerPlaybackSession { try required(music.session, "music has no session") }

    music.play(tracks: tracks)
    try await playing("First", past: 1_000)
    let first = try session()
    try check(first.info.method == .direct, "First plays by \(first.info.method.rawValue), not direct")
    try check(try await activity(page, first.id) != nil, "the dashboard doesn't show First's session")
    say("(m) music: First plays as it is, session \(short(first.id)) on the dashboard")

    // Near its end, First runs out by itself and Second follows in a session of its own.
    music.seek(toMs: max(music.state.durationMs - 2_000, 0))
    try await playing("Second", past: 500)
    let second = try session()
    try check(second.id != first.id, "Second plays in First's session")
    try check(second.info.method == .direct, "Second plays by \(second.info.method.rawValue), not direct")
    try await wait("First's session to end", seconds: 5) { try await activity(page, first.id) == nil }
    say("(m) music: at First's end, its session ended and Second's \(short(second.id)) started")

    music.next()
    try await playing("Third", past: 1_000)
    let third = try session()
    try check(third.info.method != .direct, "Third (MP2) plays as it is, not converted")
    try await wait("Second's session to end", seconds: 5) { try await activity(page, second.id) == nil }
    say("(m) music: Third (MP2) plays by \(third.info.method.rawValue)")

    music.stop()
    await reporter.flush()
    try await wait("music's session to end", seconds: 5) { try await activity(page, third.id) == nil }
    say("(m) music: stopping ends the session")
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
        throw CheckFailure("usage: pley-apple-play --server <origin> --admin <username> --media <dir> --caps-fixtures <dir>"
            + " [--work-dir <dir>] [--live-source <origin>]")
    }
    workDir = options["work-dir"].map { URL(fileURLWithPath: $0, isDirectory: true) }
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
    let movies = try await library(page, name: "Movies", kind: "movies", path: media + "/Movies", count: 10)
    let shows = try await library(page, name: "Shows", kind: "shows", path: media + "/Shows", count: 1)
    let artists = try await library(page, name: "Music", kind: "music", path: media + "/Music", count: 1)
    say("libraries: \(movies.count) movies, \(shows.count) show(s), \(artists.count) artist(s)")
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

    try await checkQuality(api: api, page: page, caps: caps, reporter: reporter, itemId: try item("Direct H264"))

    guard let showId = shows["Pley Test Show"] else { throw CheckFailure("no show Pley Test Show") }
    let pair = try await episodes(page, api: api, showId: showId)
    try await checkEpisodes(api: api, page: page, caps: caps, reporter: reporter, episodes: pair)

    try await checkAudio(api: api, caps: caps, reporter: reporter, itemId: try item("Two Audio"))

    try await play("(b) remux", api: api, caps: caps, reporter: reporter, itemId: try item("Remux H264 AC3")) { _, session in
        try check(session.info.method == .remux, "file 3 plays by \(session.info.method.rawValue), not remux")
        let (_, media) = try await playlists(api, session.info)
        try check(media.contains(#"#EXT-X-MAP:URI="init.mp4""#), "the media playlist has no fMP4 init segment")
        try check(media.contains(".m4s"), "the media playlist has no .m4s segments")
        // No captions in the frames: AVKit offers no captions menu beside the app's subtitles.
        let (master, _) = try await playlists(api, session.info)
        try check(master.contains("CLOSED-CAPTIONS=NONE"), "the master playlist doesn't say CLOSED-CAPTIONS=NONE")
        return "fMP4 segments (init.mp4, .m4s), CLOSED-CAPTIONS=NONE"
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

    // Opus and FLAC in Matroska: the video copied, the audio made AAC.
    for name in ["Opus", "FLAC"] {
        try await play("(k) \(name) MKV", api: api, caps: caps, reporter: reporter, itemId: try item(name)) { _, session in
            try check(session.info.method == .remux, "the \(name) MKV plays by \(session.info.method.rawValue), not remux")
            try check(session.info.audioConverted, "the \(name) MKV's audio isn't converted")
            return "audio converted"
        }
    }
    // Opus in MP4 plays as it is where AVPlayer decodes it (this Mac's macOS does).
    try await play("(k) Opus MP4", api: api, caps: caps, reporter: reporter, itemId: try item("Opus MP4")) { run, session in
        try check(session.info.method == .direct, "the Opus MP4 plays by \(session.info.method.rawValue), not direct")
        try check(run.engine?.player.currentItem?.tracks.contains { $0.assetTrack?.mediaType == .audio } == true,
                  "the Opus MP4 plays without its audio")
        return "Opus decoded here"
    }

    try await checkSubtitles(api: api, caps: caps, reporter: reporter, direct: try item("Direct H264"),
                             remux: try item("Remux H264 AC3"))

    try await checkMusic(api: api, page: page, caps: caps, reporter: reporter,
                         artistId: try required(artists["Test Artist"], "no artist Test Artist in Music"))

    if let source = options["live-source"] {
        try await checkLive(api: api, page: page, source: source)
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
