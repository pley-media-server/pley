package app.pley.android.data.drive

import java.io.File
import java.io.IOException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** A drive plugged into the device, by the folder the device mounts it at: "/storage/1A2B-3C4D". */
data class Drive(val root: String)

/** How full a drive is. */
data class DriveSpace(val freeBytes: Long, val totalBytes: Long)

/** A drive's folders as the app reads them: the device's ([FileDriveFiles]), or a fake tree in tests. */
interface DriveFiles {
    /** What [dir] holds, as it is; throws [IOException] when it can't be read (the drive pulled out, no permission). */
    fun read(dir: String): List<DriveFile>

    /** How full the drive at [root] is; `null` when the device won't say. */
    fun space(root: String): DriveSpace?
}

/**
 * The File API's view of a drive: on Android 10 and earlier every file
 * (READ_EXTERNAL_STORAGE), from Android 11 the media files and folders
 * (READ_EXTERNAL_STORAGE, from 13 READ_MEDIA_VIDEO).
 */
class FileDriveFiles : DriveFiles {
    override fun read(dir: String): List<DriveFile> {
        val children = File(dir).listFiles() ?: throw IOException("$dir can't be read")
        return children.map { DriveFile(it.name, folder = it.isDirectory, size = if (it.isDirectory) 0 else it.length()) }
    }

    override fun space(root: String): DriveSpace? = try {
        val folder = File(root)
        folder.totalSpace.takeIf { it > 0 }?.let { DriveSpace(folder.usableSpace, it) }
    } catch (_: SecurityException) {
        null
    }
}

/**
 * Videos on a drive plugged into the device (Android TV and Fire TV): the
 * drive there is now, its folders, and where each video stopped.
 */
interface Drives {
    /** The drive plugged in, the first one mounted; `null` while none is. */
    val drive: StateFlow<Drive?>

    /** Looks for drives again (a page opened, the app came back): some TV builds say nothing when one comes or goes. */
    fun refresh() {}

    val files: DriveFiles

    /** Where each video stopped, kept on this device. */
    val resume: DriveResume

    companion object {
        /** No drives: phones, tests. */
        val None: Drives = object : Drives {
            override val drive: StateFlow<Drive?> = MutableStateFlow<Drive?>(null).asStateFlow()
            override val files: DriveFiles = object : DriveFiles {
                override fun read(dir: String): List<DriveFile> = throw IOException("No drives here")

                override fun space(root: String): DriveSpace? = null
            }
            override val resume: DriveResume = DriveResume.memory()
        }
    }
}
