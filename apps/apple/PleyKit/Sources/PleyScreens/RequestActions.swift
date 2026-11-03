import Combine
import Foundation
import PleyData
import PleyModel
import PleyNetwork

/// What a request action left.
public enum RequestOutcome: Equatable, Sendable {
    /// The request as it is now.
    case changed(MediaRequest)
    /// It's no longer this account's: withdrawn, removed, deleted, or no longer followed.
    case gone(Int)
    /// Asked for again: a new request (or one now followed).
    case askedAgain(Asked)
    case failed(RequestProblem)
}

extension AppModel {
    /// The open server's API, or `signedOut`.
    func requestsAPI() async throws -> PleyAPI {
        guard let api = await session.api else { throw PleyError.signedOut }
        return api
    }

    /// Tells `problem` in a toast: Sign in again with its button; the
    /// server out of reach in the offline banner.
    public func tell(_ problem: RequestProblem) {
        switch problem {
        case .offline:
            wentOffline()
        case .signInAgain:
            let words = problem.words(manage: requests.summary?.manage == true, summary: requests.summary)
            shell.show(words.title, action: ToastAction(RequestWords.signIn) { [weak self] in self?.signInAgain() })
        default:
            shell.show(problem.toast(manage: requests.summary?.manage == true, summary: requests.summary),
                       action: nil, error: true)
        }
    }

    /// How titles already shown change with `change`: their badges follow
    /// their request, read once (`GET /api/requests/{id}`); a title whose
    /// request can't be read, or that is named without one, follows its
    /// page instead (the request let go of, should that fail too). `nil`
    /// when none of `shown` is touched, or many changed at once (rows are
    /// never read again for that).
    public func patchFor(_ change: RequestChange, shown: [DiscoverTitle]) async -> ((DiscoverTitle) -> DiscoverTitle)? {
        guard !change.all else { return nil }
        let touched = shown.filter { RequestRules.touches($0, change) }
        guard !touched.isEmpty, let api = try? await requestsAPI() else { return nil }
        var requests: [Int: MediaRequest?] = [:]
        for id in Set(touched.compactMap(\.requestId)).intersection(change.ids) {
            do {
                requests[id] = .some(try await api.request(id))
            } catch {
                return nil
            }
        }
        var pages: [String: DiscoverDetail?] = [:]
        for title in touched where requests[title.requestId ?? -1].flatMap({ $0 }) == nil && pages[title.key] == nil {
            pages[title.key] = .some(try? await api.discoverDetail(title.kind, tmdbId: title.tmdbId))
        }
        return { title in
            guard RequestRules.touches(title, change) else { return title }
            if let id = title.requestId, let request = requests[id] ?? nil { return RequestRules.patched(title, with: request) }
            if let page = pages[title.key] ?? nil {
                var title = title
                title.status = page.status
                title.itemId = page.itemId
                title.requestId = page.requestId
                return title
            }
            return RequestRules.patched(title, with: nil)
        }
    }

    /// Does `action` to `request` (`approval` for Approve…, `reason` for
    /// Decline); the summary and the other screens follow.
    public func perform(_ action: CardAction, on request: MediaRequest, approval: Approval? = nil,
                        reason: String? = nil) async -> RequestOutcome {
        do {
            let api = try await requestsAPI()
            let outcome: RequestOutcome
            switch action {
            case .withdraw, .remove, .delete:
                try await api.deleteRequest(request.id)
                outcome = .gone(request.id)
            case .stopFollowing:
                try await api.unwatchRequest(request.id)
                outcome = .gone(request.id)
            case .approve, .tryAgain:
                outcome = .changed(try await api.approveRequest(request.id))
            case .approveWith:
                outcome = .changed(try await api.approveRequest(request.id, approval ?? Approval()))
            case .decline:
                outcome = .changed(try await api.declineRequest(request.id, reason: reason))
            case .handled:
                outcome = .changed(try await api.markHandled(request.id))
            case .askAgain:
                let asked = try await api.ask(RequestRules.askAgain(request))
                self.requests.acted(asked.request)
                outcome = .askedAgain(asked)
            case .watch:
                if let itemId = request.itemId { shell.open(.item(itemId)) }
                return .changed(request)
            }
            self.requests.acted(request)
            return outcome
        } catch {
            return .failed(RequestProblem.of(error))
        }
    }
}

/// A card's actions, shared by the list and the title page: Withdraw,
/// Remove and Delete ask first; Decline asks for an optional reason;
/// Approve… opens its dialog; the rest go at once. One action at a time
/// per request (`busy`).
@MainActor
public final class RequestActionRunner: ObservableObject {
    /// An action waiting for its Yes.
    public struct Confirming: Identifiable, Hashable, Sendable {
        public var action: CardAction
        public var card: RequestCard
        /// "Withdraw your request for “X”?".
        public var title: String
        public var id: Int { card.id }
    }

    @Published public var confirming: Confirming?
    /// Decline's dialog, while it shows.
    @Published public private(set) var declining: RequestCard?
    /// The reason typed, cut to what the server takes as it's typed.
    @Published public private(set) var reason = ""
    /// Approve…'s dialog, while it shows.
    @Published public private(set) var approving: ApproveModel?
    /// Requests with an action on its way.
    @Published public private(set) var busy: Set<Int> = []

    /// What each action left, for the screen to follow.
    public var onOutcome: (RequestCard, CardAction, RequestOutcome) -> Void = { _, _, _ in }

    private let app: AppModel

    public init(app: AppModel) {
        self.app = app
    }

    /// Decline's title: "Decline “X”?".
    public var declineTitle: String { declining.map { RequestWords.declineTitle($0.title) } ?? "" }

