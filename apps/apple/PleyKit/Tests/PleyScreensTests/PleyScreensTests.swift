import CoreGraphics
import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
@testable import PleyNetwork
@testable import PleyScreens
import PleyTesting
import Testing

private let testAgent = "pleyApple/9.9.9 (iOS 18.1; iPhone)"

private func fixture() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}

/// The sign-in page without a browser: asks the fixture's `/oauth/authorize`
/// and follows its `303` back to the app by hand.
private struct FakeWebAuthenticator: WebAuthenticator {
    func authenticate(url: URL, callbackScheme: String) async throws -> URL {
        let response = try await HTTPTransport(userAgent: testAgent)
            .perform(URLRequest(url: url), redirects: .endpoints, maxBytes: BodyLimit.small)
        guard response.status == 303, let location = response.header("Location"), let back = URL(string: location)
        else { throw AuthError.noBrowser }
        return back
    }
}

/// An app on memory stores, with the fake sign-in sheet and no network search.
@MainActor
private func app(_ store: any TokenStore = MemoryTokenStore(), servers: [SavedServer] = [],
                 client: AppClient = .ios,
                 discover: @escaping @Sendable () -> AsyncStream<DiscoveryState> = { AsyncStream { $0.finish() } })
    -> AppModel {
    let transport = HTTPTransport(userAgent: testAgent)
    let repository = ServerRepository(store: MemoryServerListStore(servers))
    let session = SessionManager(transport: transport, tokenStore: store, servers: repository, client: client,
                                 sleep: { _ in try await Task.sleep(for: .milliseconds(5)) })
    return AppModel(session: session, servers: repository, transport: transport,
                    authenticator: client.signsInWithBrowser ? FakeWebAuthenticator() : nil, discover: discover)
}

/// Tokens the fixture gave, kept as a sign-in keeps them.
private func keepSignIn(_ server: FixtureServer, in store: any TokenStore) async throws {
    let tokens = server.issueTokens()
    try await store.save(StoredTokens(
        issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
        revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
        accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: tokens.refreshToken,
        scope: FixtureServer.allScopes))
}

/// Waits until `condition` holds, for at most 5 seconds.
@MainActor
private func eventually(_ condition: @MainActor () -> Bool) async -> Bool {
    for _ in 0..<500 {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return condition()
}

@MainActor
@Suite struct AppModelTests {
    @Test func withNoServerTheServersShow() async {
        let app = app()
        #expect(app.route == .starting)
        await app.start()
        #expect(app.route == .servers)
    }

    @Test func theLastServerOpensAtStartWhenSignedIn() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [
            SavedServer(origin: "http://192.168.1.2:7539", name: "Old", lastUsed: Date(timeIntervalSince1970: 1)),
            SavedServer(origin: server.origin, name: "Kept name", id: "abc", lastUsed: Date()),
        ])
        await app.start()
        // At start, Home shows straight away: choosing a profile is for after a sign-in.
        #expect(app.route == .home)
        #expect(app.user?.name == "Alex")
        #expect(app.server?.name == "Living Room")
        #expect(app.server?.id == "abc")
    }

    @Test func theLastServerWithoutASignInShowsTheServers() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = app(servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        #expect(app.route == .servers)
        #expect(app.user == nil)
    }

    @Test func aServerThatDoesNotAnswerAtStartSaysSo() async throws {
        let server = try await fixture()
        let origin = server.origin
        server.stop()
        let app = app(servers: [SavedServer(origin: origin, name: "Gone", lastUsed: Date())])
        await app.start()
        #expect(app.route == .servers)
        #expect(app.notice?.hasPrefix("Gone: ") == true)
    }

    @Test func aSignInTheServerEndsGoesBackToSignInWithWhy() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        #expect(app.route == .home)
        server.refuseEveryToken()
        let home = HomeModel(app: app)
        await home.reload()
        #expect(await eventually { if case .signIn = app.route { true } else { false } })
        guard case .signIn(let entry) = app.route else { return }
        #expect(entry.origin == server.origin)
        #expect(app.notice == PleyError.signedOut.userMessage)
        #expect(app.user == nil)
        #expect(SignInModel(app: app, entry: entry).notice == PleyError.signedOut.userMessage)
    }

    @Test func signingOutRevokesAndShowsTheServers() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        await app.signOut()
        #expect(app.route == .servers)
        #expect(server.requests("/api/oauth/revoke").count == 1)
        #expect(try await store.load(issuer: server.origin) == nil)
    }
}

