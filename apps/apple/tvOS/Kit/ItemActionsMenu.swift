import PleyModel
import PleyScreens
import SwiftUI

/// A title's actions on its tile: the context menu a long press on Select
/// opens, the same actions for VoiceOver, and Play/Pause playing it. The
/// list is made when the menu opens, so a mark or My List just changed
/// shows as it now is.
struct ItemActionsMenu: ViewModifier {
    let app: AppModel
    @ObservedObject private var actions: ItemActionsModel
    let item: ItemSummary
    let continueWatching: Bool
    /// Continue Watching's own removal (Home takes the tile out at once).
    let remove: (() async -> Void)?

    init(app: AppModel, item: ItemSummary, continueWatching: Bool, remove: (() async -> Void)?) {
        self.app = app
        _actions = ObservedObject(wrappedValue: app.actions)
        self.item = item
        self.continueWatching = continueWatching
        self.remove = remove
    }

    func body(content: Content) -> some View {
        let list = actions.actions(for: item, continueWatching: continueWatching)
        content
            .contextMenu {
                ForEach(list, id: \.self) { action in
                    Button(role: Self.destructive(action) ? .destructive : nil) {
                        run(action)
                    } label: {
                        Label(action.title, systemImage: action.symbol)
                    }
                    .accessibilityIdentifier("actions.\(action.key)")
                }
            }
            .accessibilityActions {
                ForEach(list, id: \.self) { action in
                    Button(action.title) { run(action) }
                }
            }
            .onPlayPauseCommand {
                if let first = list.first, [.play, .resume].contains(first) { run(first) }
            }
    }

    private static func destructive(_ action: ItemAction) -> Bool {
        action == .removeFromContinueWatching || action == .removeFromMyList
    }

    private func run(_ action: ItemAction) {
        Task {
            if action == .removeFromContinueWatching, let remove {
                await remove()
                return
            }
            switch await actions.perform(action, on: item) {
            case .done: break
            case .open(let screen): app.shell.open(screen)
            case .play(let request): await app.play(request)
            }
        }
    }
}

extension View {
    /// `item`'s actions on this view: its context menu, VoiceOver's actions and Play/Pause.
    func itemActions(_ app: AppModel, _ item: ItemSummary, continueWatching: Bool = false,
                     remove: (() async -> Void)? = nil) -> some View {
        modifier(ItemActionsMenu(app: app, item: item, continueWatching: continueWatching, remove: remove))
    }
}