    /// `action` on `card`, as the person tapped it.
    public func tap(_ action: CardAction, on card: RequestCard) {
        guard !busy.contains(card.id) else { return }
        if let title = action.confirmation(card.title) {
            confirming = Confirming(action: action, card: card, title: title)
            return
        }
        switch action {
        case .watch:
            if let watch = card.watch { app.shell.open(watch) }
        case .decline:
            reason = ""
            declining = card
        case .approveWith:
            let model = ApproveModel(app: app, request: card.request)
            model.onApprove = { [weak self] approval in
                self?.approving = nil
                Task { await self?.run(.approveWith, on: card, approval: approval) }
            }
            approving = model
            Task { await model.load() }
        default:
            Task { await run(action, on: card) }
        }
    }

    /// The confirmation's Yes.
    public func confirm() {
        guard let confirming else { return }
        self.confirming = nil
        Task { await run(confirming.action, on: confirming.card) }
    }

    public func cancelConfirm() { confirming = nil }

    public func setReason(_ reason: String) {
        self.reason = RequestRules.cutReason(reason)
    }

    /// Decline's dialog's Decline, with the reason typed.
    public func decline() {
        guard let card = declining else { return }
        declining = nil
        let reason = RequestRules.declineReason(reason)
        Task { await run(.decline, on: card, reason: reason) }
    }

    public func cancelDecline() { declining = nil }

    public func cancelApprove() { approving = nil }

    /// Does `action`; its failure is told, its outcome handed to `onOutcome`.
    @discardableResult
    public func run(_ action: CardAction, on card: RequestCard, approval: Approval? = nil,
                    reason: String? = nil) async -> RequestOutcome {
        busy.insert(card.id)
        let outcome = await app.perform(action, on: card.request, approval: approval, reason: reason)
        busy.remove(card.id)
        switch outcome {
        case .failed(let problem):
            app.tell(problem)
        case .askedAgain(let asked):
            app.shell.show(RequestWords.asked(asked, title: card.title))
        default:
            break
        }
        onOutcome(card, action, outcome)
        return outcome
    }
}

/// Approve…: the quality profile, the folder and (shows) the seasons to
/// approve with, from Radarr's or Sonarr's choices. "As in the settings"
/// comes first when the settings choose none; without choices (the
/// service isn't connected) only "Approve with the settings' choices" shows.
@MainActor
public final class ApproveModel: ObservableObject, Identifiable {
    public struct Profile: Hashable, Sendable, Identifiable {
        /// `nil`: as in the settings.
        public var id: Int?
        public var name: String
    }

    public struct Folder: Hashable, Sendable, Identifiable {
        /// `nil`: as in the settings.
        public var path: String?
        public var name: String
        public var free: String?
        public var id: String { path ?? "" }
    }

    public nonisolated let id = UUID()
    public let request: MediaRequest
    /// "Approve “Arcane”".
    public let title: String
    @Published public private(set) var loading = true
    @Published public private(set) var profiles: [Profile] = []
    @Published public private(set) var folders: [Folder] = []
    @Published public var profile: Int?
    @Published public var folder: String?
    /// Shows: the seasons ticked, all to start with.
    @Published public private(set) var seasons: Set<Int> = []
    /// The seasons asked for.
    public let allSeasons: [Int]
    /// What the service said went wrong, under the choices.
    @Published public private(set) var error: String?

    var onApprove: (Approval) -> Void = { _ in }
    private let app: AppModel
    private var options: ApprovalOptions?

    init(app: AppModel, request: MediaRequest) {
        self.app = app
        self.request = request
        title = RequestWords.approveTitle(request.title)
        allSeasons = Approving.seasons(request)
        seasons = Set(allSeasons)
    }

    /// Something to choose: the service answered with lists.
    public var choices: Bool { !profiles.isEmpty || !folders.isEmpty }

    /// The button's words.
    public var button: String { choices ? RequestWords.approve : RequestWords.approveSettings }

    /// Approve can be pressed: a show keeps at least one season.
    public var canApprove: Bool { !loading && (request.kind != .show || allSeasons.isEmpty || !seasons.isEmpty) }

    /// Reads the service's choices.
    public func load() async {
        let options = try? await app.requestsAPI().requestOptions().of(request.kind)
        self.options = options
        let settingsProfile = Approving.profile(options, for: request)
        let settingsFolder = Approving.folder(options, for: request)
        var profiles = (options?.qualityProfiles ?? []).map { Profile(id: $0.id, name: $0.name) }
        if settingsProfile == nil, !profiles.isEmpty { profiles.insert(Profile(id: nil, name: RequestWords.approveDefault), at: 0) }
        var folders = (options?.rootFolders ?? []).map {
            Folder(path: $0.path, name: $0.path, free: $0.freeBytes.map(RequestWords.folderFree))
        }
        if settingsFolder == nil, !folders.isEmpty { folders.insert(Folder(path: nil, name: RequestWords.approveDefault), at: 0) }
        self.profiles = profiles
        self.folders = folders
        profile = settingsProfile
        folder = settingsFolder
        error = options?.error
        loading = false
    }

    public func toggle(season: Int) {
        guard allSeasons.contains(season) else { return }
        if seasons.contains(season) { seasons.remove(season) } else { seasons.insert(season) }
    }

    /// What approving sends: only what differs from the settings' choices.
    public var approval: Approval {
        Approving.approval(options: options, request: request, profile: profile, folder: folder,
                           seasons: allSeasons.isEmpty ? nil : seasons)
    }

    public func approve() {
        guard canApprove else { return }
        onApprove(approval)
    }
}
