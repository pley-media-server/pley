package app.pley.android.ui.phone.item

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.hasText
import app.pley.android.common.TestTags
import app.pley.android.model.ItemDetail
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemViewModel
import app.pley.android.screens.item.ItemUiState
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import app.pley.android.ui.phone.theme.PhoneTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.onNodeWithContentDescription
import app.pley.android.data.OtherDevice
import app.pley.android.model.CastKind
import app.pley.android.model.DeviceKind
import app.pley.android.model.RemoteDevice
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.remote.LocalPlayOn
import app.pley.android.ui.phone.remote.PLAY_ON
import app.pley.android.ui.phone.remote.PlayOnHost
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The phone's item page: Play, Resume and From start, Unavailable, the
 * summary clamped to four lines with More, and where its links go.
 */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w412dp-h915dp-xxhdpi")
// Real text measuring, so the summary has lines to clamp.
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ItemPageTest {
    @get:Rule
    val compose = createComposeRule()

    private val opened = mutableListOf<PleyKey>()
    private val navigator = object : Navigator {
        override fun open(key: PleyKey) {
            opened += key
        }

        override fun back() = Unit

        override fun restart(key: PleyKey) = Unit
    }

    /** Play on…'s chooser asked for, by what. */
    private val castsAsked = mutableListOf<CastTarget>()

    private fun show(detail: ItemDetail, devices: List<OtherDevice> = emptyList()): ItemData {
        val data = ItemData(detail)
        val model = ItemViewModel(data, data.launcher, detail.id, compute = Dispatchers.Main.immediate)
        val host = object : PlayOnHost {
            override val devices: List<OtherDevice> = devices

            override fun playOn(target: CastTarget) {
                castsAsked += target
            }
        }
        compose.setContent {
            PhoneTheme {
                CompositionLocalProvider(LocalPlayOn provides host) {
                    val state by model.state.collectAsState()
                    (state as? ItemUiState.Ready)?.let { ItemPageContent(it.page, model, navigator) }
                }
            }
        }
        compose.waitForIdle()
        return data
    }

    @Test
    fun withoutAnotherDeviceTakingTitlesThereIsNoPlayOn() {
        show(movie(positionMs = 3_480_000))
        compose.onNodeWithTag(PLAY_ON).assertDoesNotExist()
    }

    @Test
    fun playOnBesideResumeSendsTheTitleFromWhereItResumesWithTheTracksPickedHere() {
        val tv = OtherDevice("app-12", RemoteDevice("tv-1", "app-12", "Living room TV", DeviceKind.Tv, listOf(CastKind.Item)))
        show(movie(positionMs = 3_480_000), listOf(tv))
        compose.onNodeWithContentDescription("Play on…").assertExists().performClick()
        // Automatic audio, and the file's subtitles off.
        assertEquals(listOf(CastTarget(itemId = 1, startMs = 3_480_000, subtitleIndex = -1)), castsAsked)
    }

    @Test
    fun aMovieNotStartedPlays() {
        show(movie())
        compose.onNodeWithText("Play").assertExists()
        compose.onNodeWithText("From start").assertDoesNotExist()
    }

    @Test
    fun aMovieInProgressResumesWithWhatIsLeftAndFromStart() {
        val data = show(movie(positionMs = 3_480_000))
        compose.onNodeWithText("Resume").assertExists()
        compose.onNodeWithText("1h 33m left").assertExists()
        compose.onNodeWithText("From start").performClick()
        compose.waitForIdle()
        assertEquals(0L, data.played.single().startMs)
    }

    @Test
    @Config(qualifiers = "fr-w412dp-h915dp-xxhdpi")
    fun thePageIsInFrench() {
        show(movie(positionMs = 3_480_000))
        compose.onNodeWithText("Reprendre").assertExists()
        compose.onNodeWithText("Depuis le début").assertExists()
        compose.onNodeWithText("Sous-titres").assertExists()
        compose.onNodeWithTag(ITEM_PAGE).performScrollToNode(hasText("Informations sur le média"))
        compose.onNodeWithText("Informations sur le média").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(ITEM_PAGE).performScrollToNode(hasText("Flux"))
        compose.onNodeWithText("Texte").assertExists()
    }

    @Test
    @Config(qualifiers = "es-w412dp-h915dp-xxhdpi")
    fun thePageIsInSpanish() {
        show(movie(positionMs = 3_480_000))
        compose.onNodeWithText("Reanudar").assertExists()
        compose.onNodeWithText("Desde el principio").assertExists()
        compose.onNodeWithText("Lista de reproducción").assertExists()
        compose.onNodeWithTag(ITEM_PAGE).performScrollToNode(hasText("Información multimedia"))
        compose.onNodeWithText("Información multimedia").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(ITEM_PAGE).performScrollToNode(hasText("Pistas"))
        compose.onNodeWithText("Texto").assertExists()
    }

    @Test
    fun noFilesIsUnavailable() {
        show(movie().copy(files = emptyList()))
        compose.onNodeWithText("Unavailable").assertExists()
        compose.onNodeWithTag(TestTags.ITEM_PLAY).assertDoesNotExist()
    }

    @Test
    fun aLongSummaryIsClampedWithMore() {
        val long = List(30) { "The caravan crosses the salt flats under a white sky." }.joinToString(" ")
        show(movie().copy(summary = long))
        compose.onNodeWithText("More").assertExists().performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Less").assertExists()
    }

    @Test
    fun aShortSummaryHasNoMore() {
        show(movie())
        compose.onNodeWithText("More").assertDoesNotExist()
    }

    @Test
    fun thePlaylistActionOpensAddToPlaylist() {
        show(movie())
        compose.onNodeWithText("Playlist").performClick()
        assertEquals(listOf(PleyKey.AddToPlaylist(listOf(1L))), opened)
    }

    @Test
    fun mediaInfoIsCollapsedAndShowsTheFileName() {
        show(movie())
        compose.onNodeWithTag(ITEM_PAGE).performScrollToNode(hasText("Media info"))
        compose.onNodeWithText("Saltwind.mkv").assertDoesNotExist()
        compose.onNodeWithText("Media info").performClick()
        compose.waitForIdle()
        compose.onNodeWithTag(ITEM_PAGE).performScrollToNode(hasText("Saltwind.mkv"))
        compose.onNodeWithText("Saltwind.mkv").assertExists()
    }
}
