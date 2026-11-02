package app.pley.android.data.drive

import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction

/** What a folder holds, as the device reads it: a name, whether it's a folder, a file's size. */
data class DriveFile(val name: String, val folder: Boolean, val size: Long = 0)

/** A folder or a video in a folder of a drive, as its page lists it. */
data class DriveEntry(
    /** Where it is on the device: "/storage/1A2B-3C4D/Films/Ad Astra.mkv". */
    val path: String,
    val name: String,
    val folder: Boolean,
    /** A video's size in bytes; 0 for a folder, or when the drive won't say. */
    val size: Long = 0,
)

/** A subtitle file beside a video: "Film.en.forced.srt" for "Film.mkv". */
data class Sidecar(
    val path: String,
    /** As the server names them: "subrip" or "webvtt". */
    val codec: String,
    /** "en", "eng", "pt-br"; `null` when the name gives none. */
    val language: String?,
    /** What else the name says ("SDH"); `null` when nothing. */
    val title: String?,
    val forced: Boolean,
)

/**
 * What a drive's folders show, and the subtitle files beside a video, as
 * the Samsung app has them (apps/tizen-native, Local/LocalFolder.cs): the
 * videos Media3 opens, without hidden files and what Windows and macOS
 * leave on a drive, folders first, each by name as a person sorts them.
 */
object DriveFolder {
    /**
     * The containers Media3 opens. Not M2TS and MTS, which the Samsung app
     * lists: Media3 reads MPEG-TS in 188-byte packets only, and theirs are
     * 192 bytes.
     */
    private val VIDEOS = setOf("mkv", "mp4", "m4v", "mov", "ts", "webm", "avi", "mpg", "mpeg")

    /** What Windows and macOS leave on a drive, never shown (macOS's "._" files and .Trashes go as hidden). */
    private val SYSTEM = setOf("system volume information", "\$recycle.bin", "recycler", "lost+found", "thumbs.db", "desktop.ini")

    /** A folder shows this many entries at most, folders first: a drive's folder is read whole, but only so much is kept. */
    const val MAX_ENTRIES = 5_000

    fun isVideo(name: String): Boolean = extensionOf(name) in VIDEOS

    /** Hidden ones (a dot, a dollar) and the systems' own aren't shown. */
    fun shown(name: String): Boolean = name.isNotEmpty() && name[0] != '.' && name[0] != '$' && name.lowercase() !in SYSTEM

    /**
     * [dir]'s folders, then its videos, each by name as a person sorts them
     * ("Part 2" before "Part 10"), from [files], what it holds; other files
     * aren't shown.
     */
    fun entries(dir: String, files: List<DriveFile>): List<DriveEntry> {
        val folders = files.filter { it.folder && shown(it.name) }
            .map { DriveEntry(join(dir, it.name), it.name, folder = true) }
        val videos = files.filter { !it.folder && shown(it.name) && isVideo(it.name) }
            .map { DriveEntry(join(dir, it.name), it.name, folder = false, size = it.size.coerceAtLeast(0)) }
        val byName = compareBy(NATURAL, DriveEntry::name)
        return (folders.sortedWith(byName) + videos.sortedWith(byName)).take(MAX_ENTRIES)
    }

    /** Names by their runs of digits as numbers, the rest ignoring case. */
    val NATURAL: Comparator<String> = Comparator(::compareNatural)

    fun compareNatural(a: String, b: String): Int {
        var i = 0
        var j = 0
        while (i < a.length && j < b.length) {
            if (a[i].isDigit() && b[j].isDigit()) {
                val si = i
                val sj = j
                while (i < a.length && a[i].isDigit()) i++
                while (j < b.length && b[j].isDigit()) j++
                val x = a.substring(si, i).trimStart('0')
                val y = b.substring(sj, j).trimStart('0')
                if (x.length != y.length) return x.length.compareTo(y.length)
                val c = x.compareTo(y)
                if (c != 0) return c
                continue
            }
            val d = a[i].uppercaseChar().compareTo(b[j].uppercaseChar())
            if (d != 0) return d
            i++
            j++
        }
        return (a.length - i).compareTo(b.length - j)
    }

