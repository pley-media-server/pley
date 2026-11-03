package app.pley.android.data.drive

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A drive's folders and the subtitle files beside its videos: the Samsung app's LocalTests. */
class DriveFolderTest {
    @Test
    fun namesSortAsAPersonSortsThem() {
        val names = listOf("Part 10.mkv", "part 2.mkv", "Part 1.mkv", "Alpha.mkv", "Part 02b.mkv")
        assertEquals(listOf("Alpha.mkv", "Part 1.mkv", "part 2.mkv", "Part 02b.mkv", "Part 10.mkv"), names.sortedWith(DriveFolder.NATURAL))
    }

    @Test
    fun hiddenAndSystemEntriesAreLeftOut() {
        for (name in listOf("\$RECYCLE.BIN", ".device_info_8cdc30de", "System Volume Information", "._Film.mkv", ".Trashes", "lost+found", "Thumbs.db", "")) {
            assertFalse(name, DriveFolder.shown(name))
        }
        assertTrue(DriveFolder.shown("Ad Astra"))
    }

    @Test
    fun aFolderListsItsFoldersThenItsVideos() {
        val files = listOf(
            DriveFile("Season 10", folder = true),
            DriveFile("Season 2", folder = true),
            DriveFile("\$RECYCLE.BIN", folder = true),
            DriveFile("Film.mkv", folder = false, size = 4_000),
            DriveFile("Film.en.srt", folder = false, size = 30),
            DriveFile("notes.txt", folder = false),
            DriveFile(".hidden.mkv", folder = false),
            DriveFile("Clip.MP4", folder = false, size = 900),
            DriveFile("Camera.m2ts", folder = false),
        )
        val listed = DriveFolder.entries("/storage/1A2B-3C4D/Films", files)
        // Only what plays: subtitle files, notes and hidden files aren't shown.
        assertEquals(listOf("Season 2", "Season 10", "Clip.MP4", "Film.mkv"), listed.map { it.name })
        assertEquals(listOf(true, true, false, false), listed.map { it.folder })
        assertEquals("/storage/1A2B-3C4D/Films/Season 2", listed[0].path)
        assertEquals(listOf(0L, 0L, 900L, 4_000L), listed.map { it.size })
        assertEquals("/storage/1A2B-3C4D/Clip.MP4", DriveFolder.entries("/storage/1A2B-3C4D/", files).first { !it.folder }.path)
    }

    @Test
    fun aVideoShowsItsNameWithoutItsExtension() {
        assertEquals("Ad Astra (2019)", DriveFolder.title("/storage/1A2B-3C4D/Ad Astra (2019).mkv"))
        assertEquals("Film.Final.Cut", DriveFolder.title("/storage/1A2B-3C4D/Film.Final.Cut.mp4"))
        assertEquals("MKV", DriveFolder.kind("Film.mkv"))
    }

    @Test
    fun subtitleFilesBesideAVideoSayTheirLanguageAndKind() {
        val found = DriveFolder.sidecars(
            "/media/Ad Astra.mkv",
            listOf("Ad Astra.mkv", "Ad Astra.srt", "Ad Astra.en.forced.srt", "Ad Astra.English.SDH.srt", "Ad Astra.fr.vtt", "Ad Astra 2.srt", "Other.srt", "Ad Astra.ass"),
        )
        assertEquals(
            listOf("Ad Astra.en.forced.srt", "Ad Astra.English.SDH.srt", "Ad Astra.fr.vtt", "Ad Astra.srt"),
            found.map { it.path.substringAfterLast('/') },
        )
        assertEquals("/media/Ad Astra.fr.vtt", found[2].path)
        assertEquals(listOf("en", "eng", "fr", null), found.map { it.language })
        assertEquals(listOf(true, false, false, false), found.map { it.forced })
        assertEquals("SDH", found[1].title)
        assertEquals(listOf("subrip", "subrip", "webvtt", "subrip"), found.map { it.codec })
        // A video whose name ends like a subtitle file's has none of its own.
        assertTrue(DriveFolder.sidecars("/media/Film.srt.mkv", listOf("Film.srt")).isEmpty())
    }