@MainActor
@Suite struct ServersModelTests {
    @Test func aTypedAddressOpensTheServersSignIn() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = app()
        await app.start()
        let servers = ServersModel(app: app)
        servers.address = "  \(server.origin)  "
        await servers.add()
        #expect(servers.error == nil)
        #expect(servers.address.isEmpty)
        guard case .signIn(let entry) = app.route else {
            Issue.record("no sign-in: \(app.route)")
            return
        }
        #expect(entry.origin == server.origin)
        #expect(entry.name == "Living Room")
    }

    @Test func somethingElseIsRefusedInWords() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.answer(path: "/api/server", status: 200, json: #"{"hello":"world"}"#)
        let app = app()
        await app.start()
        let servers = ServersModel(app: app)
        servers.address = server.origin
        await servers.add()
        #expect(servers.error == PleyError.notPley.userMessage)
        #expect(app.route == .servers)
    }

    @Test func foundAndSavedServersAreListedApart() async throws {
        let found = FoundServer(origin: "http://192.168.1.9:7539", name: "Den", id: "den", version: "1.0.0")
        let state = DiscoveryState(servers: [found], blocked: false, searching: false)
        let app = app(servers: [SavedServer(origin: "http://192.168.1.2:7539", name: "Office", lastUsed: Date())],
                      discover: { AsyncStream { $0.yield(state); $0.finish() } })
        let servers = ServersModel(app: app)
        await servers.load()
        await servers.browse()
        #expect(servers.savedEntries.map(\.name) == ["Office"])
        #expect(servers.foundEntries.map(\.name) == ["Den"])
        #expect(!servers.searching)
    }

    @Test func aRefusedNetworkIsShown() async {
        let app = app(discover: { AsyncStream { $0.yield(DiscoveryState(blocked: true, searching: false)); $0.finish() } })
        let servers = ServersModel(app: app)
        await servers.browse()
        #expect(servers.blocked)
    }

    @Test func forgettingASavedServerSignsOutOfIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        let servers = ServersModel(app: app)
        await servers.load()
        let entry = try #require(servers.savedEntries.first)
        await servers.forget(entry)
        #expect(servers.entries.isEmpty)
        #expect(server.requests("/api/oauth/revoke").count == 1)
        #expect(try await store.load(issuer: server.origin) == nil)
    }
}

@MainActor
@Suite struct SignInModelTests {
    private func signIn(_ app: AppModel, _ server: FixtureServer) async throws -> SignInModel {
        try await app.open(ServerEntry(origin: server.origin, name: "Living Room", saved: false, found: false))
        guard case .signIn(let entry) = app.route else { throw AuthError.noCode }
        return SignInModel(app: app, entry: entry)
    }

