package app.pley.android.ui.tv.shell

import androidx.activity.ComponentActivity
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.lifecycle.Lifecycle
import app.pley.android.common.Tier
import app.pley.android.data.RequestNotice
import app.pley.android.data.RequestsRepository
import app.pley.android.model.Library
import app.pley.android.model.TitleKind
import app.pley.android.screens.PleyKey
import app.pley.android.ui.tv.kit.KeyRepeatGate
import app.pley.android.ui.tv.kit.LocalKeyRepeatGate
import app.pley.android.ui.tv.kit.LocalTvOverlays
import app.pley.android.ui.tv.kit.TvOverlays
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvSidePanel
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.TvTextField
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.keyRepeatGate
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.theme.TvTheme
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.update
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * Requests' news over a page, as the Samsung and LG apps pop it up
 * (packages/tv's requests.test.tsx, "ready-to-watch news"), on the test's
 * clock: it waits for the remote to rest and for the page to be free,
 * takes focus and gives it back, ignores an OK meant for the page, stays
 * while newer news comes, and goes by itself.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvRequestNoticesTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val gate = KeyRepeatGate { compose.mainClock.currentTime }
    private val overlays = TvOverlays()
    private val queue = MutableStateFlow<List<TvNotice>>(emptyList())
    private val opened = mutableListOf<PleyKey>()
    private var free = true

    private val source = object : TvShellSource {
        override suspend fun startFacts() = StartFacts(null, signedIn = false)

        override val libraries: Flow<List<Library>> = flowOf(emptyList())

        override val profile: Flow<RailProfile?> = flowOf(null)

        override suspend fun keepUpdated() = Unit

        override val notices: Flow<List<TvNotice>> = queue

        override fun dismiss(notice: RequestNotice) = queue.update { it.filterNot { n -> n.notice.key == notice.key } }
    }

    /** News heard: the repository keeps the last three. */
    private fun hear(notice: TvNotice) = queue.update { (it + notice).takeLast(RequestsRepository.KEPT) }

    private fun ready(key: Long, title: String) = TvNotice(
        RequestNotice.Ready(key, requestId = key, title = title, kind = TitleKind.Movie, itemId = key + 100, seasons = null, complete = true),
    )

    /** [page], whose button has focus, and the notices over it, on a clock the test moves. */
    private fun show(page: @Composable () -> Unit = { TvPillButton("A page", onClick = {}, modifier = Modifier.initialFocus()) }) {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            val content = remember { FocusRequester() }
            val rail = remember { RailState(content) }
            TvTheme(Tier.T2, reducedMotion = true) {
                CompositionLocalProvider(LocalKeyRepeatGate provides gate, LocalTvOverlays provides overlays) {
                    Box(Modifier.fillMaxSize().keyRepeatGate(gate).focusRequester(content).focusGroup()) {
                        page()
                        TvRequestNotices(source, content, rail, free = { free && !overlays.any }, onOpen = { opened += it })
                    }
                }
            }
        }
        advance(100)
    }

    private fun advance(ms: Long) {
        compose.mainClock.advanceTimeBy(ms)
        compose.waitForIdle()
    }

    private fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        advance(100)
    }

    private fun card() = compose.onNodeWithTag(TV_REQUEST_NOTICE)

    @Test
    fun itWaitsForTheRemoteToRestTakesFocusAndGivesItBackOnBack() {
        show()
        compose.onNodeWithText("A page").assertIsFocused()
        compose.press(Key.DirectionDown)
        hear(ready(1, "Arrival"))
        advance(1_000)
        card().assertDoesNotExist()
        advance(600)
        compose.onNodeWithText("“Arrival” is ready to watch.").assertExists()
        compose.onNodeWithText("Open").assertIsFocused()
        // Focus stays in it.
        compose.press(Key.DirectionLeft)
        compose.onNodeWithText("Open").assertIsFocused()
        back()
        card().assertDoesNotExist()
        compose.onNodeWithText("A page").assertIsFocused()
        assertEquals(emptyList<TvNotice>(), queue.value)
    }

    @Test
    fun newsHeardBehindThePlayerWaitsForTheRemoteToRestOnceBack() {
        show()
        // The player's activity is in front.
        compose.activityRule.scenario.moveToState(Lifecycle.State.STARTED)
        hear(ready(1, "Arrival"))
        advance(5_000)
        card().assertDoesNotExist()
        // Back from it: the person's next press is meant for the page.
        compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
        advance(1_000)
        card().assertDoesNotExist()
        advance(600)
        card().assertExists()
        compose.onNodeWithText("Open").assertIsFocused()
    }

    @Test
    fun itWaitsWhileSomethingElseHoldsFocusAndAnOkInItsFirstMomentDoesNothing() {
        free = false
        show()
        hear(ready(1, "Arrival"))
        advance(5_000)
        card().assertDoesNotExist()
        free = true
        advance(300)
        card().assertExists()
        // Meant for what had focus before it came.
        compose.press(Key.Enter)
        assertEquals(emptyList<PleyKey>(), opened)
        advance(OK_GUARD_MS + 100)
        compose.press(Key.Enter)
        advance(100)
        assertEquals(listOf<PleyKey>(PleyKey.Item(101)), opened)
        card().assertDoesNotExist()
    }

    @Test
    fun itWaitsWhileAPanelIsOpen() {
        var panel by mutableStateOf(true)
        show {
            TvPillButton("A page", onClick = {}, modifier = Modifier.initialFocus())
            TvSidePanel(visible = panel, onDismiss = { panel = false }) { TvTextButton("In the panel", onClick = {}) }
        }
        hear(ready(1, "Arrival"))
        advance(3_000)
        card().assertDoesNotExist()
        panel = false
        advance(300)
        card().assertExists()
    }

    @Test
    fun itWaitsWhileAFieldIsTypedIn() {
        // A page that only asks for words: the keyboard is up as it shows.
        show { TvTextField(rememberTextFieldState(), "A name", typeNow = true) }
        hear(ready(1, "Arrival"))
        advance(3_000)
        card().assertDoesNotExist()
        // Back closes the keyboard, focus on the field.
        back()
        advance(300)
        card().assertExists()
    }

    @Test
    fun itStaysWhileNewerNewsPushesOlderOutOfTheQueue() {
        show()
        hear(ready(1, "T1"))
        advance(300)
        compose.onNodeWithText("“T1” is ready to watch.").assertExists()
        (2L..5L).forEach { hear(ready(it, "T$it")) }
        advance(100)
        compose.onNodeWithText("“T1” is ready to watch.").assertExists()
        compose.onNodeWithText("Open").assertIsFocused()
        assertEquals(listOf("T3", "T4", "T5"), queue.value.map { (it.notice as RequestNotice.Ready).title })
        back()
        compose.onNodeWithText("A page").assertIsFocused()
        // The next waits for the remote to rest from closing it.
        advance(1_000)
        card().assertDoesNotExist()
        advance(600)
        compose.onNodeWithText("“T3” is ready to watch.").assertExists()
    }

    @Test
    fun itGoesByItselfAfterTenSecondsUntouched() {
        show()
        hear(ready(1, "Arrival"))
        advance(300)
        card().assertExists()
        advance(NOTICE_SHOWN_MS - 1_000)
        // A key restarts its time.
        compose.press(Key.DirectionRight)
        advance(9_000)
        card().assertExists()
        advance(1_100)
        card().assertDoesNotExist()
        compose.onNodeWithText("A page").assertIsFocused()
    }

    @Test
    fun newsOfSeveralOpensTheRequests() {
        show()
        hear(TvNotice(RequestNotice.News(1, count = 3)))
        advance(300)
        compose.onNodeWithText("3 of your requests have news").assertExists()
        advance(OK_GUARD_MS)
        compose.press(Key.Enter)
        advance(100)
        assertEquals(listOf<PleyKey>(PleyKey.Requests), opened)
    }
}
