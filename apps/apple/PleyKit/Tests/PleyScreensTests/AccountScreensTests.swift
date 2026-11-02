import Foundation
import PleyAuth
import PleyCommon
import PleyData
import PleyModel
import PleyNetwork
import PleyPlayer
@testable import PleyScreens
import PleyTesting
import Testing

/// A signed-in app on `server`, at Home, with `settings` and the sign-in's `scope`.
@MainActor
private func signedIn(_ server: FixtureServer, settings: AppSettings = freshSettings(),
                      scope: String = FixtureServer.allScopes) async throws -> AppModel {
    let store = MemoryTokenStore()
    let tokens = server.issueTokens(scope: scope)
    try await store.save(StoredTokens(
        issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
        revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
        accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: tokens.refreshToken, scope: scope))
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                  settings: settings)
    await app.start()
    try #require(app.root == .signedIn)
    return app
}

/// The searches `server` was asked, in order.
private func searches(_ server: FixtureServer) -> [String] {
    server.journal.filter { $0.path == "/api/search" }.compactMap { $0.query["q"] }
}

@MainActor
@Suite struct SearchModelTests {
    @Test func typingIsAskedForOnceItPauses() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = SearchModel(app: app, debounce: .milliseconds(150))
        for text in ["d", "du", "dun", "dune"] {
            model.query = text
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(searches(server).isEmpty)
        #expect(await eventually { if case .ready = model.state { true } else { false } })
        #expect(searches(server) == ["dune"])
        #expect(server.journal.last { $0.path == "/api/search" }?.query["limit"] == "40")
    }

    @Test func onlyTheLatestAnswerCountsAndResultsStayWhileTheNextLoad() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = SearchModel(app: app, debounce: .milliseconds(10))
        model.query = "dune"
        #expect(await eventually { model.state.groups.count == 3 })
        // A slow answer for "villeneuve", then "dune: part" typed before it comes.
        server.delaySearch("villeneuve", ms: 600)
        model.query = "villeneuve"
        #expect(await eventually { searches(server).last == "villeneuve" })
        // The results shown stay, marked as loading.
        if case .ready(let query, let groups, let loading) = model.state {
            #expect(query == "dune")
            #expect(!groups.isEmpty)
            #expect(loading)
        } else {
            Issue.record("the results went while the next loaded: \(model.state)")
        }
        model.query = "dune: part"
        #expect(await eventually { if case .ready("dune: part", _, false) = model.state { true } else { false } })
        // The slow answer comes after, and is dropped.
        try await Task.sleep(for: .milliseconds(800))
        guard case .ready(let query, let groups, _) = model.state else {
            Issue.record("no results: \(model.state)")
            return
        }
        #expect(query == "dune: part")
        #expect(groups.map(\.kind) == [.movies])
        #expect(groups.first?.tiles.map(\.item.title) == ["Dune: Part Two"])
    }

    @Test func resultsComeInGroupsWithCountsAndChannelsLast() throws {
        let results = try PleyJSON.decoder.decode(SearchResults.self, from: Fixtures.data("search/results.json"))
        #expect(!results.channels.isEmpty)
        let groups = SearchModel.groups(results)
        #expect(groups.first?.kind == .people)
        #expect(groups.map(\.kind) == SearchModel.GroupKind.allCases.filter { groups.map(\.kind).contains($0) })
        #expect(!groups.contains { $0.count == 0 })
        let people = try #require(groups.first)
        #expect(people.count == results.people.count)
        #expect(people.spoken == "People, \(Words.pluralize(results.people.count, "result"))")
        let movies = try #require(groups.first { $0.kind == .movies })
        #expect(movies.tiles.allSatisfy { $0.shape == .poster && $0.item.kind == .movie })
        if let episodes = groups.first(where: { $0.kind == .episodes }) {
            #expect(episodes.tiles.allSatisfy { $0.shape == .landscape })
        }
        // Live TV's channels last, a dozen at most.
        let channels = try #require(groups.last)
        #expect(channels.kind == .channels && channels.kind.title == "Live TV")
        #expect(channels.channels == Array(results.channels.prefix(SearchModel.channels)))
        var many = results
        many.channels = (1...20).map { LiveChannel(id: $0, name: "Channel \($0)") }
        #expect(SearchModel.groups(many).last?.count == 12)
    }

    @Test func noResultsAndAFailureSaySoAndRetryAsksAgain() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = SearchModel(app: app, debounce: .milliseconds(10))
        model.query = "north"
        #expect(await eventually { model.state == .empty("north") })
        #expect(SearchModel.noResults("north") == "No results for \u{201C}north\u{201D}")
        server.answer(path: "/api/search", status: 500, json: #"{"code":"internal","message":"the index is busy"}"#)
        model.query = "dune"
        #expect(await eventually { if case .failed("dune", _) = model.state { true } else { false } })
        server.clearAnswer(path: "/api/search")
        model.retry()
        #expect(await eventually { model.state.groups.count == 3 })
        // Nothing typed: back to the recent searches.
        model.query = "  "
        #expect(model.state == .idle)
    }

    @Test func recentSearchesAreTheProfilesLastTenNewestFirst() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        let app = try await signedIn(server, settings: settings)
        let model = SearchModel(app: app, debounce: .milliseconds(10))
        for n in 1...12 {
            model.query = "query \(n)"
            model.submit()
        }
        #expect(model.recent.count == 10)
        #expect(model.recent.first == "query 12")
        #expect(model.recent.last == "query 3")
        // Again: first, and once.
        model.searchAgain("Query 5")
        #expect(model.recent.first == "Query 5")
        #expect(model.recent.count == 10)
        #expect(model.recent.filter { $0.lowercased() == "query 5" }.count == 1)

        // Opening a result keeps its query.
        model.query = "dune"
        #expect(await eventually { model.state.groups.count == 3 })
        let tile = try #require(model.state.groups.first { $0.kind == .movies }?.tiles.first)
        #expect(model.open(tile) == .item(tile.item.id))
        #expect(model.recent.first == "dune")

        // Another profile has its own.
        let origin = try #require(app.server?.origin)
        #expect(settings.recentSearches(origin: origin, profileId: 4).isEmpty)
        settings.rememberSearch("cartoons", origin: origin, profileId: 4)
        #expect(settings.recentSearches(origin: origin, profileId: 2).first == "dune")
        #expect(settings.recentSearches(origin: origin, profileId: 4) == ["cartoons"])

        model.clearRecent()
        #expect(model.recent.isEmpty)
        #expect(settings.recentSearches(origin: origin, profileId: 4) == ["cartoons"])
    }
}

