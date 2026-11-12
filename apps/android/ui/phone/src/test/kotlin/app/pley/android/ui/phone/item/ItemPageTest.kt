package app.pley.android.ui.phone.item

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import app.pley.android.common.TestTags
import app.pley.android.model.ItemDetail
import app.pley.android.screens.Navigator
import app.pley.android.screens.PleyKey
import app.pley.android.screens.item.ItemViewModel
import app.pley.android.screens.item.ItemUiState
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.getValue
import app.pley.android.ui.phone.theme.PhoneTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.onNodeWithContentDescription
import app.pley.android.data.DownloadEntry
import app.pley.android.data.DownloadState
import app.pley.android.data.OtherDevice
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.ExtraType
import app.pley.android.model.CastKind
import app.pley.android.model.DeviceKind
import app.pley.android.model.RemoteDevice
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.remote.LocalPlayOn
import app.pley.android.ui.phone.remote.PLAY_ON
import app.pley.android.ui.phone.remote.PlayOnHost
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The phone's item page: Play, Resume and From start, Unavailable,
 * Download beside Playlist, the summary clamped to four lines with More, and
 * where its links go.
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

    /** The downloads the page shows, by item, and what was asked of them. */
    private val downloadsByItem = mutableStateOf(emptyMap<Long, DownloadEntry>())
    private val downloadsAsked = mutableListOf<List<Long>>()
    private val downloads = ItemDownloads(downloadsByItem, download = { downloadsAsked += it }, retry = {}, remove = {}, play = {})

    private fun show(detail: ItemDetail, devices: List<OtherDevice> = emptyList(), withDownloads: Boolean = false): ItemData {
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
                    (state as? ItemUiState.Ready)?.let { ItemPageContent(it.page, model, navigator, downloads.takeIf { withDownloads }) }
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

    private fun download(state: DownloadState, downloadedBytes: Long = 0) = DownloadEntry(
        itemId = 1,
        fileId = 1,
        kind = ItemKind.Movie,
        title = "Saltwind",
        sizeBytes = 1_400_000_000,
        downloadedBytes = downloadedBytes,
        state = state,
        addedAt = 0,
    )

    @Test
    fun downloadIsAnActionThatDownloadsTheTitle() {
        show(movie(), withDownloads = true)
        compose.onNodeWithText("Download").assertExists()
        compose.onNodeWithContentDescription("Download Saltwind").performClick()
        compose.waitForIdle()
        assertEquals(listOf(listOf(1L)), downloadsAsked)
    }

    @Test
    fun aComingDownloadSaysHowFar() {
        downloadsByItem.value = mapOf(1L to download(DownloadState.Downloading, downloadedBytes = 630_000_000))
        show(movie(), withDownloads = true)
        compose.onNodeWithText("45%").assertExists()
        compose.onNodeWithContentDescription("Saltwind is downloading, 45%").assertExists()
    }

    @Test
    fun aWaitingDownloadsMenuSaysWhatItWaitsFor() {
        downloadsByItem.value = mapOf(1L to download(DownloadState.Waiting))
        show(movie(), withDownloads = true)
        compose.onNodeWithContentDescription("Saltwind, Waiting for Wi-Fi").assertExists()
        compose.onNodeWithText("Waiting").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Waiting for Wi-Fi").assertExists()
        compose.onNodeWithText("Cancel download").assertExists()
    }

    @Test
    fun aWholeDownloadSaysSo() {
        downloadsByItem.value = mapOf(1L to download(DownloadState.Done, downloadedBytes = 1_400_000_000))
        show(movie(), withDownloads = true)
        compose.onNodeWithContentDescription("Saltwind is downloaded").assertExists()
        compose.onNodeWithText("Downloaded").assertExists()
    }

    private fun withTrailer(detail: ItemDetail) =
        detail.copy(extras = listOf(ItemSummary(9, 1, ItemKind.Movie, "Trailer", extraType = ExtraType.Trailer)))

    private fun top(label: String) = compose.onNodeWithText(label).getUnclippedBoundsInRoot().top

    @Test
    fun sixActionsShareOneRow() {
        show(withTrailer(movie(positionMs = 3_480_000)), withDownloads = true)
        assertEquals(top("From start"), top("Trailer"))
    }

    @Test
    @Config(qualifiers = "w320dp-h640dp-xhdpi", fontScale = 2.0f)
    fun atTheLargestFontTheActionsWrapRatherThanBreakAWord() {
        show(withTrailer(movie(positionMs = 3_480_000)), withDownloads = true)
        // Two a row: "From start" and "Watched" above, "Download" and "Trailer" at the bottom.
        assertEquals(top("From start"), top("Watched"))
        assertTrue(top("Trailer") > top("Watched"))
        assertEquals(top("Download"), top("Trailer"))
    }

    @Test
    @Config(qualifiers = "fr-w360dp-h780dp-xxhdpi")
    fun fiveFrenchActionsShareOneRowOnASmallPhone() {
        show(withTrailer(movie()), withDownloads = true)
        // "Bande-annonce" goes on two lines at its hyphen rather than push the row onto two.
        assertEquals(top("Vu"), top("Bande-annonce"))
    }
}
