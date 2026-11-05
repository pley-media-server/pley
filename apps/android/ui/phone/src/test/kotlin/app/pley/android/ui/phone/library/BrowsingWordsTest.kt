package app.pley.android.ui.phone.library

import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.model.CollectionSummary
import app.pley.android.model.LibraryKind
import app.pley.android.screens.playlists.AddEvent
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.playlists.addedMessage
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The counts and toasts the phone's library pages and playlists word themselves, in English as they were, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class BrowsingWordsTest {
    private val app = ApplicationProvider.getApplicationContext<Context>()
    private val english = app.resources

    private fun inLanguage(tag: String): Resources =
        app.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) }).resources

    @Test
    fun englishIsAsItWas() {
        assertEquals("1,500 movies", count(english, R.plurals.library_count_movies, 1_500))
        assertEquals("1 show", count(english, R.plurals.library_count_shows, 1))
        assertEquals("23 of 1,500 movies", countOf(english, R.plurals.library_count_of_movies, 23, 1_500))
        assertEquals("Added 8 items to Road trip", addedMessage(english, AddEvent.Added("Road trip", 8)))
    }

    @Test
    fun frenchAndSpanishCount() {
        val fr = inLanguage("fr")
        val es = inLanguage("es-MX")
        // French says one of 0 and 1.
        assertEquals("0 film", count(fr, R.plurals.library_count_movies, 0))
        assertEquals("12 séries", count(fr, R.plurals.library_count_shows, 12))
        assertEquals("3 sur 12 artistes", countOf(fr, R.plurals.library_count_of_artists, 3, 12))
        assertEquals("1 colección", count(es, R.plurals.library_count_collections, 1))
        assertEquals("3 de 12 películas", countOf(es, R.plurals.library_count_of_movies, 3, 12))
        assertEquals("Ajouté à Road trip", addedMessage(fr, AddEvent.Added("Road trip", 1)))
        assertEquals("8 éléments ajoutés à Road trip", addedMessage(fr, AddEvent.Added("Road trip", 8)))
        assertEquals("Se añadieron 8 elementos a Road trip", addedMessage(es, AddEvent.Added("Road trip", 8)))
        assertEquals("No hay nada que añadir", addedMessage(es, AddEvent.Added("Road trip", 0)))
    }

    @Test
    fun aCollectionsTileIsInTheLanguageShownAsItIsMade() {
        var resources = english
        val tiles = PhoneTiles { resources }
        val collection = CollectionSummary(id = 7, libraryId = 1, title = "Heist films", itemCount = 12)
        assertEquals("12 movies", tiles.collection(collection, LibraryKind.Movies, "http://pley").subtitle)
        resources = inLanguage("es")
        assertEquals("12 películas", tiles.collection(collection, LibraryKind.Movies, "http://pley").subtitle)
        assertEquals("Heist films, 12 películas", tiles.collection(collection, LibraryKind.Movies, "http://pley").description)
    }
}
