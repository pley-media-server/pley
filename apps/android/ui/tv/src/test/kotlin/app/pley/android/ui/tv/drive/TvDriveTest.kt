package app.pley.android.ui.tv.drive

import androidx.activity.ComponentActivity
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import app.pley.android.common.Tier
import app.pley.android.data.drive.DriveSpace
import app.pley.android.screens.drive.DriveLine
import app.pley.android.screens.drive.DrivePage
import app.pley.android.screens.drive.DrivePlace
import app.pley.android.ui.tv.kit.focused
import app.pley.android.ui.tv.kit.press
import app.pley.android.ui.tv.theme.TvTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The USB drive's page with the remote: its lines, where it is, and what it says when it can't list a folder. */
@RunWith(RobolectricTestRunner::class)
@Config(qualifiers = "w960dp-h540dp-xhdpi")
class TvDriveTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val opened = mutableListOf<String>()
    private var ups = 0
    private var settings = 0

    private val season = DriveLine("/storage/1A2B-3C4D/Films/Season 2", "Season 2", folder = true)
    private val film = DriveLine(
        "/storage/1A2B-3C4D/Films/Mad Max.mkv",
        "Mad Max",
        folder = false,
        kind = "MKV",
        resumeMs = 3_723_000,
        progress = 0.5f,
        sizeBytes = 4_000_000_000,
    )

    private fun show(page: DrivePage) {
        compose.setContent {
            TvTheme(Tier.T2, reducedMotion = true) {
                TvDriveContent(page, onLine = { opened += it.name }, onUp = { ups++ }, onOpenSettings = { settings++ })
            }
        }
        compose.waitForIdle()
    }

    @Test
    fun aFolderListsUpThenItsFoldersThenItsVideosAndFocusStartsOnTheFirst() {
        show(DrivePage.Folder(DrivePlace.Under(listOf("Films")), up = true, lines = listOf(season, film)))
        compose.onNodeWithText("USB drive › Films").assertExists()
        compose.onNodeWithText("Up a folder").assertExists()
        compose.onNodeWithText("MKV · Resumes at 1:02:03").assertExists()
        compose.onNodeWithText("3.7 GB").assertExists()
        compose.focused().assert(hasText("Season 2"))
        compose.press(Key.Enter)
        compose.press(Key.DirectionDown)
        compose.focused().assert(hasText("Mad Max"))
        compose.press(Key.Enter)
        assertEquals(listOf("Season 2", "Mad Max"), opened)
        // ".." goes back up.
        compose.press(Key.DirectionUp, 2)
        compose.focused().assert(hasText("Up a folder"))
        compose.press(Key.Enter)
        assertEquals(1, ups)
    }

    @Test
    fun theDrivesTopSaysHowFullItIs() {
        show(DrivePage.Folder(DrivePlace.Top(DriveSpace(437_000_000_000, 1_000_000_000_000)), up = false, lines = listOf(film)))
        compose.onNodeWithText("407 GB free of 931 GB").assertExists()
        compose.onNodeWithText("..").assertDoesNotExist()
        compose.focused().assert(hasText("Mad Max"))
    }

    @Test
    fun anEmptyFolderSaysSoUnderUp() {
        show(DrivePage.Folder(DrivePlace.Under(listOf("Films", "2019")), up = true, lines = emptyList()))
        compose.onNodeWithText("USB drive › Films › 2019").assertExists()
        compose.onNodeWithText("Nothing to play here").assertExists()
        compose.focused().assert(hasText("Up a folder"))
    }

    @Test
    fun refusedItLeadsToTheSettings() {
        show(DrivePage.Refused)
        compose.focused().assert(hasText("Open settings"))
        compose.press(Key.Enter)
        assertEquals(1, settings)
    }

    @Test
    fun withoutADriveOrAReadableFolderItSaysWhy() {
        show(DrivePage.NoDrive)
        compose.onNodeWithText("No USB drive").assertExists()
        compose.onNodeWithText("Plug a USB drive into the TV to play its videos here.").assertExists()
    }
}
