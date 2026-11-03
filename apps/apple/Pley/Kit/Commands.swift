import PleyScreens
import SwiftUI

/// What Play (⌘↩) in the Mac's Playback menu plays: the tile with the
/// focus, or else the title page showing.
struct PlayTargetKey: FocusedValueKey {
    typealias Value = PlayTarget
}

extension FocusedValues {
    var playTarget: PlayTarget? {
        get { self[PlayTargetKey.self] }
        set { self[PlayTargetKey.self] = newValue }
    }
}

/// Makes the page View → Reload (⌘R) reads again while it shows.
private struct Reloads: ViewModifier {
    let shell: ShellModel
    let reload: @MainActor () async -> Void
    @State private var owner = UUID()

    func body(content: Content) -> some View {
        content
            .onAppear { shell.showing(owner, reload: reload) }
            .onDisappear { shell.leaving(owner) }
    }
}

extension View {
    /// This page reads itself again with `reload` on ⌘R (the Mac's View
    /// menu, an iPad's keyboard) while it shows.
    func reloads(_ app: AppModel, _ reload: @escaping @MainActor () async -> Void) -> some View {
        modifier(Reloads(shell: app.shell, reload: reload))
    }
}
