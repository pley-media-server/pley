// Signs in to a real, disposable Pley server the way the Apple apps do,
// and checks what the server made of it. scripts/check-server.sh starts the
// server and runs this; never point it at a server people use.
//
//   pley-apple-check --server http://127.0.0.1:7712 --client tvos|ios --admin <username>
//
// `pley-apple-check --print-caps` prints instead what this Mac's probe read
// and the caps report the Apple apps would send from it, so a Mac's device
// class can be recorded (PleyKit/Tests/PleyCapsTests/Probes).
//
// The administrator's password comes from PLEY_CHECK_PASSWORD. Nothing it
// prints holds a token, a code or the password.

#if os(macOS)
import Foundation
import PleyAuth
import PleyCaps
import PleyCommon
import PleyModel
import PleyNetwork
import PleyTesting

struct CheckFailure: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

extension Optional {
    func orThrow(_ what: String) throws -> Wrapped {
        guard let self else { throw CheckFailure(what) }
        return self
    }
}

func say(_ line: String) {
    print(line)
}

func check(_ condition: Bool, _ what: @autoclosure () -> String) throws {
    if !condition { throw CheckFailure(what()) }
}

/// This Mac's probe input, its report and the report in words. The input
/// holds no personal data: the model, the chip, the screen and the outputs.
func printCaps() async throws {
    let input = await SystemCapsProbe().read(screen: nil)
    let snapshot = CapsBuilder.build(input)
    let encoder = JSONEncoder()
    encoder.keyEncodingStrategy = .convertToSnakeCase
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
    say("probe input:")
    say(String(decoding: try encoder.encode(input), as: UTF8.self))
    say("caps report (device class \(snapshot.deviceClass)):")
    say(String(decoding: try encoder.encode(snapshot.report), as: UTF8.self))
    snapshot.lines.forEach(say)
}

