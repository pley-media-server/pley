@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.drive

import app.pley.android.data.drive.Drive
import app.pley.android.data.drive.DriveFile
import app.pley.android.data.drive.DriveFiles
import app.pley.android.data.drive.DriveResume
import app.pley.android.data.drive.DriveSpace
import app.pley.android.data.drive.Drives
import app.pley.android.player.PlayRequest
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestCoroutineScheduler
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** A drive's page on a fake file tree (the Samsung app's StorageScreen and LocalTests). */
class DriveViewModelTest {
    private val dispatcher = UnconfinedTestDispatcher()

    /** A drive with a fake file tree: folders by path, and those that can't be read. */
    private class FakeDrives : Drives {
        val plugged = MutableStateFlow<Drive?>(Drive(ROOT))
        val tree = HashMap<String, List<DriveFile>>()
        val unreadable = HashSet<String>()
        var denied = listOf<String>()
        var looks = 0
        override val drive: StateFlow<Drive?> = plugged
        override val resume: DriveResume = DriveResume.memory()
        override val files: DriveFiles = object : DriveFiles {
            override fun read(dir: String): List<DriveFile> {
                if (dir in unreadable) throw IOException("$dir can't be read")
                return tree[dir] ?: throw IOException("$dir isn't there")
            }

            override fun space(root: String): DriveSpace = DriveSpace(freeBytes = 437_000_000_000, totalBytes = 1_000_000_000_000)
        }

        override fun refresh() {
            looks++
        }

        override fun missing(): List<String> = denied
    }

    private val drives = FakeDrives().apply {
        tree[ROOT] = listOf(
            DriveFile("Season 10", folder = true),
            DriveFile("Films", folder = true),
            DriveFile("\$RECYCLE.BIN", folder = true),
            DriveFile("System Volume Information", folder = true),
            DriveFile("Mad Max.mkv", folder = false, size = 4_000_000_000),
            DriveFile("Mad Max.en.srt", folder = false, size = 40_000),
            DriveFile("._Mad Max.mkv", folder = false, size = 4_096),
            DriveFile("Clip.MP4", folder = false, size = 90_000_000),
            DriveFile("notes.txt", folder = false),
        )
        tree["$ROOT/Films"] = listOf(DriveFile("2019", folder = true))
        tree["$ROOT/Films/2019"] = emptyList()
    }
    private val played = mutableListOf<PlayRequest>()

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    private fun page(folder: String? = null) = DriveViewModel(drives, folder, { played += it }, dispatcher)

    @Test
    fun theDrivesTopListsItsFoldersThenItsVideosAndHowFullItIs() {
        drives.resume.keep("$ROOT/Mad Max.mkv", 2_214_000, 7_380_000)
        val page = page().state.value.page as DrivePage.Folder
        assertEquals(DrivePlace.Top(DriveSpace(437_000_000_000, 1_000_000_000_000)), page.place)
        // No ".." at the top; hidden files, what Windows and macOS leave and what doesn't play aren't shown.
        assertEquals(false, page.up)
        assertEquals(listOf("Films", "Season 10", "Clip", "Mad Max"), page.lines.map { it.name })
        assertEquals(listOf(true, true, false, false), page.lines.map { it.folder })
        val film = page.lines.last()
        assertEquals("$ROOT/Mad Max.mkv", film.path)
        assertEquals("MKV", film.kind)
        assertEquals(2_214_000L, film.resumeMs)
        assertEquals(0.3f, film.progress, 0.001f)
        assertEquals(4_000_000_000L, film.sizeBytes)
        // Not started: no resume line, no bar.
        assertEquals(0L to 0f, page.lines[2].resumeMs to page.lines[2].progress)
    }

    @Test
    fun aFolderSaysWhereItIsAndGoesUpFirst() {
        val films = page("$ROOT/Films").state.value.page as DrivePage.Folder
        assertEquals(DrivePlace.Under(listOf("Films")), films.place)
        assertTrue(films.up)
        assertEquals(listOf("2019"), films.lines.map { it.name })
        assertEquals("$ROOT/Films/2019", films.lines.single().path)
        // An empty folder: nothing to play here, with ".." still there to go back up.
        val year = page("$ROOT/Films/2019").state.value.page as DrivePage.Folder
        assertEquals(DrivePlace.Under(listOf("Films", "2019")), year.place)
        assertTrue(year.up)
        assertTrue(year.lines.isEmpty())
    }

