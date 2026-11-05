package app.pley.android.ui.phone.requests

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.progressSemantics
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.shownLocale
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.DownloadState
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.screens.R
import app.pley.android.screens.requests.PillTone
import app.pley.android.screens.requests.RequestEvent
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.StagePill
import app.pley.android.screens.requests.badgeOf
import app.pley.android.screens.requests.downloadProgress
import app.pley.android.screens.requests.stagePill
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import kotlinx.coroutines.flow.Flow

/*
 * The pieces the phone's request screens share: the
 * words, posters with their status badges, stage pills in their colours,
 * the download bar, the states a page can't show in, and what a screen
 * tells once.
 */

/** Requests' words, from this screen's resources; made again when the language shown changes, for what remembers them. */
@Composable
internal fun rememberRequestTexts(): RequestTexts {
    val resources = LocalResources.current
    val locale = shownLocale()
    return remember(resources, locale) { RequestTexts(resources) }
}

/** A title's key in lazy rows and grids: a movie and a show may share a TMDB id. */
internal val DiscoverTitle.tileId: Long get() = tmdbId * 2 + if (kind == TitleKind.Show) 1 else 0

/** "2024 · Movie". */
internal fun titleLine(year: Int?, kind: TitleKind, texts: RequestTexts): String =
    listOfNotNull(year?.toString(), texts.kind(kind)).joinToString(" · ")

/** [title]'s poster card: its year and kind under it, its status badge on it ("In library", "Requested"). */
internal fun titleTile(title: DiscoverTitle, origin: String, texts: RequestTexts): ArtworkTileModel {
    val badge = badgeOf(title)?.let(texts::badge)
    return ArtworkTileModel(
        id = title.tileId,
        title = title.title,
        subtitle = titleLine(title.year, title.kind, texts),
        image = ArtworkImage.of(title.posterUrl, origin),
        badges = listOfNotNull(badge),
        description = listOfNotNull(title.title, title.year?.toString(), texts.kind(title.kind), badge).joinToString(", "),
    )
}

/** [request]'s poster card: its stage under it ("Waiting for approval"), the download's progress on it. */
internal fun requestTile(request: MediaRequest, origin: String, texts: RequestTexts, manage: Boolean): ArtworkTileModel {
    val stage = texts.pill(stagePill(request, manage))
    return ArtworkTileModel(
        id = request.id,
        title = request.title,
        subtitle = stage,
        image = ArtworkImage.of(request.posterUrl, origin),
        progress = downloadProgress(request) ?: 0f,
        description = "${request.title}, $stage",
    )
}

/** A stage pill's colours: its words and dot, and the tint behind them. */
private class Tone(val content: Color, val container: Color)

private fun toneOf(tone: PillTone): Tone = when (tone) {
    PillTone.Amber -> Tone(PleyColors.Amber, PleyColors.Amber.copy(alpha = 0.14f))
    PillTone.Blue -> Tone(PleyColors.AccentBright, PleyColors.Accent.copy(alpha = 0.16f))
    PillTone.Green -> Tone(PleyColors.Success, PleyColors.Success.copy(alpha = 0.16f))
    PillTone.Red -> Tone(PleyColors.DangerText, PleyColors.Danger.copy(alpha = 0.16f))
    PillTone.Gray -> Tone(PleyColors.Muted, PleyColors.Surface4)
}

/**
 * A request's stage: its words beside a dot, tinted by
 * what it means: waiting for approval amber; approved, waiting, searching,
 * downloading and importing blue; ready green; declined and failed red;
 * gone gray.
 */