func run() async throws {
    if Array(CommandLine.arguments.dropFirst()) == ["--print-caps"] {
        try await printCaps()
        return
    }
    var options: [String: String] = [:]
    var arguments = CommandLine.arguments.dropFirst()
    while let name = arguments.popFirst() {
        guard name.hasPrefix("--"), let value = arguments.popFirst() else { throw CheckFailure("unknown argument \(name)") }
        options[String(name.dropFirst(2))] = value
    }
    guard let server = options["server"], let admin = options["admin"],
          let client = AppClient(rawValue: options["client"] ?? ""), client != .macos
    else { throw CheckFailure("usage: pley-apple-check --server <origin> --client tvos|ios --admin <username>") }
    guard let password = ProcessInfo.processInfo.environment["PLEY_CHECK_PASSWORD"], !password.isEmpty else {
        throw CheckFailure("set PLEY_CHECK_PASSWORD to the administrator's password")
    }

    // The app's User-Agent for a TV or a phone; the server takes the grant's
    // device from it, and its name from the app's built-in client document.
    let (system, device, appName) = client == .tvos
        ? ("tvOS 18.2", "Apple TV", "Pley for Apple TV")
        : ("iOS 18.1", "iPhone", "Pley for iPhone and iPad")
    let agent = UserAgent.format(version: "0.1.0", osName: system.split(separator: " ")[0].description,
                                 osVersion: system.split(separator: " ")[1].description, device: device)
    let http = HTTPTransport(userAgent: agent)
    let endpoints = EndpointClient(transport: http)
    say("client \(client.rawValue), User-Agent \(agent)")

    // 1. The server, and where it signs apps in.
    guard case .pley(let origin, let info) = await ServerProbe(transport: http).probe(server) else {
        throw CheckFailure("\(server) isn't a Pley server")
    }
    let metadata = try await ServerMetadata.fetch(origin, endpoints: endpoints)
    try check(metadata.signsInWithCodes, "the server can't sign in with a code")
    say("server \(info.name) \(info.version) at \(origin)")

    // 2. A code, as the app shows it.
    let store = MemoryTokenStore()
    let signIn = DeviceSignIn(metadata: metadata, client: client, store: store, endpoints: endpoints)
    let code = try await signIn.start()
    try check(code.verificationURL.absoluteString == origin + "/link", "the code's page is elsewhere")
    say("code shown, page \(code.verificationURL.absoluteString)")

    // 3. Allowed by the administrator on the consent page.
    var page = AdminPage(origin: origin)
    try await page.login(username: admin, password: password)
    let (verifyStatus, consent) = try await page.send("POST", "/api/oauth/device/verify",
                                                      json: ["user_code": code.userCode])
    guard verifyStatus == 200, let id = consent["id"] else {
        throw CheckFailure("the consent page refused the code (HTTP \(verifyStatus))")
    }
    let shownDevice = (consent["redirect"] as? [String: Any])?["device"] as? String
    try check(shownDevice == "\(system); \(device)", "the consent page names the device \(shownDevice ?? "nil")")
    let (allowStatus, allowed) = try await page.send("POST", "/api/oauth/requests/\(id)", json: ["allow": true])
    try check(allowStatus == 200 && allowed["done"] as? Bool == true, "allowing failed (HTTP \(allowStatus))")
    say("allowed on the consent page, which named the device \(shownDevice ?? "")")

    // 4. The app's tokens, with the server's id and this sign-in's proof key.
    let stored = try await signIn.waitForApproval(code)
    let tokens = Tokens(issuer: metadata.issuer, store: store, endpoints: endpoints)
    let api = PleyAPI(client: APIClient(origin: origin, tokens: tokens, transport: http))
    say("signed in, scope \(stored.scope)")
    let proof = try stored.proof.orThrow("the token answer has no proof key")
    try await tokens.proveServer()
    say("the server proved itself at \(origin) with this sign-in's key")
    // A key the server didn't give: what answers can't prove it, and no token goes.
    var other = stored
    other.proof = ServerProof(serverId: proof.serverId, proofId: proof.proofId,
                              proofKey: Data(repeating: 7, count: 32).base64EncodedString()
                                  .replacingOccurrences(of: "=", with: ""))
    let otherStore = MemoryTokenStore()
    try await otherStore.save(other)
    do {
        _ = try await Tokens(issuer: metadata.issuer, store: otherStore, endpoints: endpoints).accessToken()
        throw CheckFailure("a token went with a proof under another key")
    } catch let error as PleyError where error.isAnotherDevice {
        say("a proof under another key is refused: \(error.userMessage)")
    }

    // 5. Who, the household and Home.
    let me = try await api.me()
    // What was asked for: the four, and requests and approvals where the server lists them.
    try check(Set(me.scopes) == Set(metadata.scope.split(separator: " ").map(String.init)),
              "me has scope \(me.scope ?? "nil")")
    try check(!me.isAdmin, "an app's token has administrator rights")
    let profiles = try await api.householdProfiles()
    let home = try await api.home()
    say("me \(me.username), \(profiles.count) profile(s), \(home.hubs.count) row(s) on Home")

    // 6. One refresh: a new pair, and the API takes the new access token.
    let before = try await store.load(issuer: metadata.issuer)
    try await tokens.refresh(after: before?.accessToken ?? "")
    let after = try await store.load(issuer: metadata.issuer)
    try check(after?.refreshToken != nil && after?.refreshToken != before?.refreshToken, "the refresh gave no new pair")
    try check(after?.proof == proof, "the refresh changed the sign-in's proof key")
    try check(try await api.me().id == me.id, "me changed after the refresh")
    say("refreshed once, after a proof made then; the proof key is the sign-in's first")

    // 7. The server names the grant by the app's device.
    let (appsStatus, appsData) = try await page.raw("GET", "/api/me/apps")
    let apps = (try? JSONSerialization.jsonObject(with: appsData)) as? [[String: Any]] ?? []
    let grant = apps.first { $0["client_id"] as? String == client.clientID }
    try check(appsStatus == 200 && grant != nil, "the grant isn't in /api/me/apps (HTTP \(appsStatus))")
    try check(grant?["name"] as? String == appName, "the grant is named \(grant?["name"] ?? "nil")")
    try check(grant?["device"] as? String == "\(system); \(device)", "the grant's device is \(grant?["device"] ?? "nil")")
    say("grant named \(appName), device \(system); \(device)")

    // 8. Signing out revokes: the old access token is refused.
    let last = try await store.load(issuer: metadata.issuer)?.accessToken ?? ""
    await tokens.signOut()
    let (meStatus, _) = try await page.raw("GET", "/api/auth/me", asPage: false, bearer: last)
    try check(meStatus == 401, "the token still works after signing out (HTTP \(meStatus))")
    try check(try await store.load(issuer: metadata.issuer) == nil, "tokens kept after signing out")
    say("signed out; the old token gets 401")

    // 9. The administrator's session ends too.
    let (logoutStatus, _) = try await page.raw("POST", "/api/auth/logout")
    try check((200..<300).contains(logoutStatus), "logging out failed (HTTP \(logoutStatus))")
    say("ok")
}

do {
    try await run()
} catch {
    let words = (error as? CheckFailure)?.description ?? (error as? LocalizedError)?.errorDescription
        ?? String(describing: type(of: error))
    FileHandle.standardError.write(Data("pley-apple-check: \(words)\n".utf8))
    exit(1)
}
#else
// Only the Mac runs the check; elsewhere the package still builds.
print("pley-apple-check runs on macOS only")
#endif