@MainActor
@Suite struct MeTrackDefaultsTests {
    @Test func aChangeShowsAtOnceAndIsSavedOnTheServer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = MeModel(app: app)
        // Alex's, from the fixture.
        #expect(model.trackDefaults == TrackDefaults(audioLanguage: "eng", subtitleMode: .foreign))
        #expect(model.editable)
        #expect(model.audioLabel == "English")
        #expect(model.subtitleLanguageLabel == "Same as the audio language")
        #expect(model.showsSubtitleLanguage)

        await model.setSubtitleMode(.always)
        #expect(model.trackDefaults.subtitleMode == .always)
        #expect(server.trackDefaults(profile: 2)?.mode == "always")
        #expect(server.trackDefaults(profile: 2)?.audio == "eng")
        #expect(app.user?.subtitleMode == "always")
        #expect(app.shell.toast?.text == "Subtitles saved")

        await model.setSubtitleLanguage("fre")
        #expect(model.subtitleLanguageLabel == "French")
        #expect(server.trackDefaults(profile: 2)?.subtitles == "fre")
        await model.setAudioLanguage(nil)
        #expect(model.audioLabel == "The file's default")
        #expect(server.trackDefaults(profile: 2)?.audio == nil)

        await model.setSubtitleMode(.off)
        #expect(!model.showsSubtitleLanguage)
        #expect(MeModel.modes.map(MeModel.modeLabel)
            == ["Off", "Only for foreign dialogue", "When the audio is in another language", "Always"])
        #expect(MeModel.languages.first?.name == "Arabic")
        #expect(MeModel.languages.contains { $0.code == "jpn" && $0.name == "Japanese" })
    }

    @Test func aRefusedChangeComesBackWithWhy() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = MeModel(app: app)
        server.refuseTrackDefaults(status: 500)
        await model.setSubtitleMode(.off)
        #expect(model.trackDefaults.subtitleMode == .foreign)
        #expect(app.user?.subtitleMode == "foreign")
        #expect(app.shell.toast?.text.hasPrefix("Couldn't save") == true)
    }

    @Test func withoutPlayTheyAreReadOnly() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server, scope: "library playlists profiles")
        let model = MeModel(app: app)
        #expect(!model.editable)
        #expect(model.trackDefaults.subtitleMode == .foreign)
        await model.setSubtitleMode(.always)
        #expect(model.trackDefaults.subtitleMode == .foreign)
        #expect(!server.journal.contains { $0.path == "/api/me/track-defaults" })
    }
}

