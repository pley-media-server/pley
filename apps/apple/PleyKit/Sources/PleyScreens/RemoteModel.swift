import Combine
import Foundation
import PleyModel
import PleyNetwork

/// The remote on iPhone, iPad and Mac (docs/APPLE.md, Remote control):
/// the profile's other devices that are on, the ones that take casts
/// (`GET /api/remote/devices`) with what plays on each (`GET /api/sessions`),
/// merged by `device_id` and read again whenever `AppModel.sessionsVersion`
/// goes up; the commands to their players, and what "Play on…" sends them.
/// This device itself is left out: the sessions it plays, its own
/// receiver, and whatever else has its `device_id`. Apple TV only obeys,
/// so it has none.
@MainActor
public final class RemoteModel: ObservableObject {
    /// A session as the remote keeps it, with when its player last
    /// reported by this device's clock: the server says how long ago each
    /// report was rather than when, so the two clocks are never compared.
    public struct Entry: Identifiable, Hashable, Sendable {
        public let session: PlayingSession
        public let reportedAt: Date

        public init(_ session: PlayingSession, received: Date) {
            self.session = session
            reportedAt = received.addingTimeInterval(-Double(session.ageMs) / 1000)
        }

        public var id: String { session.sessionId }
        public var live: Bool { session.kind == .live }
    }

    /// One of the profile's other devices: where casts go (its receiver;
    /// `nil` for one that only plays, a Jellyfin or a Plex app), and what
    /// it plays now (its newest session; `nil` while it is ready to play).
    public struct Device: Identifiable, Hashable, Sendable {
        /// Its `device_id`.
        public let id: String
        public let receiver: RemoteDevice?
        public let entry: Entry?

        public init(id: String, receiver: RemoteDevice?, entry: Entry?) {
            self.id = id
            self.receiver = receiver
            self.entry = entry
        }

        /// What kind of device it is, as its receiver says, else its session.
        public var kind: DeviceKind { receiver?.kind ?? entry?.session.deviceKind ?? .other }
        /// Its name as the server gives it.
        public var given: String? { receiver?.name ?? entry?.session.device }
        /// Whether "Play on…" sends it what is of `kind`.
        public func takes(_ kind: CastKind) -> Bool { receiver?.takes(kind) ?? false }
    }

    /// A command sent, and what its session looked like then.
    struct Sent {
        let command: Control
        let before: Entry
        let sentAt: Date
        /// `seek`, `skip` and a `previous` that starts the title over: where it should be once the player acted.
        let target: Int?
        /// `audio` and `subtitles`: the track it should play once it acted.
        var stream: Int?
        /// Which send this was: a check judges only its own command, never one sent after it.
        var token = 0
    }

    /// How long the remote waits for a player to show it acted on a command.
    public static let answerWait: Duration = .seconds(5)
    /// Farther than this from where it should be, a report has moved (the server's rule for a seek).
    static let jumpMs = 5_000

    /// The profile's sessions, as last read, this device's included.
    @Published public private(set) var entries: [Entry] = []
    /// The profile's devices that take casts, as last read, this one included.
    @Published public private(set) var receivers: [RemoteDevice] = []
    /// The sessions this device plays now.
    @Published public private(set) var here: Set<String> = []
    /// The sessions with a command on its way, or waiting for its player to show it acted.
    @Published public private(set) var waiting: Set<String> = []
    /// The sessions with a stop on its way.
    @Published public private(set) var stopping: Set<String> = []
    /// What went wrong with each session's last command, as a sentence.
    @Published public private(set) var errors: [String: String] = [:]
    /// The remote shows: the iPhone's and iPad's sheet, the Mac's popover.
    /// Closed, the device chosen is forgotten.
    @Published public var presented = false {
        didSet { if !presented { chosen = nil } }
    }
    /// The device whose controls show, by id, while the remote shows: on a
    /// phone its own page, elsewhere below the list.
    @Published public var chosen: String?

