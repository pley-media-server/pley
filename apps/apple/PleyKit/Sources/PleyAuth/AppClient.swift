import Foundation

/// Which of Pley's Apple apps signs in. Each has a client ID: a client
/// metadata document built into every Pley server
/// (apps/server/src/oauth/builtin), never fetched, so signing in works
/// without the internet.
public enum AppClient: String, Sendable, CaseIterable {
    case ios
    case tvos
    case macos

    public var clientID: String {
        "https://pley-media-server.github.io/pley/oauth/\(rawValue).json"
    }

    /// Whether this app signs in through the server's page in a browser.
    /// Apple TV has no browser.
    public var signsInWithBrowser: Bool { self != .tvos }

    /// Whether this app signs in with a code approved on another device.
    /// The Mac signs in in its browser only, and the server refuses it a code.
    public var signsInWithCode: Bool { self != .macos }

    /// The app this build is.
    public static var current: AppClient {
        #if os(tvOS)
        .tvos
        #elseif os(macOS)
        .macos
        #else
        .ios
        #endif
    }

    /// Where the browser comes back: the app's id as a private-use scheme
    /// (RFC 8252 §7.1). `ASWebAuthenticationSession` catches it; the app
    /// registers no URL scheme on iPhone, iPad and Mac, so nothing else can
    /// open the app with one (Apple TV's opens its signed Top Shelf links).
    public static let redirectScheme = "app.pley.apple"
    public static let redirectPath = "/oauth/callback"
    public static let redirectURI = "\(redirectScheme):\(redirectPath)"

    /// What the app asks for: a viewer's rights, no more than its built-in
    /// document allows.
    public static let scope = "library play playlists profiles"

    /// Asked for besides, only where the server lists each in its
    /// metadata: asking for movies and shows, and a manager's approving
    /// and declining. A server from before requests doesn't know them and
    /// would refuse the whole sign-in for one.
    public static let listedScope = "requests approvals"

    /// RFC 8628's grant: a TV collects its tokens with its code.
    public static let deviceCodeGrant = "urn:ietf:params:oauth:grant-type:device_code"

    /// Pley's grant for moving a sign-in to another profile of the household.
    public static let switchProfileGrant = "urn:pley:params:oauth:grant-type:switch-profile"
}