@Composable
internal fun StagePillChip(pill: StagePill, texts: RequestTexts, modifier: Modifier = Modifier) {
    val tone = toneOf(pill.tone)
    Row(
        modifier
            .heightIn(min = 24.dp)
            .background(tone.container, PleyShape.Control)
            .padding(horizontal = 8.dp, vertical = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(6.dp).background(tone.content, PleyShape.Circle))
        Spacer(Modifier.width(6.dp))
        Text(texts.pill(pill), style = MaterialTheme.typography.labelMedium, color = tone.content, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * [request]'s pill, the download's bar under it while one runs, and the
 * line a failed request has ("Trying again in 5 min.").
 */
@Composable
internal fun RequestStage(request: MediaRequest, manage: Boolean, texts: RequestTexts, modifier: Modifier = Modifier) {
    val pill = stagePill(request, manage)
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        StagePillChip(pill, texts)
        val download = request.download
        if (download != null) DownloadBar(downloadProgress(request) ?: 0f, download.state, Modifier.widthIn(max = DOWNLOAD_BAR_WIDTH).fillMaxWidth())
        texts.pillLine(pill)?.let { line ->
            Text(line, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        }
    }
}

/** A download's progress, 4 dp tall: blue, gray while paused, red with a problem. Screen readers hear its value. */
@Composable
internal fun DownloadBar(progress: Float, state: DownloadState, modifier: Modifier = Modifier) {
    val fill = when (state) {
        DownloadState.Warning -> PleyColors.Danger
        DownloadState.Paused -> PleyColors.Subtle
        else -> PleyColors.Accent
    }
    val value = progress.coerceIn(0f, 1f)
    Box(
        modifier
            .height(4.dp)
            .progressSemantics(value)
            .clip(PleyShape.Badge)
            .background(PleyColors.Surface4),
    ) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(value).background(fill))
    }
}

/** "Details and pictures from TMDB.", at the foot of Discover and of every title page. */
@Composable
internal fun TmdbCredit(modifier: Modifier = Modifier) {
    Text(
        stringResource(R.string.requests_credit),
        style = MaterialTheme.typography.bodySmall,
        color = PleyColors.Subtle,
        textAlign = TextAlign.Center,
        modifier = modifier.fillMaxWidth(),
    )
}

/**
 * Why a page can't show: requests off, no access, sign
 * in again (with Sign in), TMDB's key refused, TMDB away or the
 * server out of reach (with Retry), a title not found. [failed] heads
 * any other failure ("Couldn't load Discover").
 */
@Composable
internal fun RequestProblemState(
    problem: RequestProblem,
    summary: RequestSummary?,
    texts: RequestTexts,
    failed: String,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val shell = LocalPhoneShell.current
    EmptyState(
        title = problemTitle(problem, summary, texts, failed),
        body = problemBody(problem, summary, texts),
        icon = problemIcon(problem),
        modifier = modifier.semantics { liveRegion = LiveRegionMode.Polite },
        action = when {
            problem is RequestProblem.SignInAgain -> ({ PrimaryButton(stringResource(R.string.requests_sign_in), onClick = shell::signInAgain) })
            problem.retry -> ({ TonalButton(stringResource(R.string.requests_retry), onClick = onRetry, icon = PleyIcons.RefreshCw) })
            else -> null
        },
    )
}

/** [problem]'s heading: "Can't reach your server", [failed] over any other failure, else its own words. */
@Composable
internal fun problemTitle(problem: RequestProblem, summary: RequestSummary?, texts: RequestTexts, failed: String): String = when (problem) {
    is RequestProblem.Offline -> stringResource(app.pley.android.ui.phone.R.string.kit_offline_title)
    is RequestProblem.Other -> failed
    else -> texts.problem(problem, summary)
}

/** The line under [problemTitle], where there is one: a failure's own sentence, or what can be done. */
internal fun problemBody(problem: RequestProblem, summary: RequestSummary?, texts: RequestTexts): String? = when (problem) {
    is RequestProblem.Offline, is RequestProblem.Other -> texts.problem(problem, summary)
    else -> texts.problemBody(problem, summary)
}

private fun problemIcon(problem: RequestProblem): PleyIcon = when (problem) {
    RequestProblem.Off -> PleyIcons.Compass
    RequestProblem.NoAccess, is RequestProblem.Restricted -> PleyIcons.Lock
    is RequestProblem.SignInAgain -> PleyIcons.LogOut
    RequestProblem.TmdbKey -> PleyIcons.TriangleAlert
    RequestProblem.TmdbDown, is RequestProblem.Offline -> PleyIcons.ServerOff
    else -> PleyIcons.CircleAlert
}

/**
 * What a request screen tells once: what asking did ("Asked for “X”: an
 * administrator decides."), why something failed (with Sign in for
 * a missing scope; the offline banner for a server out of reach), and
 * where to go.
 */
@Composable
internal fun RequestEvents(events: Flow<RequestEvent>, texts: RequestTexts, summary: () -> RequestSummary?) {
    val shell = LocalPhoneShell.current
    val navigator = LocalPhoneNavigator.current
    val signIn = stringResource(R.string.requests_sign_in)
    LaunchedEffect(events, texts) {
        events.collect { event ->
            when (event) {
                is RequestEvent.Asked -> shell.toasts.show(texts.asked(event.asked, event.title))
                is RequestEvent.Open -> navigator.open(event.key)
                is RequestEvent.Failed -> when (val problem = event.problem) {
                    is RequestProblem.SignInAgain ->
                        shell.toasts.show(texts.problem(problem, summary()), actionLabel = signIn, onAction = shell::signInAgain, error = true)
                    is RequestProblem.Offline -> shell.report(problem.error)
                    else -> shell.toasts.show(texts.problem(problem, summary()), error = true)
                }
            }
        }
    }
}

/** The widest a request's download bar gets. */
private val DOWNLOAD_BAR_WIDTH = 200.dp