@MainActor
@Suite struct HistoryModelTests {
    @Test func playsComeFifteenAtATimeNewestFirst() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let now = 1_761_250_000 + 3 * 3_600
        let model = HistoryModel(app: app, now: { now })
        await model.load()
        #expect(model.state == .loaded)
        #expect(model.rows.count == 15)
        #expect(model.total == FixtureServer.historyCount)
        #expect(model.canLoadMore)
        #expect(server.journal.last { $0.path == "/api/history" }?.query == ["offset": "0", "limit": "15"])
        let first = try #require(model.rows.first)
        #expect(first.title == "Severance")
        #expect(first.subtitle == "S1 · E1 · Good News About Hell")
        #expect(first.time == "3 hours ago")
        #expect(first.device == "pley for Android on Pixel 9")
        #expect(first.opens == .item(203))
        #expect(first.pictures.first == "/api/items/202/images/poster?v=1761200400")
        // A track opens its album, with the track.
        let track = try #require(model.rows.first { $0.square })
        #expect(track.opens == .album(302, track: 303))

        await model.loadMore()
        #expect(model.rows.count == 30)
        #expect(server.journal.last { $0.path == "/api/history" }?.query == ["offset": "15", "limit": "15"])
        await model.loadMore()
        #expect(model.rows.count == 40)
        #expect(!model.canLoadMore)
        #expect(Set(model.rows.map(\.id)).count == 40)
    }

    @Test func aRemovedPlayGoesAtOnceAndComesBackIfRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = HistoryModel(app: app)
        await model.load()
        let second = model.rows[1].id
        await model.remove(second)
        #expect(!model.rows.contains { $0.id == second })
        #expect(model.total == FixtureServer.historyCount - 1)
        #expect(!server.historyIds.contains(second))

        server.refuseHistoryRemoval(status: 403)
        let first = model.rows[0].id
        await model.remove(first)
        #expect(model.rows.first?.id == first)
        #expect(model.total == FixtureServer.historyCount - 1)
        #expect(server.historyIds.contains(first))
        #expect(app.shell.toast?.text.hasPrefix("Couldn't remove it") == true)
    }

    @Test func relativeTimesAreWordedAsOnAndroid() {
        let now = 1_000_000_000
        #expect(Words.relative(now, now: now) == "now")
        #expect(Words.relative(now - 180, now: now) == "3 minutes ago")
        #expect(Words.relative(now - 3 * 3_600, now: now) == "3 hours ago")
        #expect(Words.relative(now - 86_400, now: now) == "yesterday")
        #expect(Words.relative(now - 8 * 86_400, now: now) == "last week")
        #expect(Words.relative(now + 5 * 3_600, now: now) == "in 5 hours")
        #expect(Words.relative(nil, now: now) == "never")
    }
}

@MainActor
@Suite struct SettingsModelTests {
    @Test func settingsAreKeptAndReadBack() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        let app = try await signedIn(server, settings: settings)
        let model = SettingsModel(app: app, offersCellular: true)
        #expect(model.qualityOther == nil)
        #expect(model.autoplayNext)
        #expect(model.subtitles == SubtitleOverrides())
        #expect(model.gridDensity == .medium)
        #expect(model.privateSignIn)
        #expect(model.qualityTitle(cellular: false) == "Quality on Wi-Fi")
        #expect(SettingsModel(app: app, offersCellular: false).qualityTitle(cellular: false) == "Quality")
        #expect(SettingsModel.qualities.map(\.label).prefix(3) == ["Automatic", "Original", "30 Mbps 4K"])
        #expect(SettingsModel.qualityLabel("4m") == "4 Mbps 720p")

        model.setQuality("4m", cellular: false)
        model.setQuality("2m", cellular: true)
        model.setAutoplayNext(false)
        model.setSubtitleSize(.p150)
        model.setSubtitleBackground(.darkBox)
        model.setSubtitleFont(.serif)
        model.setSubtitleRaise(.ten)
        model.setGridDensity(.large)
        model.setPrivateSignIn(false)

