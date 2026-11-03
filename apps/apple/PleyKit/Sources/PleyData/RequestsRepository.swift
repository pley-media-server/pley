import Combine
import Foundation
import PleyModel
import PleyNetwork

/// Requests for the profile in use (docs/API.md, "Requests"), as Android's
/// `RequestsRepository` keeps them: the summary that says what shows, and
/// the news to tell.
///
/// The summary is read at sign-in, after a profile switch or a sign-in
/// again, when the event stream connects, a second after request events,
/// after this app's own actions, and every minute in the foreground while
/// the stream is down. An older server (`404`, or a `403`
/// `insufficient_scope` naming no scope) is taken as requests being off,
/// and asked again only when the stream connects or the app starts.
///
/// News comes from the server's events as they happen; what came while the
/// app wasn't listening is caught up on from the summary's `unseen`, with
/// a mark per server and profile (`AppSettings.requestNewsMark`) so nothing
/// is told twice, across starts too.
@MainActor
public final class RequestsRepository: ObservableObject {
    /// How long request events are gathered before the summary is read again.
    public nonisolated static let summaryDebounce: Double = 1
    /// How often the summary is read while the event stream is down, in the foreground.
    public nonisolated static let summaryPollSeconds: Double = 60
    /// The most news read when catching up.
    public nonisolated static let catchUp = 20

    /// The summary; `nil` while it isn't known.
    @Published public private(set) var summary: RequestSummary?

    /// What shows of requests.
    public var gate: RequestsGate { .of(summary) }

    /// Something to tell the person, once.
    public var onNews: (RequestNews) -> Void = { _ in }
    /// Requests this app changed itself: the screens showing them follow at once.
    public var onChange: (RequestChange) -> Void = { _ in }
    /// Saves the summary, and gives it back while the server is out of reach (the app's).
    public weak var cache: (any SnapshotCache)?

    private let api: @Sendable () async throws -> PleyAPI
    private let settings: AppSettings
    private let debounce: Double
    private let pollEvery: Double

    /// The server and profile whose requests these are.
    private var owner: (origin: String, profileId: Int)?
    /// The server answered as older ones do: not asked again until the stream connects or the app starts.
    private var older = false
    /// The last fresh summary, to tell what moved since.
    private var last: RequestSummary?
    /// News events heard since the last fresh summary: they explain its `unseen` rising.
    private var explained = 0
    /// Requests told in this process, for the profile in use.
    private var announced: Set<Int> = []
    /// Events told news the saved mark doesn't cover yet: an event doesn't
    /// say when its request changed, so the next fresh summary catches up,
    /// which raises the mark past them.
    private var unmarked = false
    private var soon: Task<Void, Never>?
    private var poll: Task<Void, Never>?
    private var reads = 0
    private var foreground = true
    private var streaming = false

    public init(api: @escaping @Sendable () async throws -> PleyAPI, settings: AppSettings,
                debounce: Double = RequestsRepository.summaryDebounce,
                pollEvery: Double = RequestsRepository.summaryPollSeconds) {
        self.api = api
        self.settings = settings
        self.debounce = debounce
        self.pollEvery = pollEvery
    }

    /// Requests on one server's API (tests).
    public convenience init(api: PleyAPI, settings: AppSettings, debounce: Double = 0.05, pollEvery: Double = 60) {
        self.init(api: { api }, settings: settings, debounce: debounce, pollEvery: pollEvery)
    }

    // MARK: Following the app

    /// Signed in on `origin` as `profileId` (at start, after a switch or a
    /// sign-in again): the summary is read now.
    public func start(origin: String, profileId: Int) {
        if owner?.origin != origin || owner?.profileId != profileId {
            reset()
            owner = (origin, profileId)
        }
        older = false
        refresh()
        polling()
    }

    /// Signed out, or another server or profile: nothing is known any more.
    public func reset() {
        owner = nil
        summary = nil
        last = nil
        older = false
        explained = 0
        announced = []
        unmarked = false
        soon?.cancel()
        soon = nil
        poll?.cancel()
        poll = nil
        reads += 1
    }

    /// The app came to the foreground (`true`) or left it: the summary is
    /// polled only in the foreground.
    public func setForeground(_ foreground: Bool) {
        self.foreground = foreground
        polling()
    }

    /// The event stream connected (`true`) or went down. Connected, the
    /// summary is read again: nothing is replayed, and the server may have
    /// been updated.
    public func setStreaming(_ streaming: Bool) {
        let was = self.streaming
        self.streaming = streaming
        if streaming && !was && owner != nil {
            older = false
            refresh()
        }
        polling()
    }

    /// Requests changed on the server (its events): the summary is read again soon.
    public func heard(_ change: RequestChange) {
        refreshSoon()
    }

    /// The server's settings for requests changed.
    public func settingsChanged() {
        refreshSoon()
    }

    /// News from an event: told now, and noted so catching up doesn't tell it again.
    public func heard(_ news: RequestNews) {
        switch news {
        case .ready(let id, _, _, _, _, _), .declined(let id, _, _):
            explained += 1
            announced.insert(id)
            unmarked = true
        default:
            break
        }
        onNews(news)
    }

