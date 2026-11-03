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

/// A signed-in app on `server`, at Home, with its event stream.
@MainActor
private func signedIn(_ server: FixtureServer) async throws -> AppModel {
    let store = MemoryTokenStore()
    try await keepSignIn(server, in: store)
    let app = app(store, servers: [SavedServer(origin: server.origin, name: "Living Room", lastUsed: Date())])
    await app.start()
    try #require(app.root == .signedIn)
    return app
}

/// A fixture's item.
private func detail(_ path: String) throws -> ItemDetail {
    try PleyJSON.decoder.decode(ItemDetail.self, from: Fixtures.data(path))
}

private func list(_ path: String) throws -> [ItemSummary] {
    try PleyJSON.decoder.decode(ItemList.self, from: Fixtures.data(path)).items
}

private let english = Locale(identifier: "en_US")

/// The page port, for each kind of item in the fixtures (Android's `ItemPagesTest`).
@Suite struct ItemPageTests {
    @Test func aMovieInProgress() throws {
        let arrival = try detail("items/movie_detail.json")
        let page = ItemPage(PageInput(detail: arrival, related: try list("items/related.json"),
                                      partOf: [CollectionRef(id: 802, title: "Villeneuve Picks")], locale: english))
        #expect(page.title == "Arrival")
        #expect(page.context == nil)
        #expect(page.meta == [.text("2016"), .text("1h 56m"), .contentRating("PG-13"), .rating("7.6"),
                              .badges(["4K", "DV P8.1", "Atmos"])])
        #expect(page.primary == PrimaryAction(.resume, progress: 2_412_000 / 6_985_000, timeLeft: "1h 16m left"))
        #expect(page.primary.label == "Resume")
        #expect(page.fromStart)
        #expect(!page.watched && page.markable)
        #expect(page.myList == false)
        #expect(page.userRating == 9 && page.rateable)
        #expect(page.tagline == "Why are they here?")
        #expect(page.trailer?.id == 102)
        #expect(page.extras.map(\.id) == [102, 103])
        #expect(page.castTitle == "Cast")
        #expect(page.cast.map(\.name) == ["Amy Adams", "Jeremy Renner", "Forest Whitaker"])
        #expect(page.related.map(\.id) == [106, 105])
        #expect(page.partOf.map(\.title) == ["Villeneuve Picks"])
        #expect(page.seasons == nil)
        #expect(page.genres == ["Drama", "Science Fiction", "Mystery"])

        // Two versions, two audio tracks picked by the profile, three subtitle tracks.
        let tracks = try #require(page.tracks)
        #expect(tracks.versions.map(\.id) == [501, 502])
        #expect(tracks.versionLabel?.hasPrefix("4K · HEVC · MKV") == true)
        #expect(tracks.versions[1].label.hasPrefix("Open Matte · 1080p"))
        #expect(!tracks.automaticAudio)
        #expect(tracks.audio.count == 2 && tracks.audioIndex == 1)
        #expect(tracks.audioLabel == "English – TrueHD Atmos 7.1 (TrueHD Atmos 7.1)")
        #expect(tracks.subtitles.map(\.id) == [3, 4, 1000])
        #expect(tracks.subtitleIndex == 4)
        #expect(tracks.subtitleLabel == "English – Forced (SRT, Forced)")

        // Directors and writers, not producers.
        #expect(page.details.crew.map(\.label) == ["Director", "Writer"])
        #expect(page.details.crew.map(\.names) == ["Denis Villeneuve", "Eric Heisserer"])
        #expect(page.details.originalTitle == nil)
        #expect(page.details.released == Words.date("2016-11-11"))
        #expect(page.details.studio == "Lava Bear Films" && page.details.studioLabel == "Studio")
        #expect(page.details.plays == nil)
        #expect(page.details.versions.first?.label == "4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 29.7 GB")
        #expect(page.details.rows.map(\.label) == ["Released", "Studio"] + (page.details.versions.count > 1 ? ["Versions"] : []))

        // Media info: file names only.
        #expect(page.mediaInfo.map(\.title) == ["Version 1", "Open Matte"])
        let info = page.mediaInfo[0]
        #expect(info.rows.map(\.label) == ["File", "Container", "Size", "Duration", "Bitrate", "Video", "Audio"])
        #expect(info.rows.map(\.value) == ["Arrival (2016).mkv", "MKV", "29.7 GB", "1h 56m", "37 Mbps",
                                           "HEVC · 4K · Dolby Vision P8.1", "TrueHD"])
        #expect(info.streams.map(\.kind) == ["Video", "Audio", "Audio", "Subtitles", "Subtitles", "Subtitles"])
        #expect(info.streams[0].details == "4K · 3840×2160 · 23.98 fps · 10-bit · Dolby Vision P8.1 · Main 10 · Default")
        #expect(info.streams[2].details == "Stereo · 48.0 kHz · 192 kbps")
        #expect(info.streams[5].details == "Text · External")
        #expect(page.spokenMeta == "2016, 1h 56m, rated PG-13, 7.6 out of 10, 4K, DV P8.1, Atmos")
    }

    @Test func aShowPlaysItsNextEpisodeAndOpensOnItsSeason() throws {
        let severance = try detail("items/show_detail.json")
        let seasons = try list("items/children_seasons.json")
        let onDeck = try PleyJSON.decoder.decode(MaybeItem.self, from: Fixtures.data("items/on_deck.json")).item
        let season = ItemPage.defaultSeason(seasons, onDeck: onDeck)
        #expect(season == 202)
        let page = ItemPage(PageInput(detail: severance, onDeck: onDeck, onDeckKnown: true, seasons: seasons,
                                      selectedSeason: season, episodes: try list("items/children_episodes.json"),
                                      locale: english))
        #expect(page.meta == [.text("2 seasons"), .text("2022-2025"), .contentRating("TV-MA"), .rating("8.4")])
        #expect(page.primary.kind == .resume)
        #expect(page.primary.label == "Resume S1 · E2")
        #expect(page.primary.timeLeft == "34 min left")
        // From start plays that episode from its start.
        #expect(page.fromStart)
        #expect(!page.watched)
        #expect(page.tracks == nil)
        #expect(page.trailer?.id == 207)
        #expect(page.details.crew.map(\.label) == ["Created by"])
        #expect(page.details.studioLabel == "Network")
        let section = try #require(page.seasons)
        #expect(section.seasons.map(\.title) == ["Season 1", "Season 2"])
        #expect(section.seasons.map(\.episodes) == ["9 episodes", "10 episodes"])
        #expect(section.selectedTab?.id == 202)
        #expect(section.focus == 204)
        let episodes = try #require(section.episodes)
        #expect(episodes.map(\.title) == ["1. Good News About Hell", "2. Half Loop", "3. In Perpetuity"])
        #expect(episodes[0].watched && episodes[0].length == "57 min")
        #expect(episodes[0].line == "57 min · Feb 18, 2022")
        #expect(episodes[1].length == "34 min left" && episodes[1].progress > 0.3)
        #expect(episodes[1].longLabel == "Season 1 · Episode 2")
        #expect(episodes[0].spokenValue == "57 minutes, watched")
    }

    @Test func theDefaultSeasonSkipsSpecialsWithoutANextEpisode() {
        func season(_ id: Int, _ index: Int) -> ItemSummary {
            var s = ItemSummary(id: id, libraryId: 2, kind: .season, title: "")
            s.index = index
            return s
        }
        let seasons = [season(1, 0), season(2, 1), season(3, 2)]
        #expect(ItemPage.defaultSeason(seasons, onDeck: nil) == 2)
        var next = ItemSummary(id: 9, libraryId: 2, kind: .episode, title: "")
        next.parentId = 3
        #expect(ItemPage.defaultSeason(seasons, onDeck: next) == 3)
        next.parentId = 99
        #expect(ItemPage.defaultSeason(seasons, onDeck: next) == 2)
        #expect(ItemPage.defaultSeason([season(1, 0)], onDeck: nil) == 1)
        #expect(ItemPage.defaultSeason([], onDeck: nil) == nil)
    }

    @Test func aSeasonIsTitledByItsNumberUnderItsShow() throws {
        let season = try #require(try list("items/children_seasons.json").first)
        let page = ItemPage(PageInput(detail: ItemDetail(summary: season), episodes: [], locale: english))
        #expect(page.title == "Season 1")
        #expect(page.context == ItemContext(showId: 201, showTitle: "Severance"))
        #expect(page.meta == [.text("2022"), .text("9 episodes")])
        #expect(page.myList == nil)
        #expect(!page.rateable)
        // Not on deck yet, but it has episodes.
        #expect(page.primary.kind == .play)
        var empty = ItemDetail(summary: season)
        empty.summary.leafCount = 0
        #expect(ItemPage(PageInput(detail: empty)).primary.label == "Nothing to play")
    }

    @Test func anEpisodeHasGuestStarsAndAutomaticAudio() throws {
        let episode = try detail("items/episode_detail.json")
        let page = ItemPage(PageInput(detail: episode, selectedSeason: 202,
                                      episodes: try list("items/children_episodes.json"), locale: english))
        #expect(page.context == ItemContext(showId: 201, showTitle: "Severance", seasonId: 202,
                                            seasonTitle: "Season 1", episodeLabel: "S1 · E1",
                                            episodeLongLabel: "Season 1 · Episode 1"))
        #expect(page.meta == [.text(Words.date("2022-02-18", locale: english)), .text("57 min"), .rating("8.0"),
                              .badges(["1080p", "5.1"])])
        #expect(page.castTitle == "Guest Stars")
        #expect(page.watched && page.primary.kind == .play && !page.fromStart)
        #expect(page.myList == nil)
        #expect(page.related.isEmpty)
        #expect(page.details.released == nil)
        let tracks = try #require(page.tracks)
        // Nothing picked: Automatic, in English, first and ticked.
        #expect(tracks.automaticAudio)
        #expect(tracks.automaticLabel == "Automatic (English)")
        #expect(tracks.audioLabel == "Automatic (English)")
        #expect(tracks.audio.allSatisfy { !$0.selected })
        #expect(tracks.subtitleLabel == "Off")
        let section = try #require(page.seasons)
        #expect(section.seasons.isEmpty && section.selected == 202 && section.focus == 203)
    }

    @Test func crewLinesNameFourThenSayHowManyMore() {
        var item = ItemDetail(summary: ItemSummary(id: 1, libraryId: 1, kind: .movie, title: "Anthology"))
        item.crew = (1...6).map { Credit(id: $0, name: "D\($0)", role: .director) }
            + [Credit(id: 1, name: "D1", role: .director), Credit(id: 9, name: "W", role: .writer)]
        let crew = ItemPage(PageInput(detail: item)).details.crew
        #expect(crew.map(\.label) == ["Directors", "Writer"])
        #expect(crew[0].people.count == 4 && crew[0].more == 2)
        #expect(crew[0].names == "D1, D2, D3, D4 and 2 more")
    }

    @Test func aMovieWithoutFilesHasNothingToPlay() {
        let item = ItemDetail(summary: ItemSummary(id: 1, libraryId: 1, kind: .movie, title: "Lost"))
        let page = ItemPage(PageInput(detail: item))
        #expect(page.primary.label == "Nothing to play")
        #expect(page.tracks == nil && page.mediaInfo.isEmpty)
    }

    @Test func peoplesPartsInWords() {
        #expect(Words.roleText([PersonRole(role: .actor, character: "Neo")]) == "as Neo")
        #expect(Words.roleText([PersonRole(role: .actor, episodes: 3)]) == "Guest star (3 episodes)")
        #expect(Words.roleText([PersonRole(role: .actor)]) == "Actor")
        #expect(Words.roleText([PersonRole(role: .director, episodes: 6), PersonRole(role: .producer)])
            == "Director (6 episodes), Producer")
        #expect(Words.roleText([PersonRole(role: CreditRole(rawValue: "gaffer"))]) == "Crew")
    }

    @Test func extrasAndMusicOpenTheirOwnPages() throws {
        let extra = try detail("items/extra.json")
        #expect(ItemModel.elsewhere(extra) == .item(101))
        #expect(Screen.page(of: extra.summary) == .item(101))
        let track = try detail("items/track_detail.json")
        #expect(ItemModel.elsewhere(track) == .album(302, track: 303))
        #expect(ItemModel.elsewhere(try detail("items/movie_detail.json")) == nil)
    }
}