    public let app: AppModel
    private let wait: Duration
    private var sent: [String: Sent] = [:]
    private var checks: [String: Task<Void, Never>] = [:]
    /// The last send to each session: one answered after a newer one went leaves the row to it.
    private var latest: [String: Int] = [:]
    private var sends = 0
    /// Each reading of the list: an answer older than the last one shown is dropped.
    private var reading = 0
    private var shown = 0
    private var watching: Set<AnyCancellable> = []

    public init(app: AppModel, answerWait: Duration = RemoteModel.answerWait) {
        self.app = app
        wait = answerWait
        here = app.sessionsHere
        // The players say which sessions they play as those start and end;
        // `@Published` tells before it sets, so the set is read just after.
        let player = app.$player.map { player -> AnyPublisher<Void, Never> in
            player.map { $0.$state.map { _ in () }.eraseToAnyPublisher() } ?? Just(()).eraseToAnyPublisher()
        }.switchToLatest()
        let live = app.$livePlayer.map { live -> AnyPublisher<Void, Never> in
            live.map { $0.playback.$state.map { _ in () }.eraseToAnyPublisher() } ?? Just(()).eraseToAnyPublisher()
        }.switchToLatest()
        Publishers.Merge3(app.music.$state.map { _ in () }, player, live)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] in self?.followHere() }
            .store(in: &watching)
        // Another profile has other devices.
        app.$user.map { $0?.id }.removeDuplicates().dropFirst()
            .sink { [weak self] _ in self?.forget() }
            .store(in: &watching)
    }

    /// The sessions on the profile's other devices: what this remote controls.
    public var others: [Entry] { entries.filter { !here.contains($0.id) } }

    /// The profile's other devices that are on: those playing first, then
    /// those ready to play, each by name. A device's receiver and its
    /// sessions share its `device_id`; one with several sessions shows its
    /// newest, and one whose app takes no casts shows all the same.
    public var devices: [Device] {
        let own = app.receiverId
        // This device's id, as its own receiver says it: a session of it that isn't playing here any more
        // (its end not heard yet) is this device's all the same.
        let ownDevice = receivers.first { $0.receiverId == own }.map { Self.deviceId($0) }
        var receiverOf: [String: RemoteDevice] = [:]
        var order: [String] = []
        for receiver in receivers where receiver.receiverId != own {
            let id = Self.deviceId(receiver)
            guard id != ownDevice, receiverOf[id] == nil else { continue }
            receiverOf[id] = receiver
            order.append(id)
        }
        var newest: [String: Entry] = [:]
        // Oldest first: a later session of the same device is newer.
        for entry in others {
            let id = Self.deviceId(entry.session)
            guard id != ownDevice else { continue }
            if newest[id] == nil && receiverOf[id] == nil { order.append(id) }
            newest[id] = entry
        }
        let all = order.map { Device(id: $0, receiver: receiverOf[$0], entry: newest[$0]) }
        let names = Self.names(all)
        return all.sorted { a, b in
            if (a.entry != nil) != (b.entry != nil) { return a.entry != nil }
            let byName = (names[a.id] ?? "").localizedCaseInsensitiveCompare(names[b.id] ?? "")
            return byName == .orderedSame ? a.id < b.id : byName == .orderedAscending
        }
    }

    /// What the remote calls each device, by id.
    public var names: [String: String] { Self.names(devices) }

    public func name(_ device: Device) -> String { names[device.id] ?? device.given ?? Words.unnamedDevice }

    /// The name of the device playing `entry`.
    public func name(_ entry: Entry) -> String {
        names[Self.deviceId(entry.session)] ?? entry.session.device ?? Words.unnamedDevice
    }

    /// The device whose controls show: the one chosen, else the only one.
    public var selected: Device? {
        let devices = devices
        if devices.count == 1 { return devices[0] }
        return chosen.flatMap { id in devices.first { $0.id == id } }
    }

    /// The devices "Play on…" offers for what is of `kind`, as the list orders them.
    public func takers(_ kind: CastKind) -> [Device] { devices.filter { $0.takes(kind) } }

    /// The remote, open on the device of `id`.
    public func show(_ id: String) {
        presented = true
        chosen = id
    }

    /// The pictures for what `session` plays, tried in turn: a track's own,
    /// else its album's; a title's poster, else its still (an episode's).
    public static func artPaths(_ session: PlayingSession) -> [String] {
        guard let item = session.item else { return [] }
        let paths = item.kind == .track
            ? [item.posterUrl, item.parentId.map { "/api/items/\($0)/images/poster" }]
            : [item.posterUrl, item.thumbUrl]
        return paths.compactMap { $0 }.filter { !$0.isEmpty }
    }

    /// Reads the list again: the sessions and the devices that take casts.
    /// Without an answer it stays as it was; a server without the devices'
    /// route (an older one) has none, and its sessions show as before.
    @discardableResult
    public func reload() async -> Bool {
        guard app.user != nil, let api = await app.session.api else { return false }
        reading += 1
        let asked = reading
        async let listed = try? await api.playingSessions()
        async let castable = try? await api.remoteDevices()
        guard let list = await listed else { return false }
        let found = await castable
        guard asked > shown else { return true }
        shown = asked
        let now = Date()
        entries = list.map { Entry($0, received: now) }
        if let found { receivers = found }
        followHere()
        // The list shows the change (the player's report brings `sessions_changed`): it answered.
        for (id, command) in sent where Self.answered(command, entries.first { $0.id == id }, at: now) {
            settle(id)
        }
        return true
    }

    /// Where `entry` is at `now`: its last report, moved on by the time
    /// since while it plays (never past its end).
    public static func position(_ entry: Entry, at now: Date) -> Int {
        let session = entry.session
        guard session.state == .playing else { return session.positionMs }
        let at = session.positionMs + max(0, Int((now.timeIntervalSince(entry.reportedAt) * 1000).rounded()))
        return session.durationMs.map { min(at, $0) } ?? at
    }

    /// Sends `entry`'s player `command`. Its player acts on it and reports,
    /// which brings `sessions_changed`: the new state shows from there.
    /// Without a change in `answerWait`, the remote says it didn't answer.
    public func send(_ command: PlayerCommand, to entry: Entry) async {
        let id = entry.id
        let device = name(entry)
        errors[id] = nil
        checks[id]?.cancel()
        sent[id] = nil
        waiting.insert(id)
        sends += 1
        let token = sends
        latest[id] = token
        let sentAt = Date()
        let at = Self.position(entry, at: sentAt)
        // A previous a few seconds in starts the title over, as players do.
        let target = command.positionMs
            ?? command.offsetMs.map { max(0, min(at + $0, entry.session.durationMs ?? Int.max)) }
            ?? (command.command == .previous && !entry.live ? 0 : nil)
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            try await api.send(command, to: id)
            guard latest[id] == token else { return }
            sent[id] = Sent(command: command.command, before: entry, sentAt: sentAt, target: target,
                            stream: command.streamIndex, token: token)
            checks[id]?.cancel()
            checks[id] = Task { [weak self, wait] in
                try? await Task.sleep(for: wait)
                guard !Task.isCancelled else { return }
                await self?.check(id, token: token, device: device)
            }
        } catch {
            guard latest[id] == token else { return }
            waiting.remove(id)
            refused(error, id: id, device: device)
        }
    }

    /// Stops `entry`'s player, as every player obeys.
    public func stop(_ entry: Entry) async {
        let id = entry.id
        let device = name(entry)
        errors[id] = nil
        stopping.insert(id)
        defer { stopping.remove(id) }
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            try await api.stopSession(id)
            drop(id)
        } catch {
            refused(error, id: id, device: device)
        }
    }

    /// Sends `device` `target` to play ("Play on…"): "Playing on Living
    /// room TV" says it went, and the remote opens on that device, whose
    /// controls show once its session does. A refusal says why, in the
    /// remote's words.
    public func cast(_ target: CastTarget, to device: Device) async {
        guard let receiver = device.receiver else { return }
        let name = name(device)
        do {
            guard let api = await app.session.api else { throw PleyError.signedOut }
            try await api.cast(target, to: receiver.receiverId)
            app.shell.show(Words.playingOn(name))
            show(device.id)
        } catch {
            let status: Int? = switch error as? PleyError {
            case .api(let status, _, _)?, .refused(let status, _, _, _, _)?: status
            default: nil
            }
            let words = status.flatMap { Words.castRefusal(status: $0, device: name) } ?? asSentence(describe(error))
            app.shell.show(words, action: nil, error: true)
            // Gone, or no longer taking that: the list is read again.
            if status == 404 || status == 409 { Task { await reload() } }
        }
    }

    /// Whether the list shows the player acted on `sent`, by `now`: it is
    /// gone (next and previous end a session to start the next), plays
    /// something else, plays the track sent (audio, subtitles), changed
    /// state, or, for a seek, a skip or a previous on the same title, has
    /// reported since from somewhere new or where it was sent to. Server
    /// times are only compared with server times, and this device's with
    /// its own.
    static func answered(_ sent: Sent, _ now: Entry?, at time: Date) -> Bool {
        let before = sent.before.session
        guard let now else { return true }
        let session = now.session
        if session.item?.id != before.item?.id || session.channel?.id != before.channel?.id { return true }
        // A track is answered by the track it plays, whatever else changed meanwhile.
        if sent.command == .audio { return sent.stream != nil && session.audioIndex == sent.stream }
        if sent.command == .subtitles { return sent.stream != nil && session.subtitleIndex == sent.stream }
        if session.state != before.state { return true }
        guard [.seek, .skip, .previous].contains(sent.command) else { return false }
        let reportedSince = session.updatedAt > before.updatedAt || session.positionMs != before.positionMs
        guard reportedSince else { return false }
        let moved = abs(session.positionMs - position(sent.before, at: now.reportedAt)) > jumpMs
        // It may have played on from the target since, for as long as the command has been out.
        let out = max(0, Int(time.timeIntervalSince(sent.sentAt) * 1000))
        let there = sent.target.map { abs(session.positionMs - $0) <= jumpMs + out } ?? false
        return moved || there
    }

    // MARK: Private

    /// A device's id: its `device_id`, else (a server that doesn't say) one of its own.
    private static func deviceId(_ receiver: RemoteDevice) -> String {
        receiver.deviceId.isEmpty ? "receiver:" + receiver.receiverId : receiver.deviceId
    }

    private static func deviceId(_ session: PlayingSession) -> String {
        session.deviceId.isEmpty ? "session:" + session.sessionId : session.deviceId
    }

    private static func names(_ devices: [Device]) -> [String: String] {
        Words.deviceNames(devices.map { (id: $0.id, name: $0.given) })
    }

    /// Not answered yet after a while: the server is asked once more before the remote says so.
    private func check(_ id: String, token: Int, device: String) async {
        guard let command = sent[id], command.token == token else { return }
        let read = await reload()
        guard sent[id]?.token == token else { return }
        settle(id)
        // A player with nothing next or before stays where it is, as it should.
        guard read, command.command != .next, command.command != .previous,
              !Self.answered(command, entries.first { $0.id == id }, at: Date())
        else { return }
        errors[id] = Words.remoteNoAnswer(device)
    }

    private func settle(_ id: String) {
        sent[id] = nil
        checks[id]?.cancel()
        checks[id] = nil
        waiting.remove(id)
    }

    /// The server's refusal in the remote's words. A session gone is
    /// dropped; one whose player no longer obeys that is read again.
    private func refused(_ error: any Error, id: String, device: String) {
        let status: Int? = switch error as? PleyError {
        case .api(let status, _, _)?, .refused(let status, _, _, _, _)?: status
        default: nil
        }
        errors[id] = status.flatMap { Words.remoteRefusal(status: $0, device: device) } ?? asSentence(describe(error))
        if status == 404 {
            drop(id)
        } else if status == 409 {
            Task { await reload() }
        }
    }

    /// A session gone from the list until it is read again.
    private func drop(_ id: String) {
        entries.removeAll { $0.id == id }
        settle(id)
        Task { await reload() }
    }

    private func followHere() {
        let now = app.sessionsHere
        if now != here { here = now }
    }

    private func forget() {
        // A reading still on its way was for the profile before: its answer is dropped.
        shown = reading
        for check in checks.values { check.cancel() }
        checks = [:]
        sent = [:]
        latest = [:]
        entries = []
        receivers = []
        waiting = []
        stopping = []
        errors = [:]
        presented = false
    }
}