    // MARK: Reading the summary

    /// Reads the summary now (a Retry, a sign-in again).
    public func refresh() {
        Task { await refreshNow() }
    }

    /// Reads the summary now; its failure leaves what was known.
    public func refreshNow() async {
        guard let owner else { return }
        if older {
            summary = .off
            return
        }
        reads += 1
        let read = reads
        do {
            let fresh: RequestSummary
            if let cache {
                fresh = try await cache.cached(SnapshotKeys.requests) { try await $0.requestSummary() }
            } else {
                fresh = try await api().requestSummary()
            }
            guard read == reads, self.owner?.origin == owner.origin, self.owner?.profileId == owner.profileId else {
                return
            }
            await took(fresh)
        } catch {
            guard read == reads, Self.olderServer(error) else { return }
            older = true
            summary = .off
        }
    }

    /// The summary read again in a second, however many changes come meanwhile.
    public func refreshSoon() {
        soon?.cancel()
        let wait = debounce
        soon = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled, let self else { return }
            self.soon = nil
            await self.refreshNow()
        }
    }

    /// The person looked at their requests: the badge clears at once, the server's summary follows.
    public func zeroUnseen() {
        // A summary on its way was read before: it would bring the badge back.
        reads += 1
        if var summary, summary.unseen > 0 {
            summary.unseen = 0
            self.summary = summary
            if last != nil { last?.unseen = 0 }
        }
        refreshSoon()
    }

    /// This app changed `request` (or the title `kind` `tmdbId`): the
    /// summary is read now (counts, what's left), and the screens hear of
    /// it at once, before the server's event comes.
    public func acted(id: Int?, kind: TitleKind?, tmdbId: Int?) {
        refresh()
        onChange(RequestChange(id: id, kind: kind, tmdbId: tmdbId))
    }

    public func acted(_ request: MediaRequest) {
        acted(id: request.id, kind: request.kind, tmdbId: request.tmdbId)
    }

    /// An older server's answer: `404`, or (before scopes for requests) a `403` naming no scope.
    public nonisolated static func olderServer(_ error: any Error) -> Bool {
        switch error as? PleyError {
        case .api(404, _, _)?, .insufficientScope(nil)?: true
        default: false
        }
    }

    /// A fresh summary: what it says changed is caught up on.
    private func took(_ fresh: RequestSummary) async {
        let before = last
        last = fresh
        summary = fresh
        let news = explained
        explained = 0
        // Without events, changed_at says when what shows is to be read again.
        if let before, before.changedAt != fresh.changedAt, !streaming { onChange(.everything) }
        let rose = before.map { fresh.unseen > $0.unseen && news == 0 } ?? (fresh.unseen > 0)
        if (rose || unmarked) && fresh.enabled && fresh.signInAgain != RequestSummary.requestsScope {
            await catchUp()
        }
    }

    /// `GET /api/requests?unseen=1`: one notice for what wasn't told yet.
    /// Then every request listed has been told (now, by an event, or
    /// before), and the saved mark is raised past them all.
    private func catchUp() async {
        guard let owner else { return }
        // Events heard from here on set it again: their requests may come after this list.
        let pending = unmarked
        unmarked = false
        let list: RequestList
        do {
            list = try await api().requests(RequestQuery(unseen: true, limit: Self.catchUp))
        } catch {
            if pending { unmarked = true }
            return
        }
        guard self.owner?.origin == owner.origin, self.owner?.profileId == owner.profileId else { return }
        let mark = settings.requestNewsMark(origin: owner.origin, profileId: owner.profileId)
        let fresh = list.requests.filter { $0.updatedAt > mark && !announced.contains($0.id) }
        if !fresh.isEmpty {
            if fresh.count == 1 {
                if let news = Self.news(fresh[0]) { onNews(news) }
            } else {
                onNews(.news(count: fresh.count))
            }
            announced.formUnion(fresh.map(\.id))
        }
        guard let newest = list.requests.map(\.updatedAt).max(), newest > mark else { return }
        settings.setRequestNewsMark(newest, origin: owner.origin, profileId: owner.profileId)
    }

    /// The news a request caught up on tells: ready, or declined.
    public nonisolated static func news(_ request: MediaRequest) -> RequestNews? {
        switch request.stage {
        case .available:
            .ready(requestId: request.id, title: request.title, kind: request.kind, itemId: request.itemId,
                   seasons: nil, complete: true)
        case .partial:
            .ready(requestId: request.id, title: request.title, kind: request.kind, itemId: request.itemId,
                   seasons: request.availableSeasons, complete: false)
        case .declined:
            .declined(requestId: request.id, title: request.title, reason: request.reason)
        default:
            nil
        }
    }

    /// Every minute in the foreground while the stream is down.
    private func polling() {
        let wanted = foreground && !streaming && owner != nil
        guard wanted != (poll != nil) else { return }
        poll?.cancel()
        poll = nil
        guard wanted else { return }
        let every = pollEvery
        poll = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(every))
                guard !Task.isCancelled, let self else { return }
                await self.refreshNow()
            }
        }
    }
}