        // A new model reads what was kept.
        let again = SettingsModel(app: app, offersCellular: true)
        #expect(again.qualityOther == "4m")
        #expect(again.qualityCellular == "2m")
        #expect(!again.autoplayNext)
        #expect(again.subtitles == SubtitleOverrides(size: .p150, background: .darkBox, font: .serif, raise: .ten))
        #expect(again.gridDensity == .large)
        #expect(!again.privateSignIn)

        // Automatic is nothing kept.
        again.setQuality(nil, cellular: false)
        #expect(settings.quality(cellular: false) == nil)
        #expect(SubtitleSize.p150.label == "150 %")
        #expect(SubtitleRaise.ten.label == "Raised 10 %")
    }

    @Test func theQualityChosenIsThePlayersQuality() {
        let name = "pley-test-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defer { defaults.removePersistentDomain(forName: name) }
        let settings = AppSettings(defaults: defaults)
        settings.setQuality("4m", cellular: false)
        settings.setQuality("original", cellular: true)
        #expect(DefaultsQuality(defaults: defaults, cellular: { false }).remembered() == "4m")
        #expect(DefaultsQuality(defaults: defaults, cellular: { true }).remembered() == "original")
        // And the player's menu choice shows in Settings.
        DefaultsQuality(defaults: defaults, cellular: { false }).remember("8m")
        #expect(settings.quality(cellular: false) == "8m")
        DefaultsQuality(defaults: defaults, cellular: { false }).remember("auto")
        #expect(settings.quality(cellular: false) == nil)
        settings.setQuality(nil, cellular: false)
        #expect(DefaultsQuality(defaults: defaults, cellular: { false }).remembered() == nil)
        #expect(Qualities.choose(nil, remembered: nil).automatic)
    }

    @Test func autoplayGoesToThePlayer() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        settings.autoplayNext = false
        let app = try await signedIn(server, settings: settings)
        server.addPlayable(FixtureServer.Playable(itemId: 41, fileId: 410, title: "Two Audio", durationMs: 90_000))
        let player = try #require(await app.play(PlayRequest(itemId: 41)))
        #expect(!player.autoplaysNext)
        await app.closePlayer()
    }

    @Test func alwaysStartAsIsPerServerAndOffersTheHousehold() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        let app = try await signedIn(server, settings: settings)
        let model = SettingsModel(app: app, offersCellular: false)
        #expect(!model.offersStartAs)
        await model.load()
        #expect(model.offersStartAs)
        #expect(model.startChoices.map(\.label) == ["Ask who's watching", "Alex", "Sam", "Kids", "Robin", "Guest"])
        model.setAlwaysStartAs(4)
        #expect(model.startAsLabel == "Kids")
        let origin = try #require(app.server?.origin)
        #expect(settings.alwaysStartAs(origin: origin) == 4)
        #expect(settings.alwaysStartAs(origin: "http://elsewhere:7539") == nil)
        model.setAlwaysStartAs(nil)
        #expect(settings.alwaysStartAs(origin: origin) == nil)
    }

    @Test func useTheServersOrderSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = SettingsModel(app: app, offersCellular: false)
        await model.useServerOrder()
        #expect(server.journal.contains { $0.method == "DELETE" && $0.path == "/api/me/libraries/order" })
        #expect(app.shell.toast?.text == "Libraries are in the server's order")
    }

    @Test func diagnosticsShowTheReportAsIndentedJSONAndAboutSaysTheVersion() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = DiagnosticsModel(app: app)
        await model.load()
        #expect(!model.deviceClass.isEmpty)
        #expect(!model.lines.isEmpty)
        #expect(model.json.hasPrefix("{\n"))
        #expect(model.json.contains("\"video\""))
        let about = AboutModel(info: ["CFBundleShortVersionString": "0.1.0", "CFBundleVersion": "12"],
                               device: "iPhone and iPad")
        #expect(about.title == "Pley 0.1.0 (build 12) for iPhone and iPad")
        #expect(AboutModel.licences.contains("no third-party code"))
    }
}

