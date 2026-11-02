package app.pley.android.data.drive

import app.pley.android.data.drive.StorageDrives.Volume
import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Which of the volumes Android lists is the drive, as TV builds report them. */
class StorageDrivesTest {
    private val folders = setOf("/storage/1A2B-3C4D", "/mnt/media_rw/1A2B-3C4D", "/storage/5E6F-7A8B")

    private fun volume(
        removable: Boolean = true,
        state: String? = "mounted",
        directory: String? = null,
        uuid: String? = "1A2B-3C4D",
        primary: Boolean = false,
        emulated: Boolean = false,
    ) = Volume(primary, removable, emulated, state, directory, uuid)

    private fun pick(vararg volumes: Volume) = StorageDrives.pick(volumes.toList(), folders::contains)

    @Test
    fun theDriveIsTheFirstMountedVolumeThatIsntTheDevicesOwn() {
        val own = volume(removable = false, primary = true, emulated = true, directory = "/storage/emulated/0", uuid = null)
        assertEquals("/storage/1A2B-3C4D", pick(own, volume(directory = "/storage/1A2B-3C4D")))
        // Read-only counts; one being checked or pulled out doesn't.
        assertEquals("/storage/5E6F-7A8B", pick(volume(state = "checking"), volume(state = "mounted_ro", uuid = "5E6F-7A8B")))
        assertNull(pick(own, volume(state = "unmounted"), volume(state = "bad_removal")))
    }

    @Test
    fun buildsThatReportVolumesDifferentlyStillFindIt() {
        // Before Android 11 there is no directory: /storage/<id>, where Android mounts public volumes.
        assertEquals("/storage/1A2B-3C4D", pick(volume(directory = null)))
        // A directory that isn't there falls back to /storage/<id> too.
        assertEquals("/storage/1A2B-3C4D", pick(volume(directory = "/mnt/usb/sda1")))
        // A USB drive not marked removable, after the removable ones.
        assertEquals("/storage/5E6F-7A8B", pick(volume(removable = false), volume(uuid = "5E6F-7A8B")))
        assertEquals("/storage/1A2B-3C4D", pick(volume(removable = false)))
        // Neither a folder nor an id: none.
        assertNull(pick(volume(directory = null, uuid = null)))
    }

    @Test
    fun aDriveTheSystemDidntListIsFoundUnderStorage() {
        val storage = Files.createTempDirectory("pley-storage").toFile()
        try {
            File(storage, "emulated").mkdirs()
            File(storage, "self").mkdirs()
            assertNull(StorageDrives.mountedUnder(storage))
            File(storage, "9C33-6BBD").mkdirs()
            File(storage, "1A2B-3C4D").mkdirs()
            assertEquals(File(storage, "1A2B-3C4D").path, StorageDrives.mountedUnder(storage))
        } finally {
            storage.deleteRecursively()
        }
    }
}
