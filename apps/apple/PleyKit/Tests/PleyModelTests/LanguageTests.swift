import Foundation
@testable import PleyModel
import Testing

/// The language shown, as the Samsung apps and the Android app decide it:
/// every case of packages/tv/test/fixtures/language.json.
@Suite("Which language shows")
struct LanguageTests {
    private struct Case: Decodable, CustomTestStringConvertible {
        var account: String?
        var choice: String?
        var server: String?
        var device: [String]
        var out: String

        var testDescription: String {
            "account \(account ?? "nil"), choice \(choice ?? "nil"), server \(server ?? "nil"), device \(device)"
        }
    }

    private struct Fixture: Decodable {
        var pickLanguage: [Case]
        var pickLanguageWithChoice: [Case]
    }

    private static let fixture: Fixture = {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/tv/test/fixtures/language.json")
        // swiftlint:disable:next force_try
        return try! JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    }()

    @Test(arguments: fixture.pickLanguage)
    private func picksAsTheOtherApps(_ c: Case) {
        #expect(Language.pick(account: c.account, server: c.server, device: c.device).code == c.out)
    }

    @Test(arguments: fixture.pickLanguageWithChoice)
    private func aLanguagePickedOnTheDeviceComesBeforeItsOwn(_ c: Case) {
        let picked = Language.of(c.choice)
        #expect(Language.pick(account: c.account, server: c.server, device: c.device, picked: picked).code == c.out)
    }

    @Test func theFixtureIsRead() {
        #expect(Self.fixture.pickLanguage.count == 9 && Self.fixture.pickLanguageWithChoice.count == 6)
    }

    @Test func tagsPleyHas() {
        #expect(Language.of("fr") == .french)
        #expect(Language.of(" FR_ca") == .french)
        #expect(Language.of("es-419") == .spanish)
        #expect(Language.of("en-GB") == .english)
        #expect(Language.of("de") == nil)
        #expect(Language.of("") == nil)
        #expect(Language.of(nil) == nil)
        #expect(Language.ofDevice(["de-DE", "zh-Hans-CN", "es-MX", "fr-FR"]) == .spanish)
    }

    /// The device's own locale where it is in the language shown, so its region's ways stay.
    @Test func theLocaleKeepsTheDevicesRegion() {
        #expect(Language.locale(of: .english, device: ["fr-FR", "en-GB"]).identifier == "en-GB")
        #expect(Language.locale(of: .french, device: ["fr-CA"]).identifier == "fr-CA")
        #expect(Language.locale(of: .spanish, device: ["de-DE"]).identifier == "es")
        // en-GB's 24-hour clock, en-US's 12.
        let evening = Date(timeIntervalSince1970: 1_700_000_000 + 4 * 3600)
        let gb = evening.formatted(Date.FormatStyle(date: .omitted, time: .shortened, timeZone: .gmt)
            .locale(Language.locale(of: .english, device: ["en-GB"])))
        let us = evening.formatted(Date.FormatStyle(date: .omitted, time: .shortened, timeZone: .gmt)
            .locale(Language.locale(of: .english, device: ["en-US"])))
        #expect(!gb.contains("AM") && gb.hasSuffix(":13") && us.hasSuffix("AM"))
    }

    /// The module's catalog, read in each language whatever the system's is.
    @Test func wordsComeInTheLanguageAsked() {
        func automatic(_ language: Language) -> String {
            ShownLanguage.string("Automatic (\(Language.french.autonym))", bundle: .module, language: language,
                                 locale: Locale(identifier: language.code))
        }
        #expect(automatic(.english) == "Automatic (Français)")
        #expect(automatic(.french) == "Automatique (Français)")
        #expect(automatic(.spanish) == "Automático (Français)")
        #expect(Language.label(.spanish, automatic: .french) == "Español")
    }
}
