import PleyModel
import PleyPlayer
import PleyScreens
import SwiftUI

/// A title's actions on its tile: the context menu (long press on iPhone
/// and iPad, right-click or Control-click on the Mac) and the same actions
/// for VoiceOver's Actions rotor. The list is made when the menu opens, so
/// a mark or My List just changed shows as it now is.
struct ItemActionsMenu: ViewModifier {
    let app: AppModel
    @ObservedObject private var actions: ItemActionsModel
    let item: ItemSummary
    let continueWatching: Bool
    /// Continue Watching's own removal (Home takes the tile out at once).
    let remove: (() async -> Void)?
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif

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
    }

    private static func destructive(_ action: ItemAction) -> Bool {
        action == .removeFromContinueWatching || action == .removeFromMyList
    }

    private func run(_ action: ItemAction) {
        #if os(macOS)
        Self.run(app, action, on: item, remove: remove) { openWindow(id: PlayerWindow.id, value: $0) }
        #else
        Self.run(app, action, on: item, remove: remove, play: nil)
        #endif
    }

    /// Does `action` on `item`: the tile's menu and a page's More menu both
    /// come here. `play` opens the player on the Mac (its own window);
    /// elsewhere the app's player covers the screen.
    static func run(_ app: AppModel, _ action: ItemAction, on item: ItemSummary, remove: (() async -> Void)? = nil,
                    play: ((PlayRequest) -> Void)?) {
        Task {
            if action == .removeFromContinueWatching, let remove {
                await remove()
                return
            }
            switch await app.actions.perform(action, on: item) {
            case .done: break
            case .open(let screen): app.shell.open(screen)
            case .play(let request):
                if let play {
                    play(request)
                } else {
                    await app.play(request)
                }
            }
        }
    }
}

extension View {
    /// `item`'s actions on this view: its context menu and VoiceOver's actions.
    func itemActions(_ app: AppModel, _ item: ItemSummary, continueWatching: Bool = false,
                     remove: (() async -> Void)? = nil) -> some View {
        modifier(ItemActionsMenu(app: app, item: item, continueWatching: continueWatching, remove: remove))
    }
}
