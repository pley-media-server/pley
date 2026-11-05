package app.pley.android.screens

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.resolve
import app.pley.android.model.CollectionSort
import app.pley.android.model.CreditRole
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.PersonCredit
import app.pley.android.model.PersonDetail
import app.pley.android.model.PersonRole
import app.pley.android.model.format.FormatWords
import app.pley.android.screens.collection.CollectionViewModel
import app.pley.android.screens.home.libraryCountText
import app.pley.android.screens.library.SmartFilter
import app.pley.android.screens.library.labelText
import app.pley.android.screens.music.ALBUM_DETAIL
import app.pley.android.screens.music.ARTIST_DETAIL
import app.pley.android.screens.music.albumPage
import app.pley.android.screens.music.artistPage
import app.pley.android.screens.music.track
import app.pley.android.screens.person.personPage
import app.pley.android.screens.person.roleWords
import app.pley.android.screens.playlists.AddToPlaylistViewModel
import app.pley.android.screens.playlists.Playlists
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * What browsing says in words made where nothing is drawn (the libraries'
 * counts, a smart filter, roles, music and playlist counts): in English,
 * as it always said; and in French and Spanish.
 */
@RunWith(RobolectricTestRunner::class)
class BrowsingWordsTest {
    private val app = ApplicationProvider.getApplicationContext<Context>()