/// The item page against the fixture server: what it reads, and what it changes.
@MainActor
@Suite struct ItemPageModelTests {
    @Test func aMoviesPageReadsWhatItShowsBesideIt() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 101)
        model.locale = english
        await model.load()
        #expect(model.state == .loaded)
        let page = try #require(model.page)
        #expect(page.related.map(\.id) == [106, 105])
        #expect(page.partOf.map(\.title) == ["Villeneuve Picks"])
        #expect(page.cast.count == 3)
        #expect(server.requests("/api/items/101/related").first?.query["limit"] == "20")
        // The page's file is the one Play plays.
        #expect(page.tracks?.versionId == model.file?.id)
        #expect(model.request() == PlayRequest(itemId: 101))
        #expect(model.request(for: try #require(page.trailer)) == PlayRequest(itemId: 102))
    }

    @Test func aPageOpenedAfterAChangeReadsOnce() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        #expect(await eventually { server.openEventStreams == 1 })
        // Back from the background: everything shown was read again.
        app.note(ServerChange(home: true, libraries: true, watchlist: true, everything: true))
        #expect(await eventually { app.change != nil })
        // A page opened afterwards doesn't take that change as its own.
        let model = ItemModel(app: app, itemId: 101)
        await model.load()
        try await Task.sleep(for: .milliseconds(300))
        #expect(server.requests("/api/items/101").count == 1)
        // Nor does Home (on iPad, made again at each sidebar switch).
        let home = HomeModel(app: app)
        await home.reload()
        try await Task.sleep(for: .milliseconds(300))
        #expect(server.requests("/api/hubs/home").count == 1)
    }

    @Test func aShowsPageListsTheSeasonOfItsNextEpisodeAndAnotherOnRequest() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 201)
        await model.load()
        let page = try #require(model.page)
        #expect(page.primary.label == "Resume S1 · E2")
        #expect(page.seasons?.selected == 202)
        #expect(page.seasons?.episodes?.count == 3)
        #expect(page.myList == true)
        // Play plays the next episode.
        #expect(model.playable?.id == 204)
        model.selectSeason(206)
        #expect(model.page?.seasons?.selected == 206)
        #expect(model.page?.seasons?.episodes == nil)
        #expect(await eventually { model.page?.seasons?.episodes == [] })
        model.selectSeason(999)
        #expect(model.page?.seasons?.selected == 206)
    }

    @Test func extrasAndTracksSendThePageElsewhere() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let extra = ItemModel(app: app, itemId: 102)
        await extra.load()
        #expect(extra.elsewhere == .item(101))
        #expect(extra.page == nil)
        let track = ItemModel(app: app, itemId: 303)
        await track.load()
        #expect(track.elsewhere == .album(302, track: 303))
    }

    @Test func aTitleThatIsntThereSaysSo() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 999_999)
        await model.load()
        #expect(model.state == .notFound)
        #expect(ItemModel.notFoundTitle == "This title isn't available")
    }

    @Test func watchedShowsAtOnceAndGoesBackWhenRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 101)
        await model.load()
        #expect(model.page?.watched == false)
        await model.toggleWatched()
        #expect(server.watched(101) == true)
        #expect(model.page?.watched == true)
        #expect(model.page?.primary.kind == .play)

        server.answer(path: "/api/items/101/unwatched", status: 500, json: #"{"code":"oops","message":"disk full"}"#)
        await model.toggleWatched()
        #expect(model.page?.watched == true)
        #expect(app.shell.toast?.text.hasPrefix("Couldn't mark it") == true)

        // An episode of a show's list.
        let show = ItemModel(app: app, itemId: 201)
        await show.load()
        let third = try #require(show.page?.seasons?.episodes?.last)
        #expect(!third.watched)
        await show.setWatched(third.item, true)
        #expect(server.watched(205) == true)
        #expect(show.page?.seasons?.episodes?.last?.watched == true)
    }

    @Test func myListShowsAtOnceAndGoesBackWhenRefused() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        #expect(await eventually { app.actions.listed != nil })
        let model = ItemModel(app: app, itemId: 101)
        await model.load()
        #expect(model.page?.myList == false)
        await model.toggleMyList()
        #expect(server.watchlistIds.contains(101))
        #expect(await eventually { model.page?.myList == true })
        server.answer(path: "/api/watchlist/101", status: 403, json: #"{"code":"forbidden","message":"no"}"#)
        await model.toggleMyList()
        #expect(await eventually { model.page?.myList == true })
        #expect(app.shell.toast?.text.hasPrefix("Couldn't change My List") == true)
    }

    @Test func aRatingIsSavedOrGoesBackWithWhy() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 101)
        await model.load()
        #expect(model.page?.userRating == 9)
        await model.rate(7)
        #expect(model.page?.userRating == 7)
        #expect(server.rating(101) == .some(7))
        await model.rate(0)
        #expect(model.page?.userRating == nil)
        #expect(server.rating(101) == .some(nil))

        server.refuseRating(status: 500)
        await model.rate(4)
        #expect(model.page?.userRating == nil)
        #expect(app.shell.toast?.text.hasPrefix("Couldn't save your rating") == true)
    }

    @Test func playIsReadAheadAtMostOnceInTwoMinutes() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        var now = Date(timeIntervalSince1970: 1_000_000)
        app.actions.now = { now }
        let model = ItemModel(app: app, itemId: 101)
        await model.load()
        model.prefetch()
        model.prefetch()
        #expect(await eventually { server.prefetches == [101] })
        now += 119
        model.prefetch()
        try await Task.sleep(for: .milliseconds(200))
        #expect(server.prefetches == [101])
        now += 2
        model.prefetch()
        #expect(await eventually { server.prefetches == [101, 101] })
    }

    @Test func automaticAudioForgetsThePick() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 203)
        await model.load()
        #expect(model.page?.tracks?.automaticAudio == true)
        await model.chooseAudio(2)
        #expect(model.page?.tracks?.automaticAudio == false)
        #expect(model.page?.tracks?.audioIndex == 2)
        await model.chooseAudio(nil)
        let bodies = server.streamsChoices.map { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        #expect(bodies.count == 2)
        #expect(bodies[0]?["audio_index"] as? Int == 2)
        // Automatic: `audio_index: null`.
        #expect(bodies[1]?.keys.contains("audio_index") == true)
        #expect(bodies[1]?["audio_index"] is NSNull)
    }

    @Test func thePageFollowsTheServersEvents() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let model = ItemModel(app: app, itemId: 201)
        await model.load()
        #expect(await eventually { server.openEventStreams == 1 })
        let before = server.requests("/api/items/201").count
        // One of its episodes.
        server.pushEvent(#"{"type":"user_state_changed","item_ids":[205],"children":false}"#)
        #expect(await eventually { server.requests("/api/items/201").count == before + 1 })
        // Something else.
        server.pushEvent(#"{"type":"user_state_changed","item_ids":[106],"children":false}"#)
        try await Task.sleep(for: .milliseconds(600))
        #expect(server.requests("/api/items/201").count == before + 1)
    }
}