@MainActor
@Suite struct StartGateTests {
    @Test func theRules() {
        // iPhone, iPad and Mac open Home.
        #expect(StartGate.decide(tv: false, household: [2, 3, 4], alwaysStartAs: nil, current: 2) == .home)
        // Apple TV asks when the household has several profiles.
        #expect(StartGate.decide(tv: true, household: [2, 3, 4], alwaysStartAs: nil, current: 2) == .profiles)
        // A household of one, or one not seen yet, opens Home.
        #expect(StartGate.decide(tv: true, household: [2], alwaysStartAs: nil, current: 2) == .home)
        #expect(StartGate.decide(tv: true, household: [], alwaysStartAs: nil, current: 2) == .home)
        // Always start as.
        #expect(StartGate.decide(tv: true, household: [2, 3, 4], alwaysStartAs: 2, current: 2) == .home)
        #expect(StartGate.decide(tv: true, household: [2, 3, 4], alwaysStartAs: 4, current: 2) == .switchTo(4))
        // A profile that went asks.
        #expect(StartGate.decide(tv: true, household: [2, 3], alwaysStartAs: 4, current: 2) == .profiles)
    }

    private func tvApp(_ server: FixtureServer, settings: AppSettings) async throws -> AppModel {
        let store = MemoryTokenStore()
        try await keepSignIn(server, in: store)
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      settings: settings, tv: true)
        await app.start()
        return app
    }

    @Test func appleTVAsksWhosWatchingAtStart() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        // The first start knows no household: Home.
        let first = try await tvApp(server, settings: settings)
        #expect(first.root == .signedIn)
        await ProfilesModel(app: first).load()
        // Once it is known, the next start asks.
        let second = try await tvApp(server, settings: settings)
        #expect(second.root == .profiles)
        #expect(!second.profilesFromShell)
    }

    @Test func appleTVStartsAsTheProfileChosen() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        let first = try await tvApp(server, settings: settings)
        await ProfilesModel(app: first).load()
        let origin = try #require(first.server?.origin)
        settings.setAlwaysStartAs(4, origin: origin)
        let app = try await tvApp(server, settings: settings)
        #expect(app.root == .signedIn)
        #expect(app.user?.id == 4)

        // One with a PIN can't be switched to without it: "Who's watching?".
        settings.setAlwaysStartAs(3, origin: origin)
        let pinned = try await tvApp(server, settings: settings)
        #expect(pinned.root == .profiles)
    }

    @Test func theStartGateAsksForThePinOfTheProfileInUse() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        let first = try await tvApp(server, settings: settings)
        await ProfilesModel(app: first).load()

        // Signed in as Sam, whose PIN is 1234: the gate asks for it.
        let store = MemoryTokenStore()
        let tokens = server.issueTokens(profile: 3)
        try await store.save(StoredTokens(
            issuer: server.origin, clientID: AppClient.ios.clientID, tokenEndpoint: server.origin + "/api/oauth/token",
            revocationEndpoint: server.origin + "/api/oauth/revoke", accessToken: tokens.accessToken,
            accessExpiresAt: Date().addingTimeInterval(3600), refreshToken: tokens.refreshToken,
            scope: FixtureServer.allScopes))
        let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())],
                      settings: settings, tv: true)
        await app.start()
        #expect(app.root == .profiles)
        #expect(app.profilesAtStart)
        let model = ProfilesModel(app: app)
        await model.load()
        let sam = try #require(model.profiles.first { $0.id == 3 })
        #expect(sam.current)
        await model.choose(sam)
        #expect(model.pinFor?.id == 3)
        #expect(app.root == .profiles)

        // A wrong PIN keeps the gate shut.
        model.pin = "9999"
        await model.submitPin()
        #expect(model.pinError == "wrong PIN")
        #expect(app.root == .profiles)

        model.pin = "1234"
        await model.submitPin()
        #expect(model.pinFor == nil)
        #expect(app.root == .signedIn)
        #expect(app.user?.id == 3)
        #expect(!app.profilesAtStart)

        // From Switch profile, the profile in use goes back without its PIN.
        app.showProfiles()
        await model.choose(sam)
        #expect(model.pinFor == nil)
        #expect(app.root == .signedIn)
    }

    @Test func theStartGateLetsAProfileWithoutAPinThrough() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let settings = freshSettings()
        let first = try await tvApp(server, settings: settings)
        await ProfilesModel(app: first).load()
        let app = try await tvApp(server, settings: settings)
        #expect(app.root == .profiles)
        let model = ProfilesModel(app: app)
        await model.load()
        let alex = try #require(model.profiles.first { $0.current })
        await model.choose(alex)
        #expect(model.pinFor == nil)
        #expect(app.root == .signedIn)
    }
}
