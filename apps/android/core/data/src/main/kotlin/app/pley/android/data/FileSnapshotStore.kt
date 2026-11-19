package app.pley.android.data

import app.pley.android.common.sha1Hex
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The last answer of each screen on disk: `<root>/<server>/<user>/<key>`,
 * where root is `filesDir/snapshots`, server the SHA-1 of its origin and
 * user the profile's id.
 *
 * - Everything is read and written on [io], never the caller's thread.
 * - A file is written whole to a temporary file, then moved over the old
 *   one, so a crash leaves the old answer or the new one, never half.
 * - The store holds [maxBytes] at most (25 MB): the answers read or written
 *   longest ago go first.
 * - Each server keeps the snapshots of its last [KEEP_PROFILES] profiles.
 * - The active profile's Home is copied to `<root>/startup/home.json`,
 *   which the app's startup read (StartupRead) gives the first frame.
 *
 * [read] and [write] without a scope act on the [active] profile's.
 */
class FileSnapshotStore(
    rootOf: () -> File,
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val maxBytes: Long = MAX_BYTES,
) : SnapshotStore {
    /** The profile in use, whose Home is the startup copy; set by the session. */
    @Volatile
    override var active: ProfileScope? = null
        private set

    private val lock = Any()

    /** Every snapshot file and its size, the least recently used first; read from disk at first use. */
    private var index: LinkedHashMap<String, Long>? = null
    private var total = 0L

    /** Resolved on first use, on [io]: asking Android for filesDir touches the disk. */
    private val root: File by lazy(rootOf)
    private val startupDir by lazy { File(root, STARTUP) }
    private val startupHome by lazy { File(startupDir, STARTUP_HOME) }
    private val startupOwner by lazy { File(startupDir, STARTUP_OWNER) }

    override suspend fun read(key: String): ByteArray? = active?.let { read(it, key) }

    override suspend fun write(key: String, bytes: ByteArray) {
        active?.let { write(it, key, bytes) }
    }

    /** [scope]'s snapshot of [key], `null` when there is none. */
    override suspend fun read(scope: ProfileScope, key: String): ByteArray? = withContext(io) {
        val file = fileOf(scope, key)
        val bytes = try {
            file.readBytes()
        } catch (_: FileNotFoundException) {
            null
        } catch (_: IOException) {
            null
        }
        synchronized(lock) {
            val known = index()
            if (bytes == null) {
                known.remove(file.path)?.let { total -= it }
            } else {
                // Touched: last to go.
                known[file.path]
            }
        }
        bytes?.takeIf { it.isNotEmpty() }
    }

    /** Keeps [bytes] as [scope]'s snapshot of [key]. */
    override suspend fun write(scope: ProfileScope, key: String, bytes: ByteArray) {
        withContext(io) {
            val file = fileOf(scope, key)
            try {
                writeAtomically(file, bytes)
            } catch (_: IOException) {
                // A full disk: the snapshot is only a head start.
                return@withContext
            }
            synchronized(lock) {
                val known = index()
                known.remove(file.path)?.let { total -= it }
                known[file.path] = bytes.size.toLong()
                total += bytes.size
                evict(keep = file.path)
            }
            if (key == SnapshotKeys.HOME && scope == active) writeStartup(scope, bytes)
        }
    }

    /** Deletes [scope]'s snapshot of [key]. */
    override suspend fun delete(scope: ProfileScope, key: String) {
        withContext(io) {
            val file = fileOf(scope, key)
            file.delete()
            synchronized(lock) { index().remove(file.path)?.let { total -= it } }
            if (key == SnapshotKeys.HOME && scope == active) clearStartup()
        }
    }

    /**
     * [scope] is the profile in use now (`null`: none, signed out): its
     * Home becomes the startup copy, and its server keeps the snapshots
     * of its last [KEEP_PROFILES] profiles only.
     */
    suspend fun activate(scope: ProfileScope?) {
        active = scope
        withContext(io) {
            if (scope == null) {
                clearStartup()
                return@withContext
            }
            val dir = File(root, "${scope.server}/${scope.userId}")
            dir.mkdirs()
            dir.setLastModified(System.currentTimeMillis())
            pruneProfiles(File(root, scope.server), keep = dir)
            val home = try {
                File(dir, fileName(SnapshotKeys.HOME)).readBytes()
            } catch (_: IOException) {
                null
            }
            if (home != null && home.isNotEmpty()) writeStartup(scope, home) else clearStartup()
        }
    }

    /** Deletes every snapshot of [issuer], its startup copy included. */
    override suspend fun clear(issuer: String) {
        val server = ProfileScope.serverKey(issuer)
        if (active?.server == server) active = null
        withContext(io) {
            val dir = File(root, server)
            dir.deleteRecursively()
            synchronized(lock) { forget(dir) }
            val owner = try {
                startupOwner.readText()
            } catch (_: IOException) {
                null
            }
            if (owner == null || owner.substringBefore('/') == server) clearStartup()
        }
    }

    /** What the store holds, in bytes (the startup copy aside). */
    suspend fun size(): Long = withContext(io) { synchronized(lock) { index(); total } }

    private fun fileOf(scope: ProfileScope, key: String): File = File(root, "${scope.server}/${scope.userId}/${fileName(key)}")

    private fun pruneProfiles(serverDir: File, keep: File) {
        val profiles = serverDir.listFiles { f -> f.isDirectory }?.toList().orEmpty()
        if (profiles.size <= KEEP_PROFILES) return
        val gone = profiles.filter { it != keep }
            .sortedByDescending { it.lastModified() }
            .drop(KEEP_PROFILES - 1)
        for (dir in gone) {
            dir.deleteRecursively()
            synchronized(lock) { forget(dir) }
        }
    }

    private fun writeStartup(scope: ProfileScope, home: ByteArray) {
        try {
            startupDir.mkdirs()
            writeAtomically(startupHome, home)
            writeAtomically(startupOwner, "${scope.server}/${scope.userId}".toByteArray())
        } catch (_: IOException) {
            clearStartup()
        }
    }

    private fun clearStartup() {
        startupHome.delete()
        startupOwner.delete()
    }

    /** Must hold [lock]. */
    private fun forget(dir: File) {
        val known = index()
        val prefix = dir.path + File.separator
        val gone = known.keys.filter { it.startsWith(prefix) }
        for (path in gone) known.remove(path)?.let { total -= it }
    }

    /** Must hold [lock]. Drops the least recently used files until the store fits, never [keep]. */
    private fun evict(keep: String) {
        val known = index()
        val it = known.entries.iterator()
        while (total > maxBytes && it.hasNext()) {
            val (path, size) = it.next()
            if (path == keep) continue
            File(path).delete()
            it.remove()
            total -= size
        }
    }

    /** Must hold [lock]: the files on disk, the oldest first, read the first time. */
    private fun index(): LinkedHashMap<String, Long> {
        index?.let { return it }
        val files = mutableListOf<File>()
        root.listFiles { f -> f.isDirectory && f.name != STARTUP }?.forEach { server ->
            server.listFiles { f -> f.isDirectory }?.forEach { profile ->
                profile.listFiles()?.forEach { file ->
                    if (file.name.endsWith(TMP)) file.delete() else if (file.isFile) files += file
                }
            }
        }
        val built = LinkedHashMap<String, Long>(files.size.coerceAtLeast(16), 0.75f, true)
        total = 0
        for (file in files.sortedBy { it.lastModified() }) {
            val size = file.length()
            built[file.path] = size
            total += size
        }
        index = built
        return built
    }

    companion object {
        /** The most every snapshot together holds. */
        const val MAX_BYTES = 25L * 1024 * 1024

        /** Profiles per server whose snapshots stay. */
        const val KEEP_PROFILES = 3

        /** Where the startup copy lives, beside the servers' folders. */
        const val STARTUP = "startup"
        const val STARTUP_HOME = "home.json"
        private const val STARTUP_OWNER = "owner"
        private const val TMP = ".tmp"
        private const val MAX_NAME = 120

        /**
         * A key as a file name: letters, digits, `-`, `_` and `.` as they
         * are, every other byte of its UTF-8 as `~xx`; long ones hashed.
         */
        fun fileName(key: String): String {
            val name = buildString {
                for (b in key.toByteArray(Charsets.UTF_8)) {
                    val c = b.toInt() and 0xff
                    val ch = c.toChar()
                    if (c < 0x80 && (ch.isLetterOrDigit() || ch == '-' || ch == '_' || ch == '.')) {
                        append(ch)
                    } else {
                        append('~').append(HEX[c shr 4]).append(HEX[c and 0xf])
                    }
                }
            }
            return if (name.length <= MAX_NAME && !name.endsWith(TMP)) name else "~h" + sha1Hex(key)
        }

        private const val HEX = "0123456789abcdef"

        /** Writes [bytes] to a temporary file beside [file], then moves it over [file] in one step. */
        internal fun writeAtomically(file: File, bytes: ByteArray) {
            val dir = file.parentFile ?: throw IOException("no folder for $file")
            if (!dir.isDirectory && !dir.mkdirs() && !dir.isDirectory) throw IOException("can't make $dir")
            val tmp = File.createTempFile("snap-" + file.name.take(40), TMP, dir)
            try {
                tmp.writeBytes(bytes)
                try {
                    Files.move(tmp.toPath(), file.toPath(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE)
                } catch (_: AtomicMoveNotSupportedException) {
                    Files.move(tmp.toPath(), file.toPath(), StandardCopyOption.REPLACE_EXISTING)
                }
            } finally {
                tmp.delete()
            }
        }
    }
}
