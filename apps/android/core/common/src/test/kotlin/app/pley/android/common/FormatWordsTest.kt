package app.pley.android.common

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.model.DynamicRange
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaStream
import app.pley.android.model.MediaSummary
import app.pley.android.model.StreamAction
import app.pley.android.model.StreamKind
import app.pley.android.model.TranscodeInfo
import app.pley.android.model.UserState
import app.pley.android.model.format.Counted
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.RelativeUnit
import app.pley.android.model.format.cardSubtitle
import app.pley.android.model.format.dolbyVisionDetails
import app.pley.android.model.format.dynamicRangeFilterLabel
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.episodeLongLabel
import app.pley.android.model.format.formatBitrate
import app.pley.android.model.format.formatBytes
import app.pley.android.model.format.formatRelative
import app.pley.android.model.format.formatRuntime
import app.pley.android.model.format.formatTimeLeft
import app.pley.android.model.format.formatUptime
import app.pley.android.model.format.frameRateLabel
import app.pley.android.model.format.languageName
import app.pley.android.model.format.mediaLine
import app.pley.android.model.format.seasonTitle
import app.pley.android.model.format.spokenDuration
import app.pley.android.model.format.streamLabel
import app.pley.android.model.format.tileDescription
import app.pley.android.model.format.transcodeSummary
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The format functions' words from the app's resources: English as FormatWords.English writes it, and French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class FormatWordsTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()

    private fun wordsIn(tag: String): FormatWords = formatWords(
        context.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) }).resources,
    )

    private val english = wordsIn("en-US")
    private val french = wordsIn("fr")
    private val spanish = wordsIn("es")

    private val counts = listOf(0, 1, 2, 5, 21, 1_000_000)

    @Test
    fun englishIsWhatTheFormatFunctionsAlwaysWrote() {
        val plain = FormatWords.English
        for (n in counts.map(Int::toLong)) {
            for (f in listOf<FormatWords.(Long) -> String>(
                { seconds(it) }, { minutes(it) }, { hours(it) }, { hoursMinutes(it, 7) }, { daysHours(it, 3) },
                { runtimeMinutes(it) }, { spokenSeconds(it) }, { spokenMinutes(it) }, { spokenHours(it) },
            )) {
                assertEquals(plain.f(n), english.f(n))
            }
            for (unit in RelativeUnit.entries) {
                assertEquals(plain.ago(n, unit), english.ago(n, unit))
                assertEquals(plain.later(n, unit), english.later(n, unit))
            }
        }
        for (unit in RelativeUnit.entries) {
            assertEquals(plain.thisTime(unit), english.thisTime(unit))
            assertEquals(plain.lastTime(unit), english.lastTime(unit))
            assertEquals(plain.nextTime(unit), english.nextTime(unit))
        }
        for (n in counts) {
            assertEquals(plain.seasonEpisode(n, n + 1), english.seasonEpisode(n, n + 1))
            assertEquals(plain.seasonShort(n), english.seasonShort(n))
            assertEquals(plain.episodeShort(n), english.episodeShort(n))
            assertEquals(plain.season(n), english.season(n))
            assertEquals(plain.episode(n), english.episode(n))
            for (what in Counted.entries) assertEquals(plain.count(what, n, "$n"), english.count(what, n, "$n"))
            assertEquals(plain.unwatched(n), english.unwatched(n))
            assertEquals(plain.channels(n), english.channels(n))
            assertEquals(plain.bitDepth(n), english.bitDepth(n))
            assertEquals(plain.version(n), english.version(n))
        }
        for (power in 0..4) assertEquals(plain.size("1.5", power), english.size("1.5", power))
        for (f in listOf<FormatWords.(String) -> String>(
            { left(it) }, { spokenHoursMinutes(it, "x") }, { spokenLeft(it) }, { megabits(it) }, { kilobits(it) }, { copied(it) },
            { decoded(it) }, { level(it) }, { frameRate(it) }, { dvProfile(it) }, { dvBaseLayer(it) },
        )) {
            assertEquals(plain.f("5.1"), english.f("5.1"))
        }
        for (f in listOf<FormatWords.() -> String>(
            { never }, { seasonUnnumbered }, { specials }, { watched }, { unknown }, { stereo }, { forced }, { external },
            { bitrateUnknown }, { anyHdr }, { videoTranscode }, { video }, { burnIn }, { dvNoBaseLayer }, { dvFullLayer },
            { dvMinimalLayer },
        )) {
            assertEquals(plain.f(), english.f())
        }
        for (code in listOf("und", "mul", "zxx", "qaa", "cmn", "eng")) assertEquals(plain.ownLanguageName(code), english.ownLanguageName(code))
        assertEquals('.', english.decimalSeparator)
    }

    @Test
    fun timesInFrenchAndSpanish() {
        assertEquals("58 min", formatRuntime(3_480_000, french))
        assertEquals("1 h 52 min", formatRuntime(6_720_000, french))
        assertEquals("Il reste 26 min", formatTimeLeft(1_920_000, 3_480_000, french))
        assertEquals("Quedan 1 h 12 min", formatTimeLeft(3_000_000, 7_320_000, spanish))
        assertEquals("1 heure 12 minutes", spokenDuration(4_320_000, french))
        assertEquals("1 hora y 12 minutos", spokenDuration(4_320_000, spanish))
        assertEquals("1 j 1 h", formatUptime(90_000, french))

        val now = 1_700_000_000L
        assertEquals("il y a 3 minutes", formatRelative(now - 180, now, french))
        assertEquals("hace 3 minutos", formatRelative(now - 180, now, spanish))
        assertEquals("hier", formatRelative(now - 86_400, now, french))
        assertEquals("la semana pasada", formatRelative(now - 86_400 * 8, now, spanish))
        assertEquals("dans 5 heures", formatRelative(now + 18_000, now, french))
        assertEquals("maintenant", formatRelative(now, now, french))
        assertEquals("nunca", formatRelative(null, now, spanish))
    }

    @Test
    fun episodesAndCountsInFrenchAndSpanish() {
        assertEquals("S1 · É3", episodeLabel(1, 3, french))
        assertEquals("T1 · E3", episodeLabel(1, 3, spanish))
        assertEquals("É3", episodeLabel(null, 3, french))
        assertEquals("Épisodes spéciaux", seasonTitle(null, 0, french))
        assertEquals("Temporada 2", seasonTitle(null, 2, spanish))
        assertEquals("Saison 2 · Épisode 5", episodeLongLabel(2, 5, french))

        val show = item(ItemKind.Show, childCount = 3, leafCount = 30, viewedLeafCount = 24)
        assertEquals("3 saisons", cardSubtitle(show, french))
        assertEquals("1 temporada", cardSubtitle(show.copy(childCount = 1), spanish))
        // French counts 0 as one.
        assertEquals("0 épisode", cardSubtitle(item(ItemKind.Season, leafCount = 0), french))
        assertEquals("Severance, 3 saisons, 6 non vus", tileDescription(show, french))
        assertEquals("Severance, 3 temporadas, 6 sin ver", tileDescription(show, spanish))

        val episode = item(ItemKind.Episode, index = 5, parentIndex = 2, grandparentTitle = "Glasshouse", durationMs = 3_480_000)
            .copy(userState = UserState(positionMs = 1_920_000))
        assertEquals("Severance, Glasshouse, Saison 2 Épisode 5, il reste 26 minutes", tileDescription(episode, french))
        assertEquals("Severance, Glasshouse, Temporada 2 Episodio 5, quedan 26 minutos", tileDescription(episode, spanish))
    }

    @Test
    fun streamsAndFilesInFrenchAndSpanish() {
        assertEquals("1,5 Ko", formatBytes(1536, french))
        assertEquals("1,5 KB", formatBytes(1536, spanish))
        assertEquals("8,2 Mbit/s", formatBitrate(8_200_000, french))
        assertEquals("Débit inconnu", mediaLine(MediaSummary(), french))
        assertEquals("23,976 i/s", frameRateLabel(23.976, french))
        assertEquals("Tout HDR", dynamicRangeFilterLabel(DynamicRangeFilter.Hdr, french))
        assertEquals("Dolby Vision", dynamicRangeFilterLabel(DynamicRangeFilter.DolbyVision, spanish))

        val audio = MediaStream(index = 1, kind = StreamKind.Audio, codec = "aac", language = "eng", channels = 2)
        assertEquals("Anglais (AAC Stéréo)", streamLabel(audio, french))
        assertEquals("Inglés (AAC Estéreo)", streamLabel(audio, spanish))
        // A title that names the language in English adds nothing.
        val subtitle = MediaStream(index = 2, kind = StreamKind.Subtitle, codec = "subrip", language = "ger", title = "German Forced", forced = true)
        assertEquals("Allemand (SRT, Forcés)", streamLabel(subtitle, french))
        assertEquals("Desconocido (AAC 5 canales)", streamLabel(audio.copy(language = null, channels = 5), spanish))

        val video = MediaStream(index = 0, kind = StreamKind.Video, codec = "hevc", dynamicRange = DynamicRange.DolbyVision, dvProfile = 8, dvBlCompat = 1, dvLevel = 6)
        assertEquals("Profil 8.1 · Niveau 6 · couche de base compatible HDR10", dolbyVisionDetails(video, french))
        assertEquals("Perfil 8.1 · Nivel 6 · capa base compatible con HDR10", dolbyVisionDetails(video, spanish))

        val job = TranscodeInfo(jobId = "j", video = StreamAction.Copy, videoIn = "HEVC", audio = StreamAction.Copy, audioIn = "EAC3 5.1", speed = 3.2)
        assertEquals("HEVC (copie) · EAC3 5.1 (copie) · 3,2×", transcodeSummary(job, french))
    }

    @Test
    fun languagesAreNamedInTheLanguageShown() {
        assertEquals("Anglais", languageName("eng", french))
        assertEquals("Portugais (BR)", languageName("pt-BR", french))
        assertEquals("Francés", languageName("fre", spanish))
        assertEquals("Langue inconnue", languageName("und", french))
        assertEquals("Mandarín", languageName("cmn", spanish))
        assertEquals("XX", languageName("xx", french))
        // English names stay the table's.
        assertEquals("Norwegian Bokmål", languageName("nob", english))
        assertEquals("Unknown", languageName("und", english))
    }

    private fun item(
        kind: ItemKind,
        index: Int? = null,
        parentIndex: Int? = null,
        grandparentTitle: String? = null,
        childCount: Int? = null,
        leafCount: Int? = null,
        viewedLeafCount: Int? = null,
        durationMs: Long? = null,
    ) = ItemSummary(
        id = 1, libraryId = 1, kind = kind, title = "Severance", index = index, parentIndex = parentIndex,
        grandparentTitle = grandparentTitle, childCount = childCount, leafCount = leafCount, viewedLeafCount = viewedLeafCount,
        durationMs = durationMs,
    )
}
