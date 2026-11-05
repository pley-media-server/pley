package app.pley.android.screens.item

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.UiText
import app.pley.android.common.resolve
import app.pley.android.model.ExtraType
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.screens.R
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** What an item page says itself, in English as it always was, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class ItemWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    private val streams = arrayOf(
        MediaStream(0, StreamKind.Video, "hevc", width = 3840, height = 2160, frameRate = 23.976, bitDepth = 10, profile = "Main 10"),
        MediaStream(1, StreamKind.Audio, "truehd", language = "eng", channels = 8, sampleRate = 48_000, default = true),
        MediaStream(2, StreamKind.Subtitle, "hdmv_pgs_subtitle", language = "eng", textBased = false, forced = true),
    )

    /** A movie in two versions, its page written in [locale]. */
    private fun page(locale: Locale): ItemPage =
        itemPage(PageInput(detail = movie(files = listOf(file(11, *streams), file(12, *streams))), origin = ORIGIN, locale = locale))

    /** The first version's streams' details, as Media info shows them in [context]'s language. */
    private fun details(page: ItemPage, context: Context): List<String> =
        page.mediaInfo.first().streams.map { stream -> stream.details.joinToString(" · ") { it.resolve(context) } }

    private fun extra(type: ExtraType) = summary(5, ItemKind.Extra, durationMs = null).copy(extraType = type)

    private fun episodes(count: Int): UiText = UiText.plural(R.plurals.item_episode_count, count)

    @Test
    fun englishIsAsItWas() {
        val page = page(Locale.US)
        assertEquals(listOf("4K · 3840×2160 · 23.98 fps · 10-bit · Main 10", "7.1 · 48.0 kHz · Default", "Image-based · Forced"), details(page, english))
        assertEquals(listOf("Version 1", "Version 2"), page.mediaInfo.map { it.title?.resolve(english) })
        assertEquals("Behind the Scenes", extraSubtitle(extra(ExtraType.BehindTheScenes)).resolve(english))
        assertEquals("1 season", UiText.plural(R.plurals.item_season_count, 1).resolve(english))
        assertEquals("1,234 episodes", episodes(1_234).resolve(english))
        val season = ItemDetail(id = 30, libraryId = 2, kind = ItemKind.Season, title = "Season 1", index = 1, year = 2024, leafCount = 8)
        assertEquals(listOf("2024", "8 episodes"), itemPage(PageInput(detail = season, origin = ORIGIN)).meta.map { (it as MetaPart.Text).text.resolve(english) })
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        val french = inLanguage("fr")
        assertEquals(listOf("4K · 3840×2160 · 23,98 i/s · 10 bits · Main 10", "7.1 · 48,0 kHz · Par défaut", "Image · Forcés"), details(page(Locale.FRANCE), french))
        assertEquals("Version 2", page(Locale.FRANCE).mediaInfo[1].title?.resolve(french))
        assertEquals("Coulisses", extraSubtitle(extra(ExtraType.BehindTheScenes)).resolve(french))
        assertEquals("Bonus", extraSubtitle(extra(ExtraType.Unknown)).resolve(french))
        // French puts 0 and 1 in the singular.
        assertEquals("0 épisode", episodes(0).resolve(french))
        assertEquals("3 saisons", UiText.plural(R.plurals.item_season_count, 3).resolve(french))

        val spanish = inLanguage("es-MX")
        assertEquals(listOf("4K · 3840×2160 · 23,98 fps · 10 bits · Main 10", "7.1 · 48,0 kHz · Por defecto", "Imagen · Forzados"), details(page(Locale.forLanguageTag("es")), inLanguage("es")))
        assertEquals("Versión 2", page(Locale.forLanguageTag("es")).mediaInfo[1].title?.resolve(spanish))
        assertEquals("Tráiler", extraSubtitle(extra(ExtraType.Trailer)).resolve(spanish))
        assertEquals("1 temporada", UiText.plural(R.plurals.item_season_count, 1).resolve(spanish))
        assertEquals("8 episodios", episodes(8).resolve(spanish))
    }
}
