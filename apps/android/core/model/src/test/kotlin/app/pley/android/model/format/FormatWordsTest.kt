package app.pley.android.model.format

import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.model.TranscodeInfo
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test

/** What the format functions take from their words besides the words: the locale's names of languages and its decimals. */
class FormatWordsTest {
    /** English words with French's locale and decimals. */
    private val frenchLocale = object : FormatWords by FormatWords.English {
        override val locale: Locale = Locale.FRENCH
        override val decimalSeparator: Char = ','
    }

    @Test
    fun languagesAreNamedInTheWordsLocale() {
        assertEquals("Allemand", languageName("ger", frenchLocale))
        assertEquals("Allemand", languageName("deu", frenchLocale))
        assertEquals("Chinois (TW)", languageName("zh-TW", frenchLocale))
        // The codes Pley names itself are the words'.
        assertEquals("Unknown", languageName("und", frenchLocale))
        assertEquals("Mandarin", languageName("cmn", frenchLocale))
        // An unknown code, as in English.
        assertEquals("XYZ", languageName("xyz", frenchLocale))
        // English words keep the table's names, whatever the device's language.
        assertEquals("Norwegian Bokmål", languageName("nob"))
    }

    @Test
    fun quantitiesTakeTheWordsDecimalSeparator() {
        assertEquals("1,5 KB", formatBytes(1536, frenchLocale))
        assertEquals("150 MB", formatBytes(150L * 1024 * 1024, frenchLocale))
        assertEquals("8,2 Mbps", formatBitrate(8_200_000, frenchLocale))
        assertEquals("23,976 fps", frameRateLabel(23.976, frenchLocale))
        assertEquals("44,1 kHz", sampleRateLabel(44_100, frenchLocale))
        assertEquals("3,2×", transcodeSummary(TranscodeInfo(jobId = "j", speed = 3.2), frenchLocale))
        // A level is a name, written as the standard does.
        assertEquals("Main 10 · Level 5.1", profileLevelLabel(MediaStream(index = 0, kind = StreamKind.Video, codec = "hevc", profile = "Main 10", level = 153), frenchLocale))
    }
}