    @Test
    fun thePageIsTitledWithTheDrivesName() {
        drives.plugged.value = Drive(ROOT, name = "Kingston DataTraveler 3.0")
        val vm = page("$ROOT/Films")
        assertEquals("Kingston DataTraveler 3.0", vm.state.value.name)
        drives.plugged.value = null
        dispatcher.scheduler.advanceUntilIdle()
        assertEquals(DrivePage.NoDrive, vm.state.value.page)
        assertEquals(null, vm.state.value.name)
    }

    @Test
    fun aFolderThatCantBeReadSaysSoAndNoDriveSaysPlugOneIn() {
        drives.unreadable += "$ROOT/Films"
        assertEquals(DrivePage.Unreadable(DrivePlace.Under(listOf("Films")), up = true), page("$ROOT/Films").state.value.page)
        drives.plugged.value = null
        assertEquals(DrivePage.NoDrive, page().state.value.page)
    }

    @Test
    fun thePageFollowsTheDrivePulledOutAndPluggedInAgain() {
        val vm = page("$ROOT/Films")
        drives.plugged.value = null
        assertEquals(DrivePage.NoDrive, vm.state.value.page)
        drives.plugged.value = Drive(ROOT)
        assertEquals(listOf("2019"), (vm.state.value.page as DrivePage.Folder).lines.map { it.name })
        // Another drive in its place: this folder isn't on it.
        drives.plugged.value = Drive("/storage/5E6F-7A8B")
        assertEquals(DrivePage.NoDrive, vm.state.value.page)
    }

    @Test
    fun leaveToReadTheDriveIsAskedForAsThePageOpensAndSettingsCanGiveItLater() {
        drives.denied = listOf("android.permission.READ_MEDIA_VIDEO")
        val vm = page()
        assertEquals(DrivePage.Loading, vm.state.value.page)
        assertEquals(drives.denied, vm.state.value.ask)
        vm.asked()
        assertTrue(vm.state.value.ask.isEmpty())
        vm.permissionsAnswered()
        assertEquals(DrivePage.Refused, vm.state.value.page)
        // Still refused when it shows again; allowed in the settings meanwhile, the folder shows.
        vm.resumed()
        assertEquals(DrivePage.Refused, vm.state.value.page)
        drives.denied = emptyList()
        vm.resumed()
        assertEquals(4, (vm.state.value.page as DrivePage.Folder).lines.size)
        assertEquals("it looks for the drive again once allowed", 1, drives.looks)
    }

    @Test
    fun backFromThePlayerEachVideoSaysWhereItResumes() {
        val vm = page()
        val film = (vm.state.value.page as DrivePage.Folder).lines.last()
        vm.play(film)
        assertEquals(listOf(PlayRequest.drive("$ROOT/Mad Max.mkv")), played)
        // Folders open as pages of their own: playing one does nothing.
        vm.play((vm.state.value.page as DrivePage.Folder).lines.first())
        assertEquals(1, played.size)

        // Watched for 10 s of two hours: a bar all the same.
        drives.resume.keep(film.path, 10_000, 7_200_000)
        vm.resumed()
        val again = (vm.state.value.page as DrivePage.Folder).lines.last()
        assertEquals(10_000L, again.resumeMs)
        assertEquals(DriveViewModel.MIN_PROGRESS, again.progress)
        assertTrue("drives are looked for again as the page shows", drives.looks > 0)
    }

    @Test
    fun aFolderReadAgainIsNotUndoneByTheLinesOfTheOneBefore() {
        // The drive is read on a clock of its own, so both reads below wait for it together.
        val disk = TestCoroutineScheduler()
        drives.tree[OTHER] = listOf(DriveFile("Fury Road.mkv", folder = false, size = 1))
        val vm = DriveViewModel(drives, null, { played += it }, StandardTestDispatcher(disk))
        disk.runCurrent()
        assertEquals(4, (vm.state.value.page as DrivePage.Folder).lines.size)
        // Another drive comes in as the page shows again: its folder is read first, then the lines before.
        drives.plugged.value = Drive(OTHER)
        dispatcher.scheduler.runCurrent()
        vm.resumed()
        disk.runCurrent()
        assertEquals(listOf("Fury Road"), (vm.state.value.page as DrivePage.Folder).lines.map { it.name })
    }

    private companion object {
        const val OTHER = "/storage/5E6F-7A8B"
        const val ROOT = "/storage/1A2B-3C4D"
    }
}