    /** The title a video shows: its file name without the extension. */
    fun title(path: String): String = path.substringAfterLast('/').substringBeforeLast('.')

    /** A video's kind, as its line says it: "MKV". */
    fun kind(name: String): String = extensionOf(name).uppercase()

    /**
     * The subtitle files beside [video] among [names] (its folder's files):
     * those named as it is, then a language ("en", "eng", "English"),
     * "forced", "sdh", in any order. SubRip and WebVTT, which the app draws.
     */
    fun sidecars(video: String, names: Iterable<String>): List<Sidecar> {
        val dir = video.substringBeforeLast('/', "")
        val stem = title(video)
        val found = mutableListOf<Sidecar>()
        for (name in names) {
            val ext = extensionOf(name)
            val codec = when (ext) {
                "srt" -> "subrip"
                "vtt" -> "webvtt"
                else -> continue
            }
            val end = name.length - ext.length - 1
            if (end < stem.length || !name.startsWith(stem, ignoreCase = true)) continue
            val middle = name.substring(stem.length, end)
            // "Film 2.srt" is Film 2's, not Film's.
            if (middle.isNotEmpty() && middle[0] != '.' && middle[0] != '_') continue
            var language: String? = null
            var forced = false
            val titles = mutableListOf<String>()
            for (part in middle.split('.', '_', ' ').filter { it.isNotEmpty() }) {
                val p = part.lowercase()
                val code = if (language == null) languageOf(p) else null
                when {
                    p == "forced" -> forced = true
                    p == "sdh" || p == "cc" || p == "hi" -> titles += "SDH"
                    code != null -> language = code
                    else -> titles += part
                }
            }
            found += Sidecar(join(dir, name), codec, language, titles.joinToString(" ").ifEmpty { null }, forced)
        }
        return found.sortedWith(compareBy(NATURAL, Sidecar::path))
    }

    /**
     * A SubRip file as WebVTT, as the server would serve it: the header, and
     * its times with a full stop. A WebVTT file is kept as it is.
     */
    fun toWebVtt(text: String): String {
        val lines = text.removePrefix("﻿").replace("\r\n", "\n").replace('\r', '\n')
        if (lines.trimStart().startsWith("WEBVTT")) return lines
        return "WEBVTT\n\n" + TIMES.replace(lines, "$1.$2")
    }

    /** A subtitle file's text: UTF-8 when it is, else Latin-1, close to the Western code page older SubRip files are in. */
    fun decode(bytes: ByteArray): String {
        val utf8 = Charsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
        return try {
            utf8.decode(ByteBuffer.wrap(bytes)).toString()
        } catch (_: CharacterCodingException) {
            String(bytes, Charsets.ISO_8859_1)
        }
    }

    private val TIMES = Regex("""(\d{1,2}:\d{2}:\d{2}),(\d{1,3})""")

    private val NAMES = mapOf(
        "english" to "eng", "french" to "fre", "spanish" to "spa", "german" to "ger", "italian" to "ita",
        "portuguese" to "por", "japanese" to "jpn", "korean" to "kor", "chinese" to "chi", "arabic" to "ara",
        "russian" to "rus", "dutch" to "dut", "swedish" to "swe", "hindi" to "hin", "turkish" to "tur",
    )

    private val CODE = Regex("^[a-z]{2,3}(-[a-z]{2})?$")

    /** A language code ("en", "eng", "pt-br") or English name, as the code the player's labels read; `null` for anything else. */
    private fun languageOf(part: String): String? = NAMES[part] ?: part.takeIf(CODE::matches)

    private fun extensionOf(name: String): String = name.substringAfterLast('.', "").lowercase()

    private fun join(dir: String, name: String): String = if (dir.endsWith('/')) dir + name else "$dir/$name"
}
