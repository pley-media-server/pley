@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.account

import app.pley.android.data.AppSettings
import app.pley.android.data.HistoryRepository
import app.pley.android.data.Loadable
import app.pley.android.data.Session
import app.pley.android.data.SessionRepository
import app.pley.android.data.SessionState
import app.pley.android.data.Settings
import app.pley.android.data.TrackDefaultsRepository
import app.pley.android.data.TrackDefaultsUnsupported
import app.pley.android.model.HistoryEntry
import app.pley.android.model.HistoryPage
import app.pley.android.model.HistoryUser
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Me
import app.pley.android.model.SubtitleMode
import app.pley.android.model.TrackDefaults
import app.pley.android.model.format.FormatWords
import app.pley.android.network.ApiException
import app.pley.android.screens.PleyKey
import app.pley.android.screens.music.FakeMusic
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class AccountViewModelsTest {
    private val dispatcher = UnconfinedTestDispatcher()

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    // ---------------------------------------------------------------- Me

    private val me = Me(
        id = 7,
        username = "anna",
        name = "Anna",
        managed = true,
        avatarUrl = "/api/users/7/avatar?v=3",
        avatarColor = "#38bdf8",
        audioLanguage = "eng",
        subtitleMode = SubtitleMode.Forced,
        scope = "library play playlists profiles",
    )

    private inner class Sessions(scopes: Set<String> = setOf("library", "play", "playlists", "profiles")) : SessionRepository {
        val flow = MutableStateFlow<Session?>(Session(ORIGIN, me, scopes))
        var signedOut = 0
        override val session: StateFlow<Session?> = flow
        override val state: StateFlow<SessionState> = MutableStateFlow(SessionState.Starting)
        override suspend fun signOut() {
            signedOut++
            flow.value = null
        }
    }

    /** Track defaults the server keeps, or refuses to change ([refuse]). */
    private class Defaults : TrackDefaultsRepository {
        val flow = MutableStateFlow<Loadable<TrackDefaults>>(Loadable.Ready(TrackDefaults("eng", null, SubtitleMode.Forced), fresh = true))
        val saved = mutableListOf<TrackDefaults>()
        var refuse: Exception? = null
        override fun defaults(): Flow<Loadable<TrackDefaults>> = flow
        override suspend fun save(defaults: TrackDefaults) {
            refuse?.let { throw it }
            saved += defaults
            flow.value = Loadable.Ready(defaults, fresh = true)
        }
    }

    private fun TestScope.meModel(sessions: Sessions = Sessions(), defaults: Defaults = Defaults(), music: FakeMusic? = null) =
        MeViewModel(sessions, defaults, null, music).also { vm -> backgroundScope.launch { vm.state.collect { } } }

    @Test
    fun meShowsTheProfileAsAManagedProfileOrAUserNeverAnAdministrator() = runTest(dispatcher) {
        val vm = meModel()
        val profile = vm.state.value.profile!!
        assertEquals("Anna", profile.name)
        assertTrue(profile.managed)
        assertTrue(profile.canSwitch)
        assertEquals("$ORIGIN/api/users/7/avatar?v=3", profile.avatarUrl)

        val noSwitch = meModel(Sessions(setOf("library", "play"))).state.value.profile!!
        assertFalse(noSwitch.canSwitch)
    }

    @Test
    fun trackDefaultsAreSavedAndToldOnce() = runTest(dispatcher) {
        val defaults = Defaults()
        val vm = meModel(defaults = defaults)
        val events = mutableListOf<MeEvent>()
        backgroundScope.launch { vm.events.collect { events += it } }

        vm.setSubtitleMode(SubtitleMode.Foreign)
        vm.setSubtitleLanguage("fre")
        vm.setAudioLanguage(null)
        assertEquals(
            listOf(
                TrackDefaults("eng", null, SubtitleMode.Foreign),
                TrackDefaults("eng", "fre", SubtitleMode.Foreign),
                TrackDefaults(null, "fre", SubtitleMode.Foreign),
            ),
            defaults.saved,
        )
        assertEquals(
            listOf(MeEvent.Saved(TrackField.SubtitleMode), MeEvent.Saved(TrackField.SubtitleLanguage), MeEvent.Saved(TrackField.AudioLanguage)),
            events,
        )
        assertTrue(vm.state.value.trackDefaults!!.readsSubtitles)
        // The same choice again changes nothing.
        vm.setAudioLanguage(null)
        assertEquals(3, defaults.saved.size)
    }

    @Test
    fun aServerThatRefusesShowsTheDefaultsReadOnly() = runTest(dispatcher) {
        val defaults = Defaults().apply { refuse = TrackDefaultsUnsupported(ApiException(404, "not_found", "Not found")) }
        val vm = meModel(defaults = defaults)
        val events = mutableListOf<MeEvent>()
        backgroundScope.launch { vm.events.collect { events += it } }
        assertTrue(vm.state.value.trackDefaults!!.editable)

        vm.setSubtitleMode(SubtitleMode.Always)
        assertEquals(listOf<MeEvent>(MeEvent.ReadOnly), events)
        val shown = vm.state.value.trackDefaults!!
        assertFalse(shown.editable)
        assertEquals(SubtitleMode.Forced, shown.defaults.subtitleMode)

        // Read-only: nothing more is sent.
        vm.setAudioLanguage("ger")
        assertEquals(1, events.size)
    }

    @Test
    fun withoutThePlayScopeTheDefaultsAreReadOnlyFromTheStart() = runTest(dispatcher) {
        val vm = meModel(Sessions(setOf("library")))
        assertFalse(vm.state.value.trackDefaults!!.editable)
    }

    @Test
    fun otherFailuresAreToldAndChangeNothing() = runTest(dispatcher) {
        val defaults = Defaults().apply { refuse = IOException("refused") }
        val vm = meModel(defaults = defaults)
        val events = mutableListOf<MeEvent>()
        backgroundScope.launch { vm.events.collect { events += it } }
        vm.setAudioLanguage("ger")
        assertTrue(events.single() is MeEvent.Failed)
        assertTrue(vm.state.value.trackDefaults!!.editable)
        assertFalse(vm.state.value.trackDefaults!!.saving)
    }

    @Test
    fun signingOutStopsTheMusicThenSaysWhichServer() = runTest(dispatcher) {
        val sessions = Sessions()
        val music = FakeMusic()
        val vm = meModel(sessions, music = music)
        val events = mutableListOf<MeEvent>()
        backgroundScope.launch { vm.events.collect { events += it } }
        vm.signOut()
        assertEquals(1, music.stops)
        assertEquals(1, sessions.signedOut)
        assertEquals(listOf<MeEvent>(MeEvent.SignedOut(ORIGIN)), events)
        assertNull(vm.state.value.profile)
    }

    @Test
    fun theLanguagesAreTheWebs34ByName() {
        val all = TrackLanguages.all(FormatWords.English)
        assertEquals(34, all.size)
        assertEquals("Arabic", all.first().second)
        assertEquals(all.map { it.second }.sorted(), all.map { it.second })
        assertEquals("French", TrackLanguages.nameOf("fre", FormatWords.English))
        assertNull(TrackLanguages.nameOf(null, FormatWords.English))
    }

    // ---------------------------------------------------------------- History

    /** The repository's paging and optimistic removal, as CachedHistory does them. */
    private class History(total: Int) : HistoryRepository {
        private val all = (1..total.toLong()).map(::entry)
        val flow = MutableStateFlow<Loadable<HistoryPage>>(Loadable.Ready(HistoryPage(total, all.take(15)), fresh = true))
        var refuseRemoval: Exception? = null
        override fun history(): Flow<Loadable<HistoryPage>> = flow
        override suspend fun loadMore() {
            val shown = (flow.value as Loadable.Ready).value
            flow.value = Loadable.Ready(shown.copy(entries = shown.entries + all.drop(shown.entries.size).take(15)), fresh = true)
        }
        override suspend fun remove(entryId: Long) {
            val before = (flow.value as Loadable.Ready).value
            flow.value = Loadable.Ready(before.copy(total = before.total - 1, entries = before.entries.filterNot { it.id == entryId }), fresh = true)
            refuseRemoval?.let {
                flow.value = Loadable.Ready(before, fresh = true)
                throw it
            }
        }
    }

    @Test
    fun historyPagesBy15AndRemovesAtOnce() = runTest(dispatcher) {
        val history = History(total = 40)
        val vm = HistoryViewModel(history, flowOf(ORIGIN), nowSeconds = { NOW }, compute = dispatcher)
        backgroundScope.launch { vm.state.collect { } }
        val first = vm.state.value as HistoryUiState.Ready
        assertEquals(15, first.rows.size)
        assertTrue(first.canLoadMore)

        vm.loadMore()
        vm.loadMore()
        val all = vm.state.value as HistoryUiState.Ready
        assertEquals(40, all.rows.size)
        assertFalse(all.canLoadMore)

        vm.remove(3)
        val after = vm.state.value as HistoryUiState.Ready
        assertEquals(39, after.total)
        assertTrue(after.rows.none { it.entryId == 3L })
    }

    @Test
    fun aRefusedRemovalComesBackAndIsTold() = runTest(dispatcher) {
        val history = History(total = 5).apply { refuseRemoval = IOException("offline") }
        val vm = HistoryViewModel(history, flowOf(ORIGIN), nowSeconds = { NOW }, compute = dispatcher)
        backgroundScope.launch { vm.state.collect { } }
        val errors = mutableListOf<Throwable>()
        backgroundScope.launch { vm.errors.collect { errors += it } }
        vm.remove(2)
        assertEquals(5, (vm.state.value as HistoryUiState.Ready).rows.size)
        assertEquals(1, errors.size)
    }

    @Test
    fun historyRowsSayWhatWhenAndWhere() {
        val episode = HistoryEntry(
            id = 1,
            viewedAt = NOW - 3 * 3600,
            device = "Chrome on Windows",
            user = HistoryUser(7, "Anna"),
            item = ItemSummary(205, 2, ItemKind.Episode, "Glasshouse", index = 5, parentIndex = 2, parentId = 202, grandparentId = 201, grandparentTitle = "Severance"),
        )
        val row = historyRow(episode, NOW)
        assertEquals("Severance", row.title)
        assertEquals("S2 · E5 · Glasshouse", row.subtitle)
        assertEquals("3 hours ago", row.time)
        assertEquals("Chrome on Windows", row.device)
        assertEquals(PleyKey.Item(205), row.opens)
        assertEquals(listOf("/api/items/202/images/poster", "/api/items/201/images/poster"), row.artPaths)

        val song = historyRow(episode.copy(item = ItemSummary(9, 3, ItemKind.Track, "Swell", parentId = 301, parentTitle = "Tides", grandparentTitle = "Harbour Lights")), NOW)
        assertEquals("Harbour Lights · Tides", song.subtitle)
        assertEquals(PleyKey.Album(301, trackId = 9), song.opens)
        assertTrue(song.square)
    }

    // ---------------------------------------------------------------- Settings

    /** Settings kept in memory as the DataStore keeps them: changes in, the new value out. */
    private class MemorySettings : AppSettings {
        val flow = MutableStateFlow(Settings())
        override val settings: Flow<Settings> = flow
        override suspend fun update(change: (Settings) -> Settings) {
            flow.value = change(flow.value)
        }
    }

    @Test
    fun settingsAreSavedAsTheyChange() = runTest(dispatcher) {
        val store = MemorySettings()
        val vm = app.pley.android.screens.settings.SettingsViewModel(store, tv = true)
        backgroundScope.launch { vm.state.collect { } }
        assertEquals(app.pley.android.data.FrameRateMatching.Auto, vm.state.value.frameRate)

        vm.setFrameRate(app.pley.android.data.FrameRateMatching.Off)
        vm.setTunneling(true)
        vm.setPassthrough("truehd", app.pley.android.data.Passthrough.Off)
        vm.setQuality(mobile = false, app.pley.android.screens.settings.SettingsViewModel.presetOf("8m"))
        vm.setQuality(mobile = true, app.pley.android.screens.settings.SettingsViewModel.presetOf("original"))
        vm.setPrivateTab(false)
        vm.setAlwaysStartAs(7)

        val s = store.flow.value
        assertEquals(app.pley.android.data.FrameRateMatching.Off, s.frameRateMatching)
        assertTrue(s.tunneling)
        assertEquals(app.pley.android.data.Passthrough.Off, s.passthrough("truehd"))
        assertEquals("8m", s.qualityOnHomeNetwork)
        assertEquals(Settings.ORIGINAL, s.qualityOnMobile)
        assertFalse(s.privateSignInTab)
        assertEquals(7L, s.alwaysStartAs)
        assertEquals(app.pley.android.data.FrameRateMatching.Off, vm.state.value.frameRate)
    }

    @Test
    fun aPhoneOffersSeamlessOrOff() = runTest(dispatcher) {
        val vm = app.pley.android.screens.settings.SettingsViewModel(MemorySettings(), tv = false)
        backgroundScope.launch { vm.state.collect { } }
        assertEquals(listOf(app.pley.android.data.FrameRateMatching.Seamless, app.pley.android.data.FrameRateMatching.Off), vm.state.value.frameRateChoices)
        assertEquals(app.pley.android.data.FrameRateMatching.Seamless, vm.state.value.frameRate)
    }

    private companion object {
        const val ORIGIN = "http://192.168.1.20:7539"
        const val NOW = 1_760_000_000L

        fun entry(id: Long) = HistoryEntry(id, NOW - id * 60, "pleyAndroid", HistoryUser(7, "Anna"), ItemSummary(100 + id, 1, ItemKind.Movie, "Film $id", year = 2020))
    }
}
