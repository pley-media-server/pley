package app.pley.android.data.drive

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.os.Build
import android.os.Environment
import android.os.storage.StorageManager
import android.os.storage.StorageVolume
import androidx.annotation.RequiresApi
import java.io.File
import java.io.IOException
import java.util.concurrent.Executor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * The drive plugged into the device, as Android mounts it: the first
 * volume of StorageManager's that is a drive ([pick]), by its folder. It
 * follows drives plugged in and pulled out through the media broadcasts
 * and, from Android 11, StorageManager's callback; [refresh] reads them
 * again for builds that send neither.
 */
class StorageDrives(
    context: Context,
    override val resume: DriveResume,
    override val files: DriveFiles = FileDriveFiles(),
) : Drives {
    private val app = context.applicationContext
    private val storage: StorageManager? = app.getSystemService(StorageManager::class.java)
    private val state = MutableStateFlow<Drive?>(null)
    override val drive: StateFlow<Drive?> = state.asStateFlow()

    /** One look at a time, off the main thread: StorageManager asks the system, and a folder check reads the drive. */
    @OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)
    private val looks = CoroutineScope(SupervisorJob() + Dispatchers.IO.limitedParallelism(1))

    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) = refresh()
    }

    init {
        val filter = IntentFilter().apply {
            MEDIA_ACTIONS.forEach(::addAction)
            addDataScheme("file")
        }
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                app.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
            } else {
                app.registerReceiver(receiver, filter)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Api30.watch(storage, ::refresh)
        } catch (_: RuntimeException) {
            // Not told of drives coming and going: what is there when a page opens still shows.
        }
        refresh()
    }

    override fun refresh() {
        looks.launch { state.value = look() }
    }

    override fun missing(): List<String> =
        Drives.drivePermissions(Build.VERSION.SDK_INT).filter { app.checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }

    private fun look(): Drive? {
        val volumes = try {
            storage?.storageVolumes.orEmpty().map(::volumeOf)
        } catch (_: RuntimeException) {
            // The system wouldn't say: the folders it mounts drives at are looked at instead.
            emptyList()
        }
        val root = pick(volumes, ::isFolder) ?: mountedUnder(File(STORAGE)) ?: return null
        val said = try {
            File(MOUNTS).readLines()
        } catch (_: IOException) {
            emptyList()
        } catch (_: SecurityException) {
            emptyList()
        }
        return Drive(root, name(root, said, ::readSys) ?: describedAs(root, volumes))
    }

    private fun volumeOf(volume: StorageVolume): Volume = Volume(
        primary = volume.isPrimary,
        removable = volume.isRemovable,
        emulated = volume.isEmulated,
        state = volume.state,
        directory = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Api30.directory(volume) else null,
        uuid = volume.uuid,
        description = try {
            volume.getDescription(app)
        } catch (_: RuntimeException) {
            null
        },
    )

    /** A volume as [pick] weighs it. */
    internal data class Volume(
        val primary: Boolean,
        val removable: Boolean,
        val emulated: Boolean,
        /** Environment's: "mounted", "mounted_ro"… */
        val state: String?,
        /** Its folder, as Android 11 and later say it; `null` before. */
        val directory: String?,
        /** Its file system's id ("1A2B-3C4D"), which public volumes are mounted under in /storage. */
        val uuid: String?,
        /** The system's name for it: "SanDisk USB drive", or "USB drive" when the drive gives no make. */
        val description: String? = null,
    )

    @RequiresApi(Build.VERSION_CODES.R)
    private object Api30 {
        fun directory(volume: StorageVolume): String? = volume.directory?.path

        fun watch(storage: StorageManager?, changed: () -> Unit) {
            val direct = Executor { it.run() }
            storage?.registerStorageVolumeCallback(
                direct,
                object : StorageManager.StorageVolumeCallback() {
                    override fun onStateChanged(volume: StorageVolume) = changed()
                },
            )
        }
    }

    internal companion object {
        /** Where Android mounts public volumes, each under its file system's id. */
        const val STORAGE = "/storage"

        /** Where the system mounts a public volume itself, under its id, before /storage shows it to apps. */
        private const val MEDIA_RW = "/mnt/media_rw"

        /** The mounts this app sees, one a line: "device folder type options …". */
        private const val MOUNTS = "/proc/self/mounts"

        private val MEDIA_ACTIONS = listOf(
            Intent.ACTION_MEDIA_MOUNTED,
            Intent.ACTION_MEDIA_UNMOUNTED,
            Intent.ACTION_MEDIA_EJECT,
            Intent.ACTION_MEDIA_REMOVED,
            Intent.ACTION_MEDIA_BAD_REMOVAL,
        )

        /** A volume id as FAT and exFAT ("1A2B-3C4D") or NTFS and ext4 (a UUID, or 16 hex digits) name theirs. */
        private val VOLUME_ID = Regex("^([0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}|[0-9A-Fa-f]{16}|[0-9A-Fa-f]{8}(-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12})$")

        /**
         * The drive among [volumes]: not the device's own storage (primary,
         * emulated), mounted (read-write or read-only), removable first, as
         * some TV builds don't mark a USB drive removable. Its folder is the
         * one Android 11 and later give, else /storage/<id>, where Android
         * mounts public volumes; [isFolder] says which exist.
         */
        fun pick(volumes: List<Volume>, isFolder: (String) -> Boolean): String? {
            val mounted = volumes.filter {
                !it.primary && !it.emulated && (it.state == Environment.MEDIA_MOUNTED || it.state == Environment.MEDIA_MOUNTED_READ_ONLY)
            }
            return (mounted.filter { it.removable } + mounted.filter { !it.removable }).firstNotNullOfOrNull { v ->
                v.directory?.takeIf(isFolder) ?: v.uuid?.let { "$STORAGE/$it" }?.takeIf(isFolder)
            }
        }

        /**
         * A drive mounted under [storage] that the system didn't list: a
         * folder named as volumes are, which can be read; the first by name.
         */
        fun mountedUnder(storage: File): String? = storage.listFiles().orEmpty()
            .filter { VOLUME_ID.matches(it.name) && it.isDirectory && it.canRead() }
            .minByOrNull { it.name }?.path

        private fun isFolder(path: String): Boolean = File(path).isDirectory

        /**
         * The make and model of the drive mounted at [root] as it reports
         * them ("Kingston DataTraveler 3.0"), as the Samsung app reads them:
         * the disk it is mounted from, in [mounts] (lines of /proc/self/mounts),
         * then what the disk says of itself in /sys, through [read]. `null`
         * when any of it is out of reach, as it is on most TVs: the app may
         * not open the device itself, nor see where the system mounts it.
         */
        fun name(root: String, mounts: List<String>, read: (String) -> String?): String? {
            val at = setOf(root, "$MEDIA_RW/${root.substringAfterLast('/')}")
            val device = mounts.map { it.split(' ') }.firstOrNull { it.size > 1 && it[1] in at }?.get(0) ?: return null
            val folders = when {
                // Android's own: "/dev/block/vold/public:8,1", the disk's numbers, a part of a disk or the whole of it.
                device.startsWith(VOLD) -> device.removePrefix(VOLD).replace(',', ':').let { listOf("/sys/dev/block/$it/../device", "/sys/dev/block/$it/device") }
                // sda1 is a part of the disk sda (mmcblk0p1 of mmcblk0), whose folder holds what the drive says of itself.
                device.startsWith("/dev/") -> listOf("/sys/block/${device.substringAfterLast('/').replace(PART, "")}/device")
                else -> return null
            }
            return folders.firstNotNullOfOrNull { folder ->
                listOf("vendor", "model").mapNotNull { read("$folder/$it")?.trim()?.ifEmpty { null } }.joinToString(" ").ifEmpty { null }
            }
        }

        /** The system's name for the volume mounted at [root], among [volumes]. */
        fun describedAs(root: String, volumes: List<Volume>): String? =
            volumes.firstOrNull { it.directory == root || it.uuid?.let { id -> "$STORAGE/$id" } == root }?.description?.trim()?.ifEmpty { null }

        /** A file of /sys, when there is one and the app may read it. */
        private fun readSys(path: String): String? = try {
            File(path).takeIf { it.isFile }?.readText()
        } catch (_: IOException) {
            null
        } catch (_: SecurityException) {
            null
        }

        private const val VOLD = "/dev/block/vold/public:"

        /** A partition's number at the end of its device's name: sda1's 1, mmcblk0p1's p1. */
        private val PART = Regex("""(?<=\d)p\d+$|(?<=[a-z])\d+$""")
    }
}