    @Test
    fun subRipBecomesWebVttTheAppDraws() {
        val vtt = DriveFolder.toWebVtt("\uFEFF1\r\n00:00:01,500 --> 00:00:03,000\r\nHello\r\n\r\n2\r\n00:01:02,250 --> 00:01:04,000\r\nThere\r\n")
        assertTrue(vtt.startsWith("WEBVTT\n\n"))
        assertTrue(vtt.contains("00:00:01.500 --> 00:00:03.000\nHello\n"))
        assertTrue(vtt.contains("00:01:02.250 --> 00:01:04.000\nThere\n"))
        assertFalse(vtt.contains('\r'))
        // WebVTT stays as it is.
        assertEquals("WEBVTT\n\n00:01.000 --> 00:02.000\nHi\n", DriveFolder.toWebVtt("WEBVTT\n\n00:01.000 --> 00:02.000\nHi\n"))
    }

    @Test
    fun aSubtitleFileNotInUtf8IsReadAsWindows1252() {
        assertEquals("café", DriveFolder.decode(byteArrayOf(0x63, 0x61, 0x66, 0xe9.toByte())))
        assertEquals("café", DriveFolder.decode("café".toByteArray(Charsets.UTF_8)))
        // Its curly quotes, dash and euro, which Latin-1 would make control characters.
        val quoted = byteArrayOf(0x93.toByte(), 0x48, 0x69, 0x94.toByte(), 0x20, 0x96.toByte(), 0x20, 0x35, 0x80.toByte())
        assertEquals("“Hi” – 5€", DriveFolder.decode(quoted))
    }

    @Test
    fun aSubtitleFileInUtf16WithoutItsMarkIsReadByItsNulBytes() {
        val text = "1\n00:00:01,000 --> 00:00:02,000\nCafé 日本\n"
        assertEquals(text, DriveFolder.decode(text.toByteArray(Charsets.UTF_16LE)))
        assertEquals(text, DriveFolder.decode(text.toByteArray(Charsets.UTF_16BE)))
        // Text in a single-byte encoding has no NULs: never taken for UTF-16.
        val plain = "1\n00:00:01,000 --> 00:00:02,000\nHello\n"
        assertEquals(plain, DriveFolder.decode(plain.toByteArray(Charsets.UTF_8)))
    }

    @Test
    fun aSubtitleFileInUtf32IsReadByItsByteOrderMark() {
        val points = "Café".codePoints().toArray()
        val le = byteArrayOf(0xFF.toByte(), 0xFE.toByte(), 0, 0) + points.flatMap { p -> List(4) { (p shr (8 * it)).toByte() } }
        val be = byteArrayOf(0, 0, 0xFE.toByte(), 0xFF.toByte()) + points.flatMap { p -> List(4) { (p shr (24 - 8 * it)).toByte() } }
        assertEquals("Café", DriveFolder.decode(le))
        assertEquals("Café", DriveFolder.decode(be))
    }

    @Test
    fun aSubtitleFileInUtf16IsReadByItsByteOrderMark() {
        val text = "Café 日本 \uD83C\uDFAC"
        val le = byteArrayOf(0xFF.toByte(), 0xFE.toByte()) + text.toByteArray(Charsets.UTF_16LE)
        val be = byteArrayOf(0xFE.toByte(), 0xFF.toByte()) + text.toByteArray(Charsets.UTF_16BE)
        assertEquals(text, DriveFolder.decode(le))
        assertEquals(text, DriveFolder.decode(be))
        // A lone surrogate and an odd last byte don't stop the rest.
        assertEquals("\uFFFDA", DriveFolder.decode(byteArrayOf(0xFF.toByte(), 0xFE.toByte(), 0x00, 0xD8.toByte(), 0x41, 0x00, 0x42)))
    }
}
