package app.pley.android.ui.tv.library

import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.screens.playlists.AddEvent
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.playlists.addedMessage
import java.text.NumberFormat
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The TV's browsing words with counts and arguments: in English as they
 * always were, and in French and Spanish, each count with its own plural
 * and in the language's digits.
 */
@RunWith(RobolectricTestRunner::class)
class TvBrowsingWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>().resources

    private fun inLanguage(tag: String): Resources = ApplicationProvider.getApplicationContext<Context>()
        .createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })
        .resources

    @Test
    fun englishIsAsItWas() {
        assertEquals("1 movie", tvCount(english, R.plurals.tv_library_count_movies, 1))
        assertEquals("1,500 movies", tvCount(english, R.plurals.tv_library_count_movies, 1500))
        assertEquals("23 of 1,500 shows", tvCountOf(english, R.plurals.tv_library_count_of_shows, 23, 1500))
        assertEquals("2 collections", tvCount(english, R.plurals.tv_library_count_collections, 2))
        assertEquals("Jump to a letter: K", english.getString(R.string.tv_library_jump, "K"))
        assertEquals("No results for “north”", english.getString(R.string.tv_search_none_title, "north"))
        assertEquals("437 GB free of 1.0 TB", english.getString(R.string.tv_drive_space, "437 GB", "1.0 TB"))
        assertEquals("Sleep timer: 30 min", english.getString(R.string.tv_music_sleep_running, "Sleep timer", "30 min"))
        assertEquals("Added 8 items to Road trip", addedMessage(english, AddEvent.Added("Road trip", 8)))
        assertEquals("Added to Road trip", addedMessage(english, AddEvent.Added("Road trip", 1)))
    }

    @Test
    fun frenchCountsAndArguments() {
        val fr = inLanguage("fr")
        // French says the singular for 0 and 1, and "de" from a million.
        assertEquals("0 film", tvCount(fr, R.plurals.tv_library_count_movies, 0))
        assertEquals("1 film", tvCount(fr, R.plurals.tv_library_count_movies, 1))
        assertEquals("2 films", tvCount(fr, R.plurals.tv_library_count_movies, 2))
        val million = NumberFormat.getIntegerInstance(Locale.FRENCH).format(1_000_000)
        assertEquals("$million d’artistes", tvCount(fr, R.plurals.tv_library_count_artists, 1_000_000))
        assertEquals("3 sur 40 séries", tvCountOf(fr, R.plurals.tv_library_count_of_shows, 3, 40))
        assertEquals("Aucun résultat pour « nord »", fr.getString(R.string.tv_search_none_title, "nord"))
        assertEquals("Retiré de la playlist : Vacances", fr.getString(R.string.tv_playlist_removed, "Vacances"))
        assertEquals("8 éléments ajoutés à Vacances", addedMessage(fr, AddEvent.Added("Vacances", 8)))
    }

    @Test
    fun spanishCountsAndArguments() {
        val es = inLanguage("es-MX")
        assertEquals("1 película", tvCount(es, R.plurals.tv_library_count_movies, 1))
        assertEquals("0 colecciones", tvCount(es, R.plurals.tv_library_count_collections, 0))
        assertEquals("23 de 1,500 artistas", tvCountOf(es, R.plurals.tv_library_count_of_artists, 23, 1500))
        assertEquals("437 GB libres de 1.0 TB", es.getString(R.string.tv_drive_space, "437 GB", "1.0 TB"))
        assertEquals("No hay resultados para «norte»", es.getString(R.string.tv_search_none_title, "norte"))
        assertEquals("Se añadieron 8 elementos a Viaje", addedMessage(es, AddEvent.Added("Viaje", 8)))
    }
}
