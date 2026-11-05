package app.pley.android.ui.tv.livetv

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.common.resolve
import app.pley.android.model.LiveProgram
import app.pley.android.ui.tv.R
import java.net.SocketTimeoutException
import java.time.ZoneId
import java.time.ZonedDateTime
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** What Live TV says on TVs, in English as it always was, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class TvLiveTvWordsTest {
    @get:Rule
    val compose = createComposeRule()

    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    private fun Context.plural(id: Int, count: Int): String = resources.getQuantityString(id, count, count)

    private val favoriteFailed = UiText.res(R.string.tv_livetv_favorite_failed, errorSentence(SocketTimeoutException()))

    @Test
    fun englishIsAsItWas() {
        assertEquals("1 min left", english.plural(R.plurals.tv_livetv_minutes_left, 1))
        assertEquals("2 h left", english.plural(R.plurals.tv_livetv_hours_left, 2))
        assertEquals("Its one connection is watching:", english.plural(R.plurals.tv_live_player_connections_watching, 1))
        assertEquals("Its 3 connections are watching:", english.plural(R.plurals.tv_live_player_connections_watching, 3))
        assertEquals("Someone else", english.plural(R.plurals.tv_live_player_held_by_people, 1))
        assertEquals("2 people", english.plural(R.plurals.tv_live_player_held_by_people, 2))
        assertEquals("The favourite wasn't saved: No answer in time.", favoriteFailed.resolve(english))
        assertEquals("No channel 123", english.getString(R.string.tv_live_player_no_channel, "123"))
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        val fr = inLanguage("fr-CA")
        val es = inLanguage("es")
        assertEquals("Guide", fr.getString(R.string.tv_livetv_guide))
        assertEquals("Il reste 2 h", fr.plural(R.plurals.tv_livetv_hours_left, 2))
        assertEquals("3 connexions en cours :", fr.plural(R.plurals.tv_live_player_connections_watching, 3))
        assertEquals("2 personnes", fr.plural(R.plurals.tv_live_player_held_by_people, 2))
        assertEquals("Le favori n’a pas été enregistré : Aucune réponse à temps.", favoriteFailed.resolve(fr))
        assertEquals("Aucune chaîne 123", fr.getString(R.string.tv_live_player_no_channel, "123"))

        assertEquals("Guía", es.getString(R.string.tv_livetv_guide))
        assertEquals("Queda 1 h", es.plural(R.plurals.tv_livetv_hours_left, 1))
        assertEquals("Sus 3 conexiones están ocupadas con:", es.plural(R.plurals.tv_live_player_connections_watching, 3))
        assertEquals("Otra persona", es.plural(R.plurals.tv_live_player_held_by_people, 1))
        assertEquals("No se guardó el favorito: No hubo respuesta a tiempo.", favoriteFailed.resolve(es))
        assertEquals("No hay canal 123", es.getString(R.string.tv_live_player_no_channel, "123"))
    }

    /** What the screens write themselves: a count, the time left, the episode, tomorrow and a day further on. */
    private class Words(val count: String, val left: String, val episode: String?, val tomorrow: String, val later: String)

    private fun words(): Words {
        val now = ZonedDateTime.of(2026, 6, 10, 12, 0, 0, 0, ZoneId.systemDefault())
        val program = LiveProgram(channelId = 1, start = now.toEpochSecond() - 600, stop = now.toEpochSecond() + 3600, episode = 3)
        lateinit var words: Words
        compose.setContent {
            words = Words(
                formatCount(12345),
                timeLeft(program, now.toEpochSecond()),
                episodeOf(program),
                dayOf(now.plusDays(1).toEpochSecond(), now.toEpochSecond()),
                dayOf(now.plusDays(5).toEpochSecond(), now.toEpochSecond()),
            )
        }
        compose.waitForIdle()
        return words
    }

    @Test
    fun theScreensWriteInEnglish() {
        val words = words()
        assertEquals("12,345", words.count)
        assertEquals("1 h left", words.left)
        assertEquals("E3", words.episode)
        assertEquals("Tomorrow", words.tomorrow)
        assertEquals("Jun 15, 2026", words.later)
    }

    @Test
    @Config(qualifiers = "fr")
    fun theScreensWriteInFrench() {
        val words = words()
        // A no-break space between the thousands, narrow or not as the locale data has it.
        assertTrue(words.count, Regex("12[  ]345").matches(words.count))
        assertEquals("Il reste 1 h", words.left)
        assertEquals("É3", words.episode)
        assertEquals("Demain", words.tomorrow)
        assertEquals("15 juin 2026", words.later)
    }

    @Test
    @Config(qualifiers = "es")
    fun theScreensWriteInSpanish() {
        val words = words()
        assertEquals("Queda 1 h", words.left)
        assertEquals("E3", words.episode)
        assertEquals("Mañana", words.tomorrow)
        assertTrue(words.later, words.later.startsWith("15 jun"))
    }
}
