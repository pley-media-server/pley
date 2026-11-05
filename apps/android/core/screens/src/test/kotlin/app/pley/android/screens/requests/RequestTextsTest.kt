package app.pley.android.screens.requests

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.data.RequestNotice
import app.pley.android.model.Asked
import app.pley.android.model.DiscoverSeason
import app.pley.android.model.DownloadState
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.SeasonState
import app.pley.android.model.TitleKind
import java.io.IOException
import java.time.ZoneOffset
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Requests' words, on a fixed clock: 2025-10-30 12:00 UTC. */
@RunWith(RobolectricTestRunner::class)
class RequestTextsTest {
    private val now = 1_761_825_600L
    private val english = ApplicationProvider.getApplicationContext<Context>()
    private val texts = RequestTexts(english.resources, now = { now }, locale = Locale.US, zone = ZoneOffset.UTC)

    /** Requests' words in [tag]'s language, dates written in it as the screens' resources say. */
    private fun textsIn(tag: String): RequestTexts {
        val context = english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })
        return RequestTexts(context.resources, now = { now }, zone = ZoneOffset.UTC)
    }

    private val day = 86_400L

    /** French puts a no-break space before : ; ? ! and %, and inside « ». */
    private val nb = ' '

    @Test
    fun seasonsAreNamedAsTheWebNamesThem() {
        assertEquals("Season 2", texts.seasons(listOf(2)))
        assertEquals("Seasons 1–3, 5", texts.seasons(listOf(5, 3, 2, 1)))
        assertEquals("Seasons 1–2 and specials", texts.seasons(listOf(0, 1, 2)))
        assertEquals("Season 3 and specials", texts.seasons(listOf(3, 0)))
        assertEquals("Specials", texts.seasons(listOf(0)))
    }

    @Test
    fun pillsSayTheStage() {
        assertEquals("Waiting for approval", texts.pill(StagePill.Pending))
        assertEquals("Downloading 42% · in 20 min", texts.pill(StagePill.Downloading(0.42f, now + 20 * 60, DownloadState.Downloading)))
        assertEquals("Downloading 42%", texts.pill(StagePill.Downloading(0.42f, null, DownloadState.Downloading)))
        assertEquals("Waiting to download", texts.pill(StagePill.Downloading(0f, null, DownloadState.Queued)))
        assertEquals("Download paused", texts.pill(StagePill.Downloading(0.5f, null, DownloadState.Paused)))
        assertEquals("The download has a problem", texts.pill(StagePill.Downloading(0.5f, null, DownloadState.Warning)))
        // A release known by its day comes at 12:00 UTC of that day.
        assertEquals("Out Nov 4", texts.pill(StagePill.Waiting(show = false, at = 1_762_257_600)))
        assertEquals("Airs Nov 4", texts.pill(StagePill.Waiting(show = true, at = 1_762_257_600)))
        assertEquals("Not out yet", texts.pill(StagePill.Waiting(show = false, at = null)))
        assertEquals("Seasons 1–2 ready", texts.pill(StagePill.Partial(listOf(1, 2))))
        assertEquals("Ready to watch", texts.pill(StagePill.Available))
        assertEquals("No longer in a library", texts.pill(StagePill.Gone))
        assertEquals("Adding to the library", texts.pill(StagePill.Importing))
        assertEquals("Couldn't send: Radarr isn't answering", texts.pill(StagePill.CouldNotSend("Radarr isn't answering", null)))
        assertEquals("Not sent yet", texts.pill(StagePill.NotSentYet(now + 300)))
        assertEquals("Trying again in 5 min.", texts.pillLine(StagePill.NotSentYet(now + 300)))
        assertEquals("It couldn't be sent to download yet: an administrator will look at it.", texts.pillLine(StagePill.NotSentYet(null)))
    }

    @Test
    fun timesAreWrittenHere() {
        assertEquals("in 20 min", texts.until(now + 20 * 60))
        assertEquals("in 3 hours", texts.until(now + 3 * 3_600))
        assertEquals("tomorrow", texts.until(now + day))
        assertEquals("in 12 days", texts.until(now + 12 * day))
        assertEquals("3 days ago", texts.ago(now - 3 * day))
    }

    @Test
    fun theAllowanceLine() {
        val allowance = Allowance(2, 5, 3, 10, null, null)
        assertEquals(
            listOf("You can ask for 2 more movies and 5 more seasons.", "Limits: 3 movies and 10 seasons per 7 days."),
            texts.allowance(allowance),
        )
        assertEquals(listOf("You can ask for 1 more movie.", "Limit: 3 movies per 7 days."), texts.allowance(allowance.copy(moviesLeft = 1), TitleKind.Movie))
        val noMovies = allowance.copy(moviesLeft = 0, moviesNextAt = now + 3 * day)
        assertEquals(listOf("You've asked for all the movies you can for now: you can ask for more in 3 days."), texts.allowance(noMovies, TitleKind.Movie))
        val nothing = noMovies.copy(seasonsLeft = 0, seasonsNextAt = now + 5 * day)
        assertEquals(listOf("You've asked for all you can for now: more movies in 3 days, more seasons in 5 days."), texts.allowance(nothing))
        assertEquals(emptyList<String>(), texts.allowance(Allowance(null, null, null, null, null, null)))
    }

    @Test
    fun aDeclineSaysWhenToAskAgain() {
        assertEquals("Declined: “Not for us”. You can ask again in 12 days.", texts.declined(MainAction.Declined("Not for us", now + 12 * day)))
        assertEquals("Declined. You can ask again in 12 days.", texts.declined(MainAction.Declined(null, now + 12 * day)))
        assertEquals("Declined.", texts.declined(MainAction.Declined(null, null)))
    }

    @Test
    fun askingSaysWhatHappened() {
        val request = MediaRequest(id = 60, kind = TitleKind.Movie, tmdbId = 1, title = "Arrival", status = RequestStatus.Pending)
        assertEquals("Asked for “Arrival”: an administrator decides.", texts.asked(Asked(request, created = true), "Arrival"))
        assertEquals("“Arrival” is approved and on its way.", texts.asked(Asked(request.copy(status = RequestStatus.Approved), true), "Arrival"))
        assertEquals("Asked for “Arrival”, but it couldn't be sent to download yet.", texts.asked(Asked(request.copy(status = RequestStatus.Failed), true), "Arrival"))
        assertEquals("You'll hear when “Arrival” is ready.", texts.asked(Asked(request, created = false), "Arrival"))
        assertEquals("Pick a season.", texts.pickerButton(SeasonPicker(emptyList(), emptySet(), null)))
        assertEquals("Request 1 season", texts.pickerButton(SeasonPicker(emptyList(), setOf(3), null)))
        assertEquals("Request 3 seasons", texts.pickerButton(SeasonPicker(emptyList(), setOf(3, 4, 5), null)))
    }

    @Test
    fun noticesSayWhatHappened() {
        assertEquals("“Arrival” is ready to watch.", texts.notice(RequestNotice.Ready(1, 45, "Arrival", TitleKind.Movie, 101, null, complete = true)))
        assertEquals("Season 2 of “Severance” is ready to watch.", texts.notice(RequestNotice.Ready(2, 47, "Severance", TitleKind.Show, 201, listOf(2), complete = false)))
        assertEquals("Seasons 1–2 of “Severance” are ready to watch.", texts.notice(RequestNotice.Ready(3, 47, "Severance", TitleKind.Show, 201, listOf(1, 2), false)))
        assertEquals("“Alien: Romulus” was declined: “We have enough aliens for now”.", texts.notice(RequestNotice.Declined(4, 46, "Alien: Romulus", "We have enough aliens for now")))
        assertEquals("“Alien: Romulus” was declined.", texts.notice(RequestNotice.Declined(5, 46, "Alien: Romulus", null)))
        assertEquals("“Arrival” was approved.", texts.notice(RequestNotice.Approved(6, 45, "Arrival")))
        assertEquals("Kit asked for “Arcane” (Seasons 1–3).", texts.notice(RequestNotice.Asked(7, 49, "Arcane", "Kit", listOf(1, 2, 3))))
        assertEquals("“Inside Out 2” couldn't be sent to Radarr: Radarr isn't answering", texts.notice(RequestNotice.Failed(8, 48, "Inside Out 2", TitleKind.Movie, "Radarr isn't answering")))
        assertEquals("“Arcane” couldn't be sent to Sonarr.", texts.notice(RequestNotice.Failed(9, 49, "Arcane", TitleKind.Show, null)))
        assertEquals("3 of your requests have news", texts.notice(RequestNotice.News(10, 3)))
    }

    @Test
    fun theNumberOnTheWayInIsSaidInWords() {
        assertEquals("1 of your requests has news", texts.count(RequestsEntry(requests = true, badge = 1)))
        assertEquals("1 request needs you", texts.count(RequestsEntry(requests = true, manage = true, badge = 1)))
        assertEquals("3 requests need you, 1 couldn't be sent", texts.count(RequestsEntry(requests = true, manage = true, badge = 3, failed = 1)))
    }

    @Test
    fun refusalsAreSaid() {
        val summary = RequestSummary(enabled = true, seasonsLimit = 5, manage = true)
        assertEquals("You can ask for at most 5 seasons per 7 days: pick fewer.", texts.problem(RequestProblem.Limit(null), summary))
        assertEquals("You can ask for more in 3 days", texts.problem(RequestProblem.Limit(now + 3 * day), summary))
        assertEquals("You can ask again in 12 days", texts.problem(RequestProblem.Declined(now + 12 * day), summary))
        assertEquals("Requests are off", texts.problem(RequestProblem.Off, null))
        assertEquals("This server doesn't take requests for movies and shows.", texts.problemBody(RequestProblem.Off, null))
        assertEquals("TMDB refused this server's key", texts.problem(RequestProblem.TmdbKey, summary))
        assertEquals("An administrator needs to check it.", texts.problemBody(RequestProblem.TmdbKey, summary.copy(manage = false)))
        assertEquals("Sign in again to ask for movies and shows", texts.problem(RequestProblem.SignInAgain(app.pley.android.model.RequestScope.Requests), null))
        assertEquals("The connection to the server failed.", texts.problem(RequestProblem.Other(IOException("unexpected end of stream")), null))
    }

    @Test
    fun theSpecialsAloneHaveTheirOwnWords() {
        val specials = RequestNotice.Ready(11, 47, "Severance", TitleKind.Show, 201, listOf(0), complete = false)
        assertEquals("Specials of “Severance” is ready to watch.", texts.notice(specials))
        assertEquals("Les épisodes spéciaux de «${nb}Severance$nb» sont prêts à regarder.", textsIn("fr").notice(specials))
        assertEquals("Los especiales de «Severance» ya están listos para ver.", textsIn("es").notice(specials))
    }

    @Test
    fun ratingsAreWrittenInTheLanguageShown() {
        assertEquals("7.4", texts.rating(7.44))
        assertEquals("7,4", textsIn("fr").rating(7.44))
        assertEquals("7,4", textsIn("es").rating(7.44))
    }

    @Test
    fun frenchSaysTheSame() {
        val fr = textsIn("fr")
        assertEquals("Saisons 1–3, 5", fr.seasons(listOf(5, 3, 2, 1)))
        assertEquals("Saison 3 et épisodes spéciaux", fr.seasons(listOf(3, 0)))
        assertEquals("Téléchargement 42$nb% · dans 20 min", fr.pill(StagePill.Downloading(0.42f, now + 20 * 60, DownloadState.Downloading)))
        // A day in the locale's own order.
        assertEquals("Sortie$nb: 4 nov.", fr.pill(StagePill.Waiting(show = false, at = 1_762_257_600)))
        assertEquals("Nouvel essai dans 5 min.", fr.pillLine(StagePill.NotSentYet(now + 300)))
        assertEquals("dans 1 heure", fr.until(now + 3_600))
        assertEquals("dans 12 jours", fr.until(now + 12 * day))
        assertEquals(
            listOf("Vous pouvez encore demander 2 films et 5 saisons.", "Limites$nb: 3 films et 10 saisons sur 7 jours."),
            fr.allowance(Allowance(2, 5, 3, 10, null, null)),
        )
        assertEquals("Vous pouvez encore demander 1 film.", fr.allowance(Allowance(1, null, 3, null, null, null), TitleKind.Movie).first())
        assertEquals("Refusé$nb: «${nb}Not for us$nb». Vous pourrez redemander dans 12 jours.", fr.declined(MainAction.Declined("Not for us", now + 12 * day)))
        assertEquals("Demander 1 saison", fr.pickerButton(SeasonPicker(emptyList(), setOf(3), null)))
        assertEquals("Demander 3 saisons", fr.pickerButton(SeasonPicker(emptyList(), setOf(3, 4, 5), null)))
        assertEquals("4 sur 10 épisodes dans la bibliothèque", fr.seasonState(DiscoverSeason(1, episodes = 10, inLibrary = 4, state = SeasonState.Partial)))
        assertEquals("1 épisode", fr.seasonState(DiscoverSeason(1, episodes = 1, state = SeasonState.Missing)))
        assertEquals("3 demandes vous attendent, 1 n’a pas pu être envoyée", fr.count(RequestsEntry(requests = true, manage = true, badge = 3, failed = 1)))
        assertEquals("Saison 2 de «${nb}Severance$nb» est prête à regarder.", fr.notice(RequestNotice.Ready(2, 47, "Severance", TitleKind.Show, 201, listOf(2), complete = false)))
        assertEquals("Kit a demandé «${nb}Arcane$nb» (Saisons 1–3).", fr.notice(RequestNotice.Asked(7, 49, "Arcane", "Kit", listOf(1, 2, 3))))
        assertEquals("Vous pouvez demander au plus 5 saisons sur 7 jours$nb: choisissez-en moins.", fr.problem(RequestProblem.Limit(null), RequestSummary(enabled = true, seasonsLimit = 5)))
        // An error's words are the app's, in the language shown.
        assertEquals("La connexion au serveur a échoué.", fr.problem(RequestProblem.Other(IOException("unexpected end of stream")), null))
    }

    @Test
    fun spanishSaysTheSame() {
        val es = textsIn("es")
        assertEquals("Temporadas 1–2 y especiales", es.seasons(listOf(0, 1, 2)))
        assertEquals("Descargando 42$nb%", es.pill(StagePill.Downloading(0.42f, null, DownloadState.Downloading)))
        assertEquals("Emisión: 4 nov", es.pill(StagePill.Waiting(show = true, at = 1_762_257_600)))
        assertEquals("en 3 horas", es.until(now + 3 * 3_600))
        assertEquals("mañana", es.until(now + day))
        assertEquals(
            listOf("Puedes solicitar 1 película más y 5 temporadas más.", "Límites: 3 películas y 10 temporadas cada 7 días."),
            es.allowance(Allowance(1, 5, 3, 10, null, null)),
        )
        assertEquals("Solicitar 1 temporada", es.pickerButton(SeasonPicker(emptyList(), setOf(3), null)))
        assertEquals("4 de 10 episodios en la biblioteca", es.seasonState(DiscoverSeason(1, episodes = 10, inLibrary = 4, state = SeasonState.Partial)))
        assertEquals("1 de tus solicitudes tiene novedades", es.count(RequestsEntry(requests = true, badge = 1)))
        assertEquals("¿Retirar tu solicitud de «Arrival»?", es.confirm(CardAction.Withdraw, "Arrival"))
        assertEquals("No se pudo enviar «Arcane» a Sonarr.", es.notice(RequestNotice.Failed(9, 49, "Arcane", TitleKind.Show, null)))
        assertEquals("Podrás volver a solicitarlo en 12 días", es.problem(RequestProblem.Declined(now + 12 * day), null))
        assertEquals("Falló la conexión con el servidor.", es.problem(RequestProblem.Other(IOException("unexpected end of stream")), null))
    }
}