    @Test func theBrowserSignsInAndTheHouseholdShows() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = app()
        let model = try await signIn(app, server)
        #expect(model.canUseBrowser)
        #expect(!model.unencryptedWarning)
        model.signInWithBrowser()
        // The fixture's household has five profiles, so they show first.
        #expect(await eventually { app.route == .profiles })
        #expect(app.user?.name == "Alex")
        app.back()
        #expect(app.route == .home)
    }

    @Test func aCodeIsShownWithItsQRCodeAndSignsIn() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = app(client: .tvos)
        let model = try await signIn(app, server)
        #expect(!model.canUseBrowser)
        model.signInWithCode()
        #expect(await eventually { if case .code = model.phase { true } else { false } })
        guard case .code(let code, let qr) = model.phase else { return }
        #expect(code.userCode.count == 9)
        #expect(model.codeAddress?.hasPrefix(server.origin) == true)
        #expect(qr != nil)
        #expect(model.secondsLeft > 0)
        server.approveDevice()
        #expect(await eventually { app.route == .profiles })
    }

    @Test func aRefusedCodeSaysSoAndCanBeTriedAgain() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = app(client: .tvos)
        let model = try await signIn(app, server)
        server.denyDevice()
        model.signInWithCode()
        #expect(await eventually { if case .failed = model.phase { true } else { false } })
        guard case .failed(let why) = model.phase else { return }
        #expect(why == AuthError.refused.message)
        #expect(model.usingCode)
    }

    @Test func cancellingStopsWaiting() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = app(client: .tvos)
        let model = try await signIn(app, server)
        model.signInWithCode()
        #expect(await eventually { if case .code = model.phase { true } else { false } })
        model.cancel()
        guard case .idle = model.phase else {
            Issue.record("still waiting")
            return
        }
        // A poll already sent may still arrive; after it, none (they'd come every 5 ms).
        try await Task.sleep(for: .milliseconds(100))
        let polls = server.requests("/api/oauth/token").count
        try await Task.sleep(for: .milliseconds(200))
        #expect(server.requests("/api/oauth/token").count == polls)
    }

    @Test func countdowns() {
        #expect(SignInModel.minutes(605) == "10:05")
        #expect(SignInModel.minutes(59) == "0:59")
        #expect(SignInModel.minutes(-3) == "0:00")
    }
}

@MainActor
@Suite struct ProfilesModelTests {
    private func signedIn(_ server: FixtureServer) async throws -> AppModel {
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        app.showProfiles()
        return app
    }

    @Test func theHouseholdIsListedAndALockedProfileCannotBeChosen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let model = ProfilesModel(app: try await signedIn(server))
        await model.load()
        #expect(model.profiles.map(\.name) == ["Alex", "Sam", "Kids", "Robin", "Guest"])
        let guest = try #require(model.profiles.first { $0.name == "Guest" })
        #expect(!ProfilesModel.canChoose(guest))
        #expect(model.profiles.filter(ProfilesModel.canChoose).count == 4)
    }

    @Test func aPinThatIsNotFourToEightDigitsIsNotSent() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let model = ProfilesModel(app: try await signedIn(server))
        await model.load()
        let sam = try #require(model.profiles.first { $0.name == "Sam" })
        await model.choose(sam)
        #expect(model.pinFor == sam)
        for pin in ["", "123", "123456789", "12a4", "１２３４"] {
            model.pin = pin
            await model.submitPin()
            #expect(model.pinError == ProfileSwitchError.pinFormat.message)
        }
        #expect(server.journal.allSatisfy { $0.grantType != FixtureServer.switchProfileGrant })
    }

    @Test func aWrongPinIsRefusedAndTheRightOneSwitches() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ProfilesModel(app: app)
        await model.load()
        let sam = try #require(model.profiles.first { $0.name == "Sam" })
        await model.choose(sam)
        model.pin = "9999"
        await model.submitPin()
        #expect(model.pinError == ProfileSwitchError.wrongPin.message)
        #expect(model.pin.isEmpty)
        #expect(app.route == .profiles)
        #expect(app.user?.name == "Alex")
        let version = app.homeVersion

        model.pin = "1234"
        await model.submitPin()
        #expect(model.pinFor == nil)
        #expect(app.user?.name == "Sam")
        #expect(app.route == .home)
        #expect(app.homeVersion == version + 1)
    }

    @Test func aProfileWithoutPinSwitchesAndOneWithAPasswordExplains() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ProfilesModel(app: app)
        await model.load()
        let robin = try #require(model.profiles.first { $0.name == "Robin" })
        await model.choose(robin)
        #expect(model.error == ProfileSwitchError.needsBrowser.message)
        #expect(app.user?.name == "Alex")

        let kids = try #require(model.profiles.first { $0.name == "Kids" })
        await model.choose(kids)
        #expect(app.user?.name == "Kids")
        #expect(app.route == .home)
    }

    @Test func avatars() {
        #expect(Avatar.initials("Alex") == "A")
        #expect(Avatar.initials("alex  smith jones") == "AS")
        #expect(Avatar.initials("  ") == "?")
        #expect(Avatar.rgb("#38bdf8")?.red == 0x38 / 255.0)
        #expect(Avatar.rgb("#fff")?.green == 1)
        #expect(Avatar.rgb("red") == nil)
        #expect(Avatar.rgb("#12345g") == nil)
        #expect(Avatar.rgb(nil) == nil)
    }
}