/// People, artists and albums.
@MainActor
@Suite struct PersonAndMusicTests {
    @Test func aPersonsPageListsTheirTitlesWithTheirParts() throws {
        let person = try PleyJSON.decoder.decode(PersonDetail.self, from: Fixtures.data("people/detail.json"))
        let page = PersonPage(person)
        #expect(page.name == "Ben Stiller")
        #expect(page.photo == "/api/people/706/photo?v=e5f6a7b8c9")
        #expect(page.counts == "1 movie · 1 show")
        #expect(page.movies.map(\.subtitle) == ["2013 · as Walter Mitty, Director"])
        #expect(page.shows.map(\.subtitle) == ["2022 · Director (6 episodes), Producer"])
        #expect(page.roles.map(\.title) == ["Acting", "Directing", "Producing"])
        #expect(page.roles[1].tiles.map(\.subtitle) == ["2022 · Director (6 episodes)", "2013 · Director"])
    }

    @Test func aPersonIsReadOrNotFound() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let ben = PersonModel(app: app, personId: 706)
        await ben.load()
        #expect(ben.state == .loaded && ben.page?.name == "Ben Stiller")
        let nobody = PersonModel(app: app, personId: 1)
        await nobody.load()
        #expect(nobody.state == .notFound)
        #expect(PersonModel.notFoundTitle == "Person not found")
    }

    @Test func anArtistsAlbumsAndAnAlbumsTracks() async throws {
        let server = try await fixture()
        defer { server.stop() }
        let app = try await signedIn(server)
        let artist = ArtistModel(app: app, itemId: 301)
        await artist.load()
        let page = try #require(artist.page)
        #expect(page.title == "Nils Frahm")
        #expect(page.counts == "1 album · 3 tracks")
        #expect(page.albums.map(\.title) == ["All Melody"])
        #expect(page.albums.first?.shape == .square)
        #expect(Screen.page(of: try #require(page.albums.first).item) == .album(302))

        let album = AlbumModel(app: app, itemId: 302, track: 303)
        await album.load()
        let albumPage = try #require(album.page)
        #expect(albumPage.artist == "Nils Frahm" && albumPage.artistId == 301)
        #expect(albumPage.meta == "3 tracks · 16m")
        #expect(albumPage.highlight == 303)
        // Two discs: a heading above each.
        #expect(albumPage.rows.map(\.id) == ["disc-1", "track-304", "track-303", "disc-2", "track-305"])
        guard case .track(let guest) = albumPage.rows.last else {
            Issue.record("no track")
            return
        }
        #expect(guest.artist == "Guest Player" && guest.duration == "3:10" && guest.number == "1")
        guard case .track(let second) = albumPage.rows[2] else {
            Issue.record("no track")
            return
        }
        #expect(second.artist == nil)
        #expect(second.spoken == "Track 2, The Whole Universe Wants to Be Touched, 4 minutes")

        await album.rate(8)
        #expect(album.page?.userRating == 8)
        #expect(server.rating(302) == .some(8))
        server.refuseRating(status: 500)
        await album.rate(2)
        #expect(album.page?.userRating == 8)
    }
}