    private fun UiText.inLanguage(tag: String): String =
        resolve(app.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) }).resources)

    private fun List<UiText>.inEnglish(): List<String?> = map { it.inEnglish() }

    private val filters = listOf(
        "type=1&genre=Comedy&unwatched=1&sort=year%3Adesc",
        "year%3E%3E=2019&sort=movie.addedAt:desc&includeGuids=1&limit=20",
        "label=Kids&studio=A24&contentRating=PG&inProgress=0&year%3C%3C=1990&decade=1990&sort=titleSort:desc",
        "unwatched=0&inProgress=1&year=2001&sort=viewCount",
        "resolution=1080&dynamicRange=dolby_vision&dynamicRange=hdr",
        "resolution=576&country=France&sort=originalTitle",
    )

    @Test
    fun englishIsAsItWas() {
        val counts = mapOf(
            LibraryKind.Movies to listOf("0 movies", "1 movie", "1,200 movies"),
            LibraryKind.Shows to listOf("0 shows", "1 show", "1,200 shows"),
            LibraryKind.Music to listOf("0 artists", "1 artist", "1,200 artists"),
            LibraryKind.Unknown to listOf("0 items", "1 item", "1,200 items"),
        )
        for ((kind, said) in counts) {
            assertEquals(said, listOf(0, 1, 1_200).map { libraryCountText(Library(1, "Library", kind, itemCount = it)).inEnglish() })
        }

        val described = listOf(
            listOf("Genre: Comedy", "Unwatched", "Newest first"),
            listOf("After 2019", "Recently added first", "At most 20"),
            listOf("Label: Kids", "Studio: A24", "Rated PG", "Not in progress", "Before 1990", "The 1990s", "By title, Z to A"),
            listOf("Watched", "In progress", "From 2001", "Least played first"),
            listOf("1080p", "Dolby Vision", "Any HDR"),
            listOf("resolution: 576", "country: France", "Sorted by originalTitle"),
        )
        assertEquals(described, filters.map { SmartFilter.words(it).inEnglish() })
        assertEquals(DynamicRangeFilter.entries.map { it.label }, DynamicRangeFilter.entries.map { it.labelText.inEnglish() })
        assertEquals(
            listOf("By release date", "By title", "Custom order", null),
            listOf(CollectionSort.Release, CollectionSort.Title, CollectionSort.Custom, CollectionSort.Unknown).map { CollectionViewModel.sortNoteText(it)?.inEnglish() },
        )

        val roles = listOf(
            listOf(PersonRole(CreditRole.Actor, "Neo")),
            listOf(PersonRole(CreditRole.Actor, episodes = 1)),
            listOf(PersonRole(CreditRole.Actor), PersonRole(CreditRole.Writer)),
            listOf(PersonRole(CreditRole.Director, episodes = 3), PersonRole(CreditRole.Unknown)),
            listOf(PersonRole(CreditRole.Creator), PersonRole(CreditRole.Producer, episodes = 1_200)),
        )
        assertEquals(
            listOf("as Neo", "Guest star (1 episode)", "Actor, Writer", "Director (3 episodes), Crew", "Creator, Producer (1,200 episodes)"),
            roles.map { roleWords(it).inEnglish().joinToString(", ") },
        )
        val person = personPage(PersonDetail(9, "Mara", credits = listOf(credit(1, ItemKind.Movie), credit(2, ItemKind.Show), credit(3, ItemKind.Show))), "")
        assertEquals("1 movie · 2 shows", person.countTexts.inEnglish().joinToString(" · "))

        val artist = artistPage(ARTIST_DETAIL, null, "")
        assertEquals("3 albums · 30 tracks", artist.countTexts.inEnglish().joinToString(" · "))
        val album = albumPage(ALBUM_DETAIL, (1..10L).map { track(it) }, "", FormatWords.English)
        assertEquals("10 tracks · 31m", album.metaTexts.inEnglish().joinToString(" · "))

        assertEquals(
            listOf("12 items · 3h 12m", "1 item", "0 items"),
            listOf(12 to 11_520_000L, 1 to 0L, 0 to 0L).map { (count, ms) -> Playlists.metaTexts(count, ms, FormatWords.English).inEnglish().joinToString(" · ") },
        )
        val three = (1..3L).map { track(it) }
        assertEquals("3 items", AddToPlaylistViewModel.titleOf(three, FormatWords.English).inEnglish())
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        // French says one of 0 and 1.
        assertEquals("0 film", libraryCountText(Library(1, "Library", LibraryKind.Movies)).inLanguage("fr"))
        assertEquals("38 series", libraryCountText(Library(2, "Library", LibraryKind.Shows, itemCount = 38)).inLanguage("es-MX"))

        val filter = "genre=Comedy&unwatched=1&decade=1990&limit=20&country=France&sort=year:desc"
        assertEquals(
            listOf("Genre : Comedy", "Non vu", "Années 1990", "20 au maximum", "country : France", "Les plus récents d’abord"),
            SmartFilter.words(filter).map { it.inLanguage("fr") },
        )
        assertEquals(
            listOf("Género: Comedy", "No visto", "Años 1990", "20 como máximo", "country: France", "Más recientes primero"),
            SmartFilter.words(filter).map { it.inLanguage("es") },
        )
        assertEquals("Ordenado por originalTitle", SmartFilter.words("sort=originalTitle").single().inLanguage("es"))
        assertEquals("Tout HDR", DynamicRangeFilter.Hdr.labelText.inLanguage("fr-CA"))
        assertEquals("Dolby Vision", DynamicRangeFilter.DolbyVision.labelText.inLanguage("es"))
        assertEquals("Por fecha de estreno", CollectionViewModel.sortNoteText(CollectionSort.Release)?.inLanguage("es"))

        assertEquals(listOf("dans le rôle de Neo"), roleWords(listOf(PersonRole(CreditRole.Actor, "Neo"))).map { it.inLanguage("fr") })
        assertEquals(listOf("Apparition (1 épisode)"), roleWords(listOf(PersonRole(CreditRole.Actor, episodes = 1))).map { it.inLanguage("fr") })
        assertEquals(listOf("Dirección (3 episodios)"), roleWords(listOf(PersonRole(CreditRole.Director, episodes = 3))).map { it.inLanguage("es") })

        val person = personPage(PersonDetail(9, "Mara", credits = listOf(credit(1, ItemKind.Movie), credit(2, ItemKind.Show), credit(3, ItemKind.Show))), "")
        assertEquals(listOf("1 película", "2 series"), person.countTexts.map { it.inLanguage("es") })
        assertEquals(listOf("3 albums", "30 titres"), artistPage(ARTIST_DETAIL, null, "").countTexts.map { it.inLanguage("fr") })
        assertEquals("1 pista", albumPage(ALBUM_DETAIL, listOf(track(1)), "").metaTexts.first().inLanguage("es"))
        assertEquals("0 élément", Playlists.metaTexts(0, 0, FormatWords.English).single().inLanguage("fr"))
        assertEquals("3 elementos", AddToPlaylistViewModel.titleOf((1..3L).map { track(it) }, FormatWords.English).inLanguage("es"))
    }

    private fun credit(id: Long, kind: ItemKind) = PersonCredit(ItemSummary(id, 1, kind, "Title $id", year = 2019), listOf(PersonRole(CreditRole.Actor, "Mara")))
}
