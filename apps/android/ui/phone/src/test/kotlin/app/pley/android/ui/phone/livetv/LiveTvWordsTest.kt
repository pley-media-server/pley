package app.pley.android.ui.phone.livetv

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.common.resolve
import app.pley.android.model.LiveProgram
import app.pley.android.ui.phone.R
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

/** What Live TV says on phones, in English as it always was, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class LiveTvWordsTest {
    @get:Rule
    val compose = createComposeRule()

    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    private fun Context.plural(id: Int, count: Int): String = resources.getQuantityString(id, count, count)

    private val favoriteFailed = UiText.res(R.string.livetv_favorite_failed, errorSentence(SocketTimeoutException()))

    @Test
    fun englishIsAsItWas() {
        assertEquals("1 channel", english.plural(R.plurals.livetv_entry_channels, 1))
        assertEquals("1,234 channels", english.plural(R.plurals.livetv_entry_channels, 1234))
        assertEquals("1 min left", english.plural(R.plurals.livetv_minutes_left, 1))
        assertEquals("2 h left", english.plural(R.plurals.livetv_hours_left, 2))
        assertEquals("Its one connection is watching:", english.plural(R.plurals.live_player_connections_watching, 1))
        assertEquals("Its 3 connections are watching:", english.plural(R.plurals.live_player_connections_watching, 3))
        assertEquals("Someone else", english.plural(R.plurals.live_player_held_by_people, 1))
        assertEquals("The favourite wasn't saved: No answer in time.", favoriteFailed.resolve(english))
        assertEquals("S2 E14", english.getString(R.string.livetv_episode, 2, 14))
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        val fr = inLanguage("fr")
        val es = inLanguage("es-MX")
        assertEquals("1 chaîne", fr.plural(R.plurals.livetv_entry_channels, 1))
        assertEquals("Il reste 1 min", fr.plural(R.plurals.livetv_minutes_left, 1))
        assertEquals("1 connexion en cours :", fr.plural(R.plurals.live_player_connections_watching, 1))
        assertEquals("1 personne", fr.plural(R.plurals.live_player_held_by_people, 1))
        assertEquals("Le favori n’a pas été enregistré : Aucune réponse à temps.", favoriteFailed.resolve(fr))
        assertEquals("Résultats pour « news »", fr.getString(R.string.livetv_results_for, "news"))
        assertEquals("S2 É14", fr.getString(R.string.livetv_episode, 2, 14))

        assertEquals("2 canales", es.plural(R.plurals.livetv_entry_channels, 2))
        assertEquals("Queda 1 min", es.plural(R.plurals.livetv_minutes_left, 1))
        assertEquals("Quedan 5 min", es.plural(R.plurals.livetv_minutes_left, 5))
        assertEquals("Su única conexión está ocupada con:", es.plural(R.plurals.live_player_connections_watching, 1))
        assertEquals("4 personas", es.plural(R.plurals.live_player_held_by_people, 4))
        assertEquals("No se guardó el favorito: No hubo respuesta a tiempo.", favoriteFailed.resolve(es))
        assertEquals("T2 E14", es.getString(R.string.livetv_episode, 2, 14))
    }

    /** What the screens write themselves: a count, the time left, the episode, tomorrow and a day further on. */
    private class Words(val count: String, val left: String, val episode: String?, val tomorrow: String, val later: String)

    private fun words(): Words {
        val now = ZonedDateTime.of(2026, 6, 10, 12, 0, 0, 0, ZoneId.systemDefault())
        val program = LiveProgram(channelId = 1, start = now.toEpochSecond() - 600, stop = now.toEpochSecond() + 60, season = 2, episode = 14)
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
        assertEquals("1 min left", words.left)
        assertEquals("S2 E14", words.episode)
        assertEquals("Tomorrow", words.tomorrow)
        assertEquals("Jun 15, 2026", words.later)
    }

    @Test
    @Config(qualifiers = "fr")
    fun theScreensWriteInFrench() {
        val words = words()
        // A no-break space between the thousands, narrow or not as the locale data has it.
        assertTrue(words.count, Regex("12[  ]345").matches(words.count))
        assertEquals("Il reste 1 min", words.left)
        assertEquals("S2 É14", words.episode)
        assertEquals("Demain", words.tomorrow)
        assertEquals("15 juin 2026", words.later)
    }

    @Test
    @Config(qualifiers = "es")
    fun theScreensWriteInSpanish() {
        val words = words()
        assertEquals("Queda 1 min", words.left)
        assertEquals("T2 E14", words.episode)
        assertEquals("Mañana", words.tomorrow)
        assertTrue(words.later, words.later.startsWith("15 jun"))
    }
}
