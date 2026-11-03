package app.pley.android.data.drive

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Where a video on a drive stopped: kept on the device until it was watched (the Samsung app's LocalTests). */
class DriveResumeTest {
    @Test
    fun whereAVideoStoppedIsKeptUntilItWasWatched() {
        val resume = DriveResume.memory()
        resume.keep("/storage/1A2B-3C4D/Film.mkv", 600_000, 7_200_000)
        assertEquals(DriveKept(600_000, 7_200_000), resume.kept("/storage/1A2B-3C4D/Film.mkv"))
        assertEquals(DriveKept(), resume.kept("/storage/1A2B-3C4D/Other.mkv"))
        // Past 95 %: watched, and forgotten.
        resume.keep("/storage/1A2B-3C4D/Film.mkv", 7_000_000, 7_200_000)
        assertEquals(DriveKept(), resume.kept("/storage/1A2B-3C4D/Film.mkv"))
        // Its length unknown, the place is kept all the same.
        resume.keep("/storage/1A2B-3C4D/Clip.ts", 5_000, null)
        assertEquals(DriveKept(5_000, 0), resume.kept("/storage/1A2B-3C4D/Clip.ts"))
    }

    @Test
    fun aFilesIdIsItsPathsAndTheSamsungAppsHash() {
        val id = DriveResume.fileIdOf("/storage/1A2B-3C4D/Film.mkv")
        assertEquals(id, DriveResume.fileIdOf("/storage/1A2B-3C4D/Film.mkv"))
        assertNotEquals(id, DriveResume.fileIdOf("/storage/1A2B-3C4D/Film 2.mkv"))
        assertTrue(id > 0)
        // FNV-1a of nothing is its offset basis, less the two top bits.
        assertEquals(-3750763034362895579L and 0x3fffffffffffffffL, DriveResume.fileIdOf(""))
    }
}
