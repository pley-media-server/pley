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
        val lines = text.removePrefix("\uFEFF").replace("\r\n", "\n").replace('\r', '\n')
        if (lines.trimStart().startsWith("WEBVTT")) return lines
        return "WEBVTT\n\n" + TIMES.replace(lines, "$1.$2")
    }

    /**
     * A subtitle file's text, as on LG TVs. A byte-order mark says its
     * encoding: UTF-16 or UTF-32, as Windows tools save "Unicode" text
     * (UTF-8's goes in [toWebVtt]). Without one, UTF-16 shows by its NUL
     * bytes, which text in UTF-8 or a code page never has; then UTF-8 when it
     * is, else Windows-1252, the Western code page older SubRip files are in.
     */
    fun decode(bytes: ByteArray): String {
        fun at(i: Int) = bytes.getOrNull(i)?.toInt()?.and(0xFF)
        if (at(0) == 0xFF && at(1) == 0xFE && at(2) == 0 && at(3) == 0) return utf32(bytes, 4, littleEndian = true)
        if (at(0) == 0 && at(1) == 0 && at(2) == 0xFE && at(3) == 0xFF) return utf32(bytes, 4, littleEndian = false)
        if (at(0) == 0xFF && at(1) == 0xFE) return utf16(bytes, 2, littleEndian = true)
        if (at(0) == 0xFE && at(1) == 0xFF) return utf16(bytes, 2, littleEndian = false)
        utf16Order(bytes)?.let { return utf16(bytes, 0, littleEndian = it) }
        val utf8 = Charsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
        return try {
            utf8.decode(ByteBuffer.wrap(bytes)).toString()
        } catch (_: CharacterCodingException) {
            String(CharArray(bytes.size) { i -> windows1252(bytes[i].toInt() and 0xFF) })
        }
    }

    /**
     * Whether UTF-16 without its mark is little-endian, from its first 4 KB:
     * where it is Latin text, times or numbers, every other byte is NUL, the
     * high one. `null` when it isn't UTF-16.
     */
    private fun utf16Order(bytes: ByteArray): Boolean? {
        val pairs = minOf(bytes.size, 4096) / 2
        var first = 0
        var second = 0
        for (i in 0 until pairs) {
            if (bytes[2 * i] == 0.toByte()) first++
            if (bytes[2 * i + 1] == 0.toByte()) second++
        }
        fun many(n: Int) = pairs >= 8 && n * 4 >= pairs
        return when {
            many(second) && first * 8 <= second -> true
            many(first) && second * 8 <= first -> false
            else -> null
        }
    }

    /** Windows-1252's byte [b]: Latin-1 but for 0x80–0x9F, where it has the euro, curly quotes, dashes and the like. */
    private fun windows1252(b: Int): Char = if (b in 0x80..0x9F) WINDOWS_1252_HIGH[b - 0x80] else b.toChar()

    /** Windows-1252's 0x80–0x9F (the five it leaves out stay as Latin-1 has them). */
    private const val WINDOWS_1252_HIGH =
        "€\u0081‚ƒ„…†‡ˆ‰Š‹Œ\u008DŽ\u008F" +
            "\u0090‘’“”•–—˜™š›œ\u009DžŸ"

    /** UTF-32 text from [from]; what isn't a character reads as U+FFFD, and a last word cut short is dropped. */
    private fun utf32(bytes: ByteArray, from: Int, littleEndian: Boolean): String {
        val text = StringBuilder()
        for (i in 0 until (bytes.size - from) / 4) {
            val b = IntArray(4) { bytes[from + 4 * i + it].toInt() and 0xFF }
            val point = if (littleEndian) {
                b[0] or (b[1] shl 8) or (b[2] shl 16) or (b[3] shl 24)
            } else {
                (b[0] shl 24) or (b[1] shl 16) or (b[2] shl 8) or b[3]
            }
            if (point in 0..0x10FFFF && point !in 0xD800..0xDFFF) text.appendCodePoint(point) else text.append('�')
        }
        return text.toString()
    }

    /** UTF-16 text from [from]; a lone surrogate reads as U+FFFD and an odd last byte is dropped, as on LG TVs. */
    private fun utf16(bytes: ByteArray, from: Int, littleEndian: Boolean): String {
        val units = CharArray((bytes.size - from) / 2) { i ->
            val a = bytes[from + 2 * i].toInt() and 0xFF
            val b = bytes[from + 1 + 2 * i].toInt() and 0xFF
            (if (littleEndian) a or (b shl 8) else (a shl 8) or b).toChar()
        }
        val text = StringBuilder(units.size)
        var i = 0
        while (i < units.size) {
            val unit = units[i]
            if (unit.isHighSurrogate() && units.getOrNull(i + 1)?.isLowSurrogate() == true) {
                text.append(unit).append(units[i + 1])
                i += 2
                continue
            }
            text.append(if (unit.isSurrogate()) '\uFFFD' else unit)
            i++
        }
        return text.toString()
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
