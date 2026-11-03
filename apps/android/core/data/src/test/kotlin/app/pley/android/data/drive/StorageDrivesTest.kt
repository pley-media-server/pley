package app.pley.android.data.drive

import app.pley.android.data.drive.StorageDrives.Volume
import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Which of the volumes Android lists is the drive, as TV builds report them, and what it is called. */
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
    fun androidThirteenAsksForVideosAndEarlierVersionsForStorage() {
        assertEquals(listOf("android.permission.READ_EXTERNAL_STORAGE"), Drives.drivePermissions(28))
        assertEquals(listOf("android.permission.READ_EXTERNAL_STORAGE"), Drives.drivePermissions(32))
        assertEquals(listOf("android.permission.READ_MEDIA_VIDEO"), Drives.drivePermissions(33))
        assertEquals(listOf("android.permission.READ_MEDIA_VIDEO"), Drives.drivePermissions(37))
    }

    @Test
    fun theDrivesNameIsItsMakeAndModelFromTheDiskItIsMountedFrom() {
        val sys = mapOf(
            "/sys/block/sda/device/vendor" to "Kingston\n",
            "/sys/block/sda/device/model" to "DataTraveler 3.0 \n",
            "/sys/dev/block/8:17/../device/vendor" to "SanDisk ",
            "/sys/dev/block/8:17/../device/model" to "Ultra",
        )
        val mounts = listOf(
            "/dev/fuse /storage/1A2B-3C4D fuse rw,nosuid 0 0",
            "/dev/block/vold/public:8,17 /mnt/media_rw/5E6F-7A8B vfat rw 0 0",
            "/dev/sda1 /mnt/usb/sda1 exfat rw 0 0",
        )
        // A disk Linux names (sda1, a part of sda).
        assertEquals("Kingston DataTraveler 3.0", StorageDrives.name("/mnt/usb/sda1", mounts, sys::get))
        // Android's own mount, where /storage shows the volume to apps.
        assertEquals("SanDisk Ultra", StorageDrives.name("/storage/5E6F-7A8B", mounts, sys::get))
        // What the app sees is the system's file system, and /sys says nothing: no name.
        assertNull(StorageDrives.name("/storage/1A2B-3C4D", mounts, sys::get))
        assertNull(StorageDrives.name("/storage/1A2B-3C4D", emptyList(), sys::get))
    }

    @Test
    fun withoutItsMakeTheDriveHasTheSystemsNameForIt() {
        val volumes = listOf(
            volume(directory = "/storage/1A2B-3C4D").copy(description = "SanDisk USB drive"),
            volume(uuid = "5E6F-7A8B").copy(description = " "),
        )
        assertEquals("SanDisk USB drive", StorageDrives.describedAs("/storage/1A2B-3C4D", volumes))
        assertNull(StorageDrives.describedAs("/storage/5E6F-7A8B", volumes))
        assertNull(StorageDrives.describedAs("/storage/9C33-6BBD", volumes))
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
