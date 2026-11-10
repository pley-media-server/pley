package app.pley.android.screens.item

import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.Me
import app.pley.android.model.PersonCredit
import app.pley.android.model.PersonDetail
import app.pley.android.model.PersonRole
import app.pley.android.model.PleyJson
import app.pley.android.network.ApiException
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.person.PersonUiState
import app.pley.android.screens.person.PersonViewModel
import app.pley.android.screens.person.roleWords
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ItemViewModelTest {
    private val dispatcher = UnconfinedTestDispatcher()
    private val data = FakeData()
    private val launcher = RecordingLauncher()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun TestScope.model(id: Long): ItemViewModel {
        val model = ItemViewModel(data, launcher, id, compute = dispatcher)
        backgroundScope.launch(dispatcher) { model.state.collect { } }
        return model
    }

    private val ItemViewModel.page: ItemPage get() = (state.value as ItemUiState.Ready).page

    @Test
    fun `nothing known shows the skeleton, then the page`() = runTest(dispatcher) {
        val model = model(1)
        assertEquals(ItemUiState.Loading, model.state.value)
        data.set(movie())
        assertEquals("Saltwind", model.page.title)
    }

    @Test
    fun `a 404 is not found, another failure says why`() = runTest(dispatcher) {
        val gone = model(1)
        data.detail(1).value = Loadable.Failed(ApiException(404, "not_found", "no such item"))
        assertEquals(ItemUiState.NotFound, gone.state.value)
        val broken = model(2)
        val error = ApiException(500, null, "the server failed")
        data.detail(2).value = Loadable.Failed(error)
        assertEquals(ItemUiState.Failed(error), broken.state.value)
    }

    @Test
    fun `music and extras open their own pages`() = runTest(dispatcher) {
        val cases = mapOf(
            ItemDetail(id = 60, libraryId = 3, kind = ItemKind.Artist, title = "A") to PleyKey.Artist(60),
            ItemDetail(id = 61, libraryId = 3, kind = ItemKind.Album, title = "B") to PleyKey.Album(61),
            ItemDetail(id = 62, libraryId = 3, kind = ItemKind.Track, title = "C", parentId = 61) to PleyKey.Album(61),
            ItemDetail(id = 63, libraryId = 1, kind = ItemKind.Extra, title = "Trailer", parentId = 1) to PleyKey.Item(1),
        )
        cases.forEach { (detail, key) ->
            val model = model(detail.id)
            data.set(detail)
            assertEquals(ItemUiState.Elsewhere(key), model.state.value)
        }
    }

    @Test
    fun `Resume plays from where it was left, with the chosen tracks`() = runTest(dispatcher) {
        val model = model(1)
        data.set(movie(positionMs = 60_000))
        model.chooseAudio(2)
        model.chooseSubtitle(3)
        model.play()
        assertEquals(PlayRequest(itemId = 1, startMs = 60_000, fromStart = false, audioIndex = 2, subtitleIndex = 3), launcher.played.single())
        model.play(fromStart = true)
        assertEquals(0L, launcher.played.last().startMs)
        assertTrue(launcher.played.last().fromStart)
    }

    @Test
    fun `choosing tracks saves them for the item, as the web's streams body`() = runTest(dispatcher) {
        val model = model(1)
        data.set(movie())
        model.chooseAudio(2)
        assertEquals(FakeData.StreamWrite(1, 11, 2, null), data.streams.last())
        model.chooseSubtitle(3)
        assertEquals(FakeData.StreamWrite(1, 11, 2, 3), data.streams.last())
        model.chooseSubtitle(null)
        assertEquals(FakeData.StreamWrite(1, 11, 2, null), data.streams.last())
        // The page shows the picks at once.
        assertEquals(2, model.page.tracks?.audioIndex)
    }

    @Test
    fun `the audio goes to the player and the saved choice only when picked here`() = runTest(dispatcher) {
        val model = model(1)
        val streams = arrayOf(
            stream(0, app.pley.android.model.StreamKind.Video, "hevc"),
            stream(1, app.pley.android.model.StreamKind.Audio, "truehd", "eng", default = true, channels = 8),
            stream(2, app.pley.android.model.StreamKind.Audio, "ac3", "fra", channels = 6),
            stream(3, app.pley.android.model.StreamKind.Subtitle, "subrip", "eng"),
        )
        // The details' audio is the profile's, chosen for no device: the server picks for this one.
        data.set(movie(files = listOf(file(11, *streams, selectedAudio = 2))))
        assertEquals(true, model.page.tracks?.automaticAudio)
        assertEquals("French", model.page.tracks?.automaticLanguage)
        model.play()
        assertEquals(null, launcher.played.single().audioIndex)
        // Subtitles alone: the audio chosen before stays.
        model.chooseSubtitle(3)
        assertEquals(FakeData.StreamWrite(1, 11, null, 3), data.streams.last())
        model.play()
        assertEquals(3L, launcher.played.last().subtitleIndex)
        assertEquals(null, launcher.played.last().audioIndex)
    }

    @Test
    fun `Automatic forgets the audio picked, and the player gets none`() = runTest(dispatcher) {
        val model = model(1)
        data.set(movie())
        model.chooseAudio(2)
        model.chooseAudio(null)
        assertEquals(FakeData.StreamWrite(1, 11, null, null, automaticAudio = true), data.streams.last())
        assertEquals(true, model.page.tracks?.automaticAudio)
        model.play()
        assertEquals(null, launcher.played.last().audioIndex)
    }

    @Test
    fun `the streams body is the server's wire format`() {
        val body = PleyJson.encodeToString(app.pley.android.model.StreamChoice.serializer(), app.pley.android.model.StreamChoice(11, 2, null))
        assertTrue(body, body.contains("\"file_id\":11") && body.contains("\"audio_index\":2"))
    }

    @Test
    fun `a version is played but not saved`() = runTest(dispatcher) {
        val model = model(1)
        data.set(movie(files = listOf(file(11, stream(1, app.pley.android.model.StreamKind.Audio, "aac", default = true)), file(12, stream(1, app.pley.android.model.StreamKind.Audio, "aac", default = true)))))
        model.chooseVersion(12)
        assertTrue(data.streams.isEmpty())
        model.play()
        assertEquals(12L, launcher.played.single().fileId)
    }

    @Test
    fun `a show plays its next episode, picked on its season`() = runTest(dispatcher) {
        val model = model(10)
        data.set(ItemDetail(id = 10, libraryId = 2, kind = ItemKind.Show, title = "Driftline", leafCount = 6, childCount = 2))
        data.setChildren(10, listOf(summary(30, ItemKind.Season, index = 1), summary(31, ItemKind.Season, index = 2)))
        val next = summary(41, ItemKind.Episode, parentId = 31, parentIndex = 2, index = 1, positionMs = 5_000)
        data.setOnDeck(10, next)
        data.setChildren(31, listOf(next, summary(42, ItemKind.Episode, parentId = 31, parentIndex = 2, index = 2)))
        val page = model.page
        assertEquals(PrimaryAction(PrimaryAction.Kind.Resume, "S2 · E1", 5_000f / 3_480_000f, "58 min left"), page.primary)
        assertEquals(31L, page.seasons?.selected)
        assertEquals(41L, page.seasons?.focus)
        assertEquals(listOf(41L, 42L), page.seasons?.episodes?.map { it.item.id })
        model.play()
        assertEquals(PlayRequest(itemId = 41, startMs = 5_000, fromStart = false), launcher.played.single())
        model.selectSeason(30)
        assertEquals(30L, model.page.seasons?.selected)
    }

    @Test
    fun `the seasons beside the one shown have their episodes asked for ahead`() = runTest(dispatcher) {
        val model = model(10)
        data.set(ItemDetail(id = 10, libraryId = 2, kind = ItemKind.Show, title = "Driftline", leafCount = 12, childCount = 6))
        data.setChildren(10, (30L..35L).map { summary(it, ItemKind.Season, index = (it - 29).toInt()) })
        // Not before the season shown is known.
        assertTrue(data.warmed.isEmpty())
        data.setOnDeck(10, null)
        assertEquals(30L, model.page.seasons?.selected)
        assertEquals(listOf(31L, 32L), data.warmed)
        data.warmed.clear()
        model.selectSeason(33)
        // The nearest first, the next ones before the ones passed.
        assertEquals(listOf(34L, 32L, 35L, 31L), data.warmed)
    }

    @Test
    fun `a show with more than twelve seasons lists the newest first`() = runTest(dispatcher) {
        val model = model(10)
        data.set(ItemDetail(id = 10, libraryId = 2, kind = ItemKind.Show, title = "Driftline", leafCount = 28, childCount = 13))
        // Specials, then seasons 1 to 13, as the server lists them.
        data.setChildren(10, (30L..43L).map { summary(it, ItemKind.Season, index = (it - 30).toInt()) })
        val next = summary(60, ItemKind.Episode, parentId = 42, parentIndex = 12, index = 1)
        data.setOnDeck(10, next)
        val seasons = model.page.seasons!!
        assertEquals((43L downTo 31L).toList() + 30L, seasons.seasons.map { it.id })
        assertEquals((13 downTo 1).toList() + 0, seasons.seasons.map { it.item?.index })
        // It opens on the next episode's season, and asks ahead for the seasons beside it as they are listed.
        assertEquals(42L, seasons.selected)
        assertEquals(listOf(41L, 43L, 40L), data.warmed)
        // A season's own page lists its show's seasons the same way.
        val season = model(42)
        data.set(ItemDetail(id = 42, libraryId = 2, kind = ItemKind.Season, title = "Season 12", index = 12, parentId = 10))
        assertEquals(seasons.seasons.map { it.id }, season.page.seasons?.seasons?.map { it.id })
    }

    @Test
    fun `a show with one season asks for nothing ahead`() = runTest(dispatcher) {
        val model = model(10)
        data.set(ItemDetail(id = 10, libraryId = 2, kind = ItemKind.Show, title = "Driftline", leafCount = 2, childCount = 1))
        data.setChildren(10, listOf(summary(30, ItemKind.Season, index = 1)))
        data.setOnDeck(10, null)
        assertEquals(30L, model.page.seasons?.selected)
        assertTrue(data.warmed.isEmpty())
    }

    @Test
    fun `nothing to play is told, not played`() = runTest(dispatcher) {
        val model = model(10)
        data.set(ItemDetail(id = 10, libraryId = 2, kind = ItemKind.Show, title = "Empty", leafCount = 0))
        data.setOnDeck(10, null)
        val events = mutableListOf<ItemEvent>()
        backgroundScope.launch(dispatcher) { model.events.collect { events += it } }
        model.play()
        assertEquals(listOf(ItemEvent.NothingToPlay), events)
        assertTrue(launcher.played.isEmpty())
    }

    @Test
    fun `watched toggles and ratings go to the server`() = runTest(dispatcher) {
        val model = model(1)
        data.set(movie())
        model.toggleWatched()
        assertEquals(1L to true, data.watched.single())
        model.rate(7)
        model.rate(null)
        assertEquals(listOf(1L to 7.0, 1L to null), data.ratings)
    }

    @Test
    fun `actions are never an administrator's, whatever the profile is`() {
        // An answer saying the profile is an administrator changes nothing: the
        // actions don't depend on who is signed in.
        val admin = Me(id = 1, username = "admin", name = "Admin", isAdmin = true)
        assertTrue(admin.isAdmin)
        val kinds = listOf(ItemKind.Movie, ItemKind.Show, ItemKind.Season, ItemKind.Episode, ItemKind.Extra, ItemKind.Album, ItemKind.Artist, ItemKind.Track)
        val allowed = setOf(
            ItemAction.Play, ItemAction.Resume, ItemAction.FromStart, ItemAction.MarkWatched, ItemAction.MarkUnwatched,
            ItemAction.AddToPlaylist, ItemAction.GoToShow, ItemAction.GoToSeason, ItemAction.Open,
            ItemAction.RemoveFromContinueWatching, ItemAction.AddToMyList, ItemAction.RemoveFromMyList,
        )
        kinds.forEach { kind -> assertTrue(allowed.containsAll(itemActions(summary(1, kind, parentId = 2, grandparentId = 3)))) }
        kinds.forEach { kind -> assertTrue(allowed.containsAll(itemActions(summary(1, kind), inWatchlist = true, continueWatching = true))) }
        assertEquals(allowed, ItemAction.entries.toSet())
    }

    @Test
    fun `an episode in progress offers Resume, From start, Watched, Playlist, its show and season`() {
        val episode = summary(41, ItemKind.Episode, parentId = 31, grandparentId = 10, positionMs = 5_000)
        assertEquals(
            listOf(ItemAction.Resume, ItemAction.FromStart, ItemAction.MarkWatched, ItemAction.AddToPlaylist, ItemAction.GoToShow, ItemAction.GoToSeason),
            itemActions(episode),
        )
        assertEquals(PleyKey.Item(10), destination(ItemAction.GoToShow, episode))
        assertEquals(PleyKey.Item(31), destination(ItemAction.GoToSeason, episode))
        assertEquals(PleyKey.AddToPlaylist(listOf(41)), destination(ItemAction.AddToPlaylist, episode))
        assertEquals(listOf(ItemAction.Open, ItemAction.AddToPlaylist), itemActions(summary(61, ItemKind.Album)))
    }

    @Test
    fun `the action sheet plays and marks, and its work outlives it`() = runTest(dispatcher) {
        val model = ItemActionsViewModel(data, launcher, 1, compute = dispatcher, work = CoroutineScope(dispatcher))
        backgroundScope.launch(dispatcher) { model.state.collect { } }
        data.set(movie(positionMs = 60_000))
        val ready = model.state.value as ItemActionsUiState.Ready
        assertEquals(ItemAction.Resume, ready.actions.first())
        assertTrue(model.perform(ItemAction.FromStart))
        assertEquals(0L, launcher.played.single().startMs)
        assertTrue(model.perform(ItemAction.MarkWatched))
        assertEquals(1L to true, data.watched.single())
        assertFalse(model.perform(ItemAction.AddToPlaylist))
    }

    @Test
    fun `a person's page splits movies and shows, with their roles`() = runTest(dispatcher) {
        val model = PersonViewModel(data, 7, compute = dispatcher)
        backgroundScope.launch(dispatcher) { model.state.collect { } }
        assertEquals(PersonUiState.Loading, model.state.value)
        val person = PersonDetail(
            id = 7,
            name = "Lena Okafor",
            credits = listOf(
                PersonCredit(summary(1, ItemKind.Movie).copy(year = 2021), listOf(PersonRole(CreditRole.Actor, "Mara"))),
                PersonCredit(summary(10, ItemKind.Show).copy(year = 2019), listOf(PersonRole(CreditRole.Director, episodes = 3))),
            ),
        )
        data.setPerson(7, Loadable.Ready(person, fresh = true))
        val page = (model.state.value as PersonUiState.Ready).page
        assertEquals(listOf(UiText.plural(R.plurals.person_count_movies, 1), UiText.plural(R.plurals.person_count_shows, 1)), page.countTexts)
        assertEquals(listOf(UiText.res(R.string.person_role_as, "Mara")), page.movies.single().roles)
        assertEquals(2021, page.movies.single().item.year)
        assertEquals(listOf(UiText.plural(R.plurals.person_role_episodes, 3, UiText.res(R.string.person_role_director), 3)), page.shows.single().roles)
        assertEquals(listOf(CreditRole.Actor, CreditRole.Director), page.roles.map { it.role })
    }

    @Test
    fun `a person who isn't there is not found`() = runTest(dispatcher) {
        val model = PersonViewModel(data, 8, compute = dispatcher)
        backgroundScope.launch(dispatcher) { model.state.collect { } }
        data.setPerson(8,
            Loadable.Failed(ApiException(404, "not_found", "no such person")))
        assertEquals(PersonUiState.NotFound, model.state.value)
    }

    @Test
    fun `roles read as the web's`() {
        assertEquals(listOf(UiText.res(R.string.person_role_as, "Neo")), roleWords(listOf(PersonRole(CreditRole.Actor, "Neo"))))
        assertEquals(
            listOf(UiText.plural(R.plurals.person_role_episodes, 1, UiText.res(R.string.person_role_guest_star), 1)),
            roleWords(listOf(PersonRole(CreditRole.Actor, episodes = 1))),
        )
        assertEquals(
            listOf(UiText.res(R.string.person_role_actor), UiText.res(R.string.person_role_writer)),
            roleWords(listOf(PersonRole(CreditRole.Actor), PersonRole(CreditRole.Writer))),
        )
    }
}
