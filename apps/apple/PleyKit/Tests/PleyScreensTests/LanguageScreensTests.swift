import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
@testable import PleyScreens
import PleyTesting
import Testing

/// The language the app shows (docs/APPLE.md, "Languages"): the account's,
/// else this device's, else the server's, else English, as the Android
/// app's LanguageSync decides it.
@MainActor
@Suite struct LanguageScreensTests {
    /// What the app handed `ShownLanguage`, in order.
    private final class Shown {
        var languages: [Language] = []
        var locales: [Locale] = []
    }

    private func signedIn(_ server: FixtureServer, device: [String] = ["en-US"], settings: AppSettings = freshSettings(),
                          shown: Shown = Shown()) async throws -> AppModel {
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      settings: settings, deviceLanguages: { device },
                      applyLanguage: { shown.languages.append($0); shown.locales.append($1) })
        await app.start()
        return app
    }

    @Test func theAppStartsInTheLanguageShownLast() {
        let settings = freshSettings()
        settings.lastLanguage = .spanish
        let shown = Shown()
        let app = app(settings: settings, deviceLanguages: { ["fr-FR"] },
                      applyLanguage: { shown.languages.append($0); shown.locales.append($1) })
        // Before anything is known: the last one, not the device's.
        #expect(app.language == .spanish)
        #expect(shown.languages == [.spanish])
    }

    @Test func withoutAnAccountTheDevicesLanguageShowsAndTheGlobePicksAnother() async {
        let shown = Shown()
        let settings = freshSettings()
        let app = app(settings: settings, deviceLanguages: { ["de-DE", "fr-CA"] },
                      applyLanguage: { shown.languages.append($0); shown.locales.append($1) })
        await app.start()
        #expect(app.root == .servers)
        #expect(app.language == .french)
        // The device's own locale where it is in that language: fr-CA's ways stay.
        #expect(app.locale.identifier == "fr-CA")
        #expect(app.automaticLanguage == .french)
        app.pickLanguage(.spanish)
        #expect(app.language == .spanish && app.locale.identifier == "es")
        #expect(settings.pickedLanguage == .spanish && settings.lastLanguage == .spanish)
        app.pickLanguage(nil)
        #expect(app.language == .french)
        #expect(shown.languages == [.french, .spanish, .french])
    }

    @Test func theAccountsLanguageComesFirst() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setLanguage("es", profile: 2)
        let app = try await signedIn(server, device: ["fr-FR"])
        #expect(app.root == .signedIn)
        #expect(app.language == .spanish)
    }

    @Test func anAccountOnAutomaticFollowsTheDeviceThenTheServer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setServerLanguage("fr")
        let onSpanish = try await signedIn(server, device: ["es-MX"])
        #expect(onSpanish.language == .spanish)
        let onGerman = try await signedIn(server, device: ["de-DE"])
        #expect(onGerman.language == .french)
        #expect(onGerman.automaticLanguage == .french)
    }

    @Test func choosingALanguageShowsItAtOnceAndKeepsItOnTheServer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        #expect(app.language == .english)
        let home = app.homeVersion
        try await app.chooseLanguage(.french)
        #expect(app.language == .french && app.user?.language == "fr")
        #expect(server.language(profile: 2) == "fr")
        // Home's rows, which the server words, are read again.
        #expect(app.homeVersion > home)
        try await app.chooseLanguage(nil)
        #expect(app.language == .english && app.user?.language == nil)
        #expect(server.language(profile: 2) == nil)
        let put = server.requests("/api/me/language")
        #expect(put.count == 2 && put.allSatisfy { $0.method == "PUT" })
    }

    @Test func aRefusedChoiceIsPutBack() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        server.refuseLanguage(status: 500)
        await #expect(throws: (any Error).self) { try await app.chooseLanguage(.spanish) }
        #expect(app.language == .english && app.user?.language == nil)
    }

    @Test func onAServerFromBeforeLanguagesTheRowPicksOnThisDevice() async throws {
        let server = try await fixture()
        defer { server.stop() }
        server.setServerLanguage(nil)
        let settings = freshSettings()
        let app = try await signedIn(server, settings: settings)
        #expect(!app.keepsAccountLanguage)
        await app.setAccountLanguage(.french)
        #expect(app.language == .french && app.languageRowChoice == .french)
        #expect(settings.pickedLanguage == .french)
        #expect(server.requests("/api/me/language").isEmpty)
        await app.setAccountLanguage(nil)
        #expect(app.language == .english && app.languageRowChoice == nil)
    }

    @Test func aChoiceTheServerHasNoRouteForStaysOnThisDevice() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        #expect(app.keepsAccountLanguage)
        server.refuseLanguage(status: 404)
        await app.setAccountLanguage(.spanish)
        #expect(app.language == .spanish && app.languageRowChoice == .spanish)
        #expect(!app.keepsAccountLanguage && app.user?.language == nil)
        // The next choice isn't sent.
        await app.setAccountLanguage(.french)
        #expect(app.language == .french)
        #expect(server.requests("/api/me/language").count == 1)
    }

    @Test func requestsAskForTheLanguageShown() async throws {
        let server = try await fixture()
        defer { server.stop() }
        _ = try await signedIn(server)
        // The process's language: English unless something showed another.
        let code = ShownLanguage.language.code
        let me = server.requests("/api/auth/me")
        #expect(!me.isEmpty && me.allSatisfy { $0.headers["accept-language"] == code })
    }
}
