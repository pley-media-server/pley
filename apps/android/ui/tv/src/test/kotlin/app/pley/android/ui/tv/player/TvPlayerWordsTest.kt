package app.pley.android.ui.tv.player

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import android.view.Display
import androidx.test.core.app.ApplicationProvider
import app.pley.android.player.engine.PlaybackStats
import app.pley.android.ui.tv.R
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The TV player's words with numbers in them, in English as they were and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class TvPlayerWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    @Test
    fun `a screen reader hears a time in the language shown`() {
        val r = english.resources
        assertEquals("32 minutes 40 seconds", TvPlayerText.spoken(r, (32 * 60 + 40) * 1000L))
        assertEquals("1 hour 2 minutes", TvPlayerText.spoken(r, (62 * 60) * 1000L))
        assertEquals("2 hours 1 minute 1 second", TvPlayerText.spoken(r, (2 * 3600 + 61) * 1000L))
        assertEquals("0 seconds", TvPlayerText.spoken(r, 0))

        val fr = inLanguage("fr").resources
        assertEquals("32 minutes 40 secondes", TvPlayerText.spoken(fr, (32 * 60 + 40) * 1000L))
        assertEquals("1 heure 2 minutes", TvPlayerText.spoken(fr, (62 * 60) * 1000L))
        // French says one for 0 and 1.
        assertEquals("0 seconde", TvPlayerText.spoken(fr, 0))

        val es = inLanguage("es-419").resources
        assertEquals("2 horas 1 minuto 1 segundo", TvPlayerText.spoken(es, (2 * 3600 + 61) * 1000L))

        assertEquals(
            "Chapitre 3, à partir de 1 heure 2 minutes",
            fr.getString(R.string.tv_player_chapter_card, fr.getString(R.string.tv_player_chapter, 3), TvPlayerText.spoken(fr, (62 * 60) * 1000L)),
        )
        assertEquals("1:02:00 de 2:00:00", es.getString(R.string.tv_player_position, TvPlayerText.time(62 * 60_000L), TvPlayerText.time(7_200_000L)))
    }

    @Test
    fun `the readout writes the picture's numbers as the language shown does`() {
        val stats = PlaybackStats(videoDecoder = "c2.amlogic.hevc.decoder", videoDecoderHardware = true, width = 3840, height = 2160, bitDepth = 10, dynamicRange = "HDR10")
        val display = DisplayNow(3840, 2160, 23.976f, listOf(Display.HdrCapabilities.HDR_TYPE_HDR10))
        fun device(r: android.content.res.Resources) = TvReadout.sections(r, ReadoutInput(stats = stats, display = display)).single().facts

        val en = device(english.resources).associate { it.label to it.value }
        assertEquals("3840×2160 · 10-bit · HDR10", en["Decoded"])
        assertEquals("3840×2160 at 23.976 Hz · Unchanged", en["Display"])

        val fr = inLanguage("fr").resources
        val facts = device(fr).associate { it.label to it.value }
        assertTrue(facts.toString(), facts.getValue(fr.getString(R.string.tv_readout_decoded)).contains("10 bits"))
        assertTrue(facts.toString(), facts.getValue("Écran").contains("23,976") && facts.getValue("Écran").endsWith("Inchangé"))
        assertEquals("c2.amlogic.hevc.decoder (matériel)", facts["Décodeur vidéo"])
    }
}
