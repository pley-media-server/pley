import Combine
import Foundation
import PleyModel
import PleyNetwork

/// The remote on iPhone, iPad and Mac (docs/APPLE.md, Remote control):
/// what the profile plays on its other devices (`GET /api/sessions`, read
/// again whenever `AppModel.sessionsVersion` goes up), and the commands to
/// them. The sessions this device plays itself are left out. Apple TV only
/// obeys, so it has none.
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

    /// A command sent, and what its session looked like then.
    struct Sent {
        let command: Control
        let before: Entry
        let sentAt: Date
        /// `seek`, `skip` and a `previous` that starts the title over: where it should be once the player acted.
        let target: Int?
        /// Which send this was: a check judges only its own command, never one sent after it.
        var token = 0
    }

    /// How long the remote waits for a player to show it acted on a command.
    public static let answerWait: Duration = .seconds(5)
    /// Farther than this from where it should be, a report has moved (the server's rule for a seek).
    static let jumpMs = 5_000

    /// The profile's sessions, as last read, this device's included.
    @Published public private(set) var entries: [Entry] = []
    /// The sessions this device plays now.
    @Published public private(set) var here: Set<String> = []
    /// The sessions with a command on its way, or waiting for its player to show it acted.
    @Published public private(set) var waiting: Set<String> = []
    /// The sessions with a stop on its way.
    @Published public private(set) var stopping: Set<String> = []
    /// What went wrong with each session's last command, as a sentence.
    @Published public private(set) var errors: [String: String] = [:]

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

    /// What the remote calls each device, by session id.
    public var names: [String: String] { Words.deviceNames(others.map(\.session)) }

    public func name(_ entry: Entry) -> String { names[entry.id] ?? entry.session.device ?? Words.unnamedDevice }

    /// The pictures for what `session` plays, tried in turn: a track's own,
    /// else its album's; a title's poster, else its still (an episode's).
    public static func artPaths(_ session: PlayingSession) -> [String] {
        guard let item = session.item else { return [] }
        let paths = item.kind == .track
            ? [item.posterUrl, item.parentId.map { "/api/items/\($0)/images/poster" }]
            : [item.posterUrl, item.thumbUrl]
        return paths.compactMap { $0 }.filter { !$0.isEmpty }
    }

    /// Reads the list again. Without an answer it stays as it was.
    @discardableResult
    public func reload() async -> Bool {
        guard app.user != nil, let api = await app.session.api else { return false }
        reading += 1
        let asked = reading
        guard let list = try? await api.playingSessions() else { return false }
        guard asked > shown else { return true }
        shown = asked
        let now = Date()
        entries = list.map { Entry($0, received: now) }
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
            sent[id] = Sent(command: command.command, before: entry, sentAt: sentAt, target: target, token: token)
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

    /// Whether the list shows the player acted on `sent`, by `now`: it is
    /// gone (next and previous end a session to start the next), plays
    /// something else, changed state, or, for a seek, a skip or a previous
    /// on the same title, has reported since from somewhere new or where it
    /// was sent to. Server times are only compared with server times, and
    /// this device's with its own.
    static func answered(_ sent: Sent, _ now: Entry?, at time: Date) -> Bool {
        let before = sent.before.session
        guard let now else { return true }
        let session = now.session
        if session.item?.id != before.item?.id || session.channel?.id != before.channel?.id { return true }
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
        waiting = []
        stopping = []
        errors = [:]
    }
}