@MainActor
@Suite struct HomeModelTests {
    @Test func homeShowsTheRowsWithPosters() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
        await app.start()
        let home = HomeModel(app: app)
        await home.reload()
        guard case .loaded(let hubs) = home.state else {
            Issue.record("not loaded: \(home.state)")
            return
        }
        #expect(!hubs.isEmpty)
        let item = try #require(hubs.first?.items.first)
        let path = try #require(HomeModel.posterPath(item))
        let image = try await app.images.image(path: path, widthPx: HomeModel.pixels(110, scale: 3))
        #expect(max(image.width, image.height) <= 330)
    }

    @Test func tilesAskForTheScreensPixels() {
        #expect(HomeModel.pixels(110, scale: 2) == 220)
        #expect(HomeModel.pixels(250, scale: 1) == 250)
        #expect(HomeModel.pixels(130, scale: 0) == 130)
    }
}

@Suite struct QRCodeTests {
    @Test func aCodeIsSquareInWholeModulesWithItsQuietZone() throws {
        let text = "http://192.168.1.20:7539/link?code=BCDFGHJK"
        let modules = try #require(QRCode.modules(text))
        // A QR code's side is 21 modules, plus 4 per version above 1.
        #expect(modules.count >= 21 && (modules.count - 21) % 4 == 0)
        #expect(modules.allSatisfy { $0.count == modules.count })
        #expect(QRCode.finder(modules, row: 0, column: 0))
        #expect(QRCode.finder(modules, row: 0, column: modules.count - 7))
        #expect(QRCode.finder(modules, row: modules.count - 7, column: 0))
        #expect(!QRCode.finder(modules, row: modules.count - 7, column: modules.count - 7))

        let image = try #require(QRCode.make(text, scale: 8))
        #expect(image.width == image.height)
        #expect(image.width == (modules.count + 2 * QRCode.quietZone) * 8)
        #expect(image.width % 8 == 0)
    }

    @Test func theQuietZoneIsWhiteAndModulesAreDark() throws {
        let image = try #require(QRCode.make("BCDF-GHJK", scale: 4))
        let data = try #require(image.dataProvider?.data as Data?)
        let side = image.width
        func pixel(_ x: Int, _ y: Int) -> UInt8 { data[y * image.bytesPerRow + x] }
        // The quiet zone, then the top-left finder's corner.
        #expect(pixel(0, 0) == 255)
        #expect(pixel(4 * 4 - 1, 4 * 4 - 1) == 255)
        #expect(pixel(4 * 4, 4 * 4) == 0)
        #expect(pixel(side - 1, side - 1) == 255)
    }
}

@Suite struct WordsTests {
    @Test func messagesReadAsSentences() {
        #expect(asSentence("no answer in time") == "No answer in time.")
        #expect(asSentence("  wrong PIN!") == "Wrong PIN!")
        #expect(asSentence("Waiting…") == "Waiting…")
        #expect(asSentence("") == "")
    }
}
