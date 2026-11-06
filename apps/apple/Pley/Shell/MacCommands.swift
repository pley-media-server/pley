#if os(macOS)
import AppKit
import PleyData
import PleyModel
import PleyScreens
import SwiftUI

/// The Mac's menus beyond Playback: About Pley in the app menu, and the
/// File and View menus.
///
/// - File: Add Server… (⇧⌘N), Servers, Switch Profile… (⇧⌘P) and Sign Out….
/// - View: Home (⌘1), Search (⌘F), Playlists (⌘2), the first seven
///   libraries (⌘3 to ⌘9), Discover (⇧⌘D) and Your requests or Requests
///   (⇧⌘R) where requests are open, Reload (⌘R), Larger and Smaller Tiles (⌘=, ⌘−;
///   ⌘+ too, through `MacAppDelegate`'s key monitor),
///   then the sidebar's and the toolbar's own commands.
struct MacCommands: Commands {
    @ObservedObject var app: AppModel
    @ObservedObject var shell: ShellModel
    @Environment(\.openWindow) private var openWindow
    @AppStorage(AppSettings.gridDensityKey) private var density = GridDensity.medium.rawValue

    init(app: AppModel) {
        self.app = app
        shell = app.shell
    }

    private var signedIn: Bool { app.root == .signedIn }

    var body: some Commands {
        CommandGroup(replacing: .appInfo) {
            Button("About Pley") { openWindow(id: AboutWindow.id) }
        }
        CommandGroup(replacing: .newItem) {
            Button("Add Server…") { app.showServers(adding: true) }
                .keyboardShortcut("n", modifiers: [.command, .shift])
            Button("Servers") { app.showServers() }
                .disabled(app.root == .servers || app.root == .starting)
            Divider()
            Button("Switch Profile…") { app.showProfiles() }
                .keyboardShortcut("p", modifiers: [.command, .shift])
                .disabled(!signedIn || !app.canSwitchProfile)
            Button("Sign Out…") { signOut() }
                .disabled(app.user == nil)
        }
        CommandGroup(before: .sidebar) {
            ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                section(entry)
            }
            if shell.requestsEntry.discover {
                Button(RequestWords.discover) { openUnderMe(.discover) }
                    .keyboardShortcut("d", modifiers: [.command, .shift])
                    .disabled(!signedIn)
            }
            if shell.requestsEntry.requests {
                Button(shell.requestsEntry.requestsTitle) { openUnderMe(.requests) }
                    .keyboardShortcut("r", modifiers: [.command, .shift])
                    .disabled(!signedIn)
            }
            Divider()
            Button("Reload") { Task { await shell.reloadShowing() } }
                .keyboardShortcut("r")
                .disabled(!signedIn || !shell.canReload)
            // ⌘=, the key ⌘+ is typed on: a menu item named "+" would need ⇧ too.
            Button("Larger Tiles") { step(1) }
                .keyboardShortcut("=")
                .disabled(!signedIn || current.stepped(by: 1) == nil)
            Button("Smaller Tiles") { step(-1) }
                .keyboardShortcut("-")
                .disabled(!signedIn || current.stepped(by: -1) == nil)
            Divider()
        }
        SidebarCommands()
        ToolbarCommands()
    }

    /// The View menu's sections: Search goes after Home, as in the sidebar.
    private var entries: [ShellModel.MenuEntry?] {
        var entries: [ShellModel.MenuEntry?] = shell.viewMenu
        entries.insert(nil, at: min(shell.showsLiveTv ? 2 : 1, entries.count))
        return entries
    }

    @ViewBuilder private func section(_ entry: ShellModel.MenuEntry?) -> some View {
        if let entry {
            let button = Button(entry.title) { shell.select(entry.section) }
                .disabled(!signedIn)
            if let key = entry.key {
                button.keyboardShortcut(KeyEquivalent(key))
            } else {
                button
            }
        } else {
            Button("Search") { shell.findInSearch() }
                .keyboardShortcut("f")
                .disabled(!signedIn)
        }
    }

    /// Requests' pages open under Me, where their rows are.
    private func openUnderMe(_ screen: Screen) {
        if shell.section != .me { shell.select(.me) }
        shell.open(screen)
    }

    private var current: GridDensity { GridDensity(rawValue: density) ?? .medium }

    private func step(_ by: Int) {
        if let next = current.stepped(by: by) { density = next.rawValue }
    }

    /// Asks first, as Me's Sign out does.
    private func signOut() {
        let alert = NSAlert()
        alert.messageText = "Sign out of \(app.server?.name ?? "this server")?"
        alert.informativeText = "You can sign in again from Servers."
        alert.addButton(withTitle: "Sign Out")
        alert.addButton(withTitle: "Cancel")
        alert.buttons.first?.hasDestructiveAction = true
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        Task { await app.signOut() }
    }
}

/// About Pley: a window of its own, in place of the standard panel.
struct AboutWindow: Scene {
    static let id = "about"
    let app: AppModel

    var body: some Scene {
        Window("About Pley", id: Self.id) {
            InLanguage(app: app) {
                AboutView()
                    .frame(minWidth: 420, idealWidth: 460, minHeight: 300, idealHeight: 320)
            }
        }
        .windowResizability(.contentSize)
        .restorationBehavior(.disabled)
    }
}
#endif
