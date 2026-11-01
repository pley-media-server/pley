import Combine
import Foundation
import PleyCommon
import PleyModel

/// Me (Apple TV: the Settings tab): who is signed in, on which server,
/// Switch profile, the servers and Sign out.
@MainActor
public final class MeModel: ObservableObject {
    /// Says where a profile's name, picture and PIN are changed.
    public static let webNote = "A profile's name, picture and PIN are changed in Pley on the web."

    /// Whether Switch profile shows: the app may switch (`profiles`) and
    /// the household has another profile.
    @Published public private(set) var canSwitch = false

    public let app: AppModel

    public init(app: AppModel) {
        self.app = app
    }

    /// The name shown: the profile's display name, else its username.
    public var name: String {
        guard let user = app.user else { return "" }
        return user.displayName.flatMap { $0.isEmpty ? nil : $0 } ?? user.name
    }

    /// "Managed profile" for a profile without a password of its own; `nil` otherwise.
    public var kind: String? {
        app.user?.managed == true ? "Managed profile" : nil
    }

    /// The server's name, and its address as people type it.
    public var serverName: String { app.server?.name ?? "" }
    public var serverAddress: String { app.server.map { Servers.displayAddress($0.origin) } ?? "" }

    /// Asks the household, for Switch profile.
    public func load() async {
        guard app.user?.scopes.contains("profiles") == true, let switcher = await app.session.profiles else {
            canSwitch = false
            return
        }
        do {
            canSwitch = try await switcher.profiles().count > 1
        } catch is CancellationError {
        } catch {
            // Without an answer the switch stays offered: "Who's watching?" says why it can't.
            canSwitch = true
        }
    }

    public func switchProfile() {
        app.showProfiles()
    }

    public func showServers() {
        app.showServers()
    }

    public func signOut() async {
        await app.signOut()
    }
}
