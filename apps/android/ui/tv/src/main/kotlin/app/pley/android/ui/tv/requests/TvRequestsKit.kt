package app.pley.android.ui.tv.requests

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.progressSemantics
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverTitle
import app.pley.android.model.DownloadState
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestSummary
import app.pley.android.model.TitleKind
import app.pley.android.model.format.formatRuntime
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.item.MetaPart
import app.pley.android.screens.requests.PillTone
import app.pley.android.screens.requests.RequestEvent
import app.pley.android.screens.requests.RequestProblem
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.requests.StagePill
import app.pley.android.screens.requests.badgeOf
import app.pley.android.screens.requests.downloadProgress
import app.pley.android.screens.requests.stagePill
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.TvToastState
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvTextButton
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.onboarding.TvDialog
import app.pley.android.ui.tv.settings.hostPort
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle
import java.util.Locale
import kotlinx.coroutines.flow.Flow

/*
 * The pieces the TV's request screens share, as the Samsung and LG apps
 * have them (packages/tv's request-kit.tsx), built on the TV's kit: the
 * words, posters with their status badges, stage pills in their colours,
 * the download bar, the page a problem leaves, signing in again, and what
 * a screen tells once.
 */

/** Requests' words, from this screen's resources. */
@Composable
internal fun rememberRequestTexts(): RequestTexts {
    val resources = LocalResources.current
    return remember(resources) { RequestTexts(resources) }
}

/** A title's key in rows and grids: a movie and a show may share a TMDB id. */
internal val DiscoverTitle.tileId: Long get() = tmdbId * 2 + if (kind == TitleKind.Show) 1 else 0

/** "2024 · Movie". */
internal fun titleLine(year: Int?, kind: TitleKind, texts: RequestTexts): String =
    listOfNotNull(year?.toString(), texts.kind(kind)).joinToString(" · ")

/** [title]'s poster: its year and kind under it, its status on it ("In library", "Requested"). */
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

/** [request] as a wide tile: its backdrop (or poster), the download's progress on it, its stage under it. */
internal fun requestTile(request: MediaRequest, origin: String, texts: RequestTexts, manage: Boolean): ArtworkTileModel {
    val stage = texts.pill(stagePill(request, manage))
    val art = listOfNotNull(request.backdropUrl, request.posterUrl).filter { it.isNotEmpty() }
    return ArtworkTileModel(
        id = request.id,
        title = request.title,
        subtitle = stage,
        image = art.firstOrNull()?.let { ArtworkImage.of(it, origin) },
        progress = downloadProgress(request) ?: 0f,
        description = "${request.title}, $stage",
    )
}

/**
 * A title's facts as the item page's line has them: its year (else
 * whether it's a movie or a show), its certification boxed, its runtime
 * or how many seasons, TMDB's rating after a star.
 */
internal fun titleMeta(detail: DiscoverDetail, seasons: String?, kind: String): List<MetaPart> = buildList {
    add(MetaPart.Text(detail.year?.toString() ?: kind))
    detail.certification?.takeIf { it.isNotBlank() }?.let { add(MetaPart.ContentRating(it)) }
    val length = if (detail.kind == TitleKind.Show) seasons else detail.runtimeMinutes?.takeIf { it > 0 }?.let { formatRuntime(it * 60_000L) }
    length?.takeIf { it.isNotEmpty() }?.let { add(MetaPart.Text(it)) }
    detail.rating?.takeIf { it > 0 }?.let { add(MetaPart.Rating(String.format(Locale.ROOT, "%.1f", it))) }
}

/** A list title's facts, before its page is read: its year and kind, TMDB's rating. */
internal fun titleMeta(title: DiscoverTitle, texts: RequestTexts): List<MetaPart> = buildList {
    add(MetaPart.Text(titleLine(title.year, title.kind, texts)))
    title.rating?.takeIf { it > 0 }?.let { add(MetaPart.Rating(String.format(Locale.ROOT, "%.1f", it))) }
}

/** A stage's colours: its words and dot, and the tint behind them. */
@Immutable
internal class Tone(val content: Color, val container: Color)

internal fun toneOf(tone: PillTone): Tone = when (tone) {
    PillTone.Amber -> Tone(PleyColors.Amber, PleyColors.Amber.copy(alpha = 0.2f))
    PillTone.Blue -> Tone(PleyColors.AccentBright, PleyColors.Info.copy(alpha = 0.2f))
    PillTone.Green -> Tone(PleyColors.Success, PleyColors.Success.copy(alpha = 0.2f))
    PillTone.Red -> Tone(PleyColors.DangerText, PleyColors.DangerText.copy(alpha = 0.2f))
    PillTone.Gray -> Tone(PleyColors.Muted, PleyColors.Muted.copy(alpha = 0.16f))
}

/**
 * A request's stage: its words after a dot, tinted by what it means:
 * waiting for approval amber; approved, waiting, searching, downloading
 * and importing blue; ready green; declined and failed red; gone gray.
 * 26 dp tall beside a button, 22 [small] under a title's facts.
 */
@Composable
internal fun TvStagePill(pill: StagePill, texts: RequestTexts, modifier: Modifier = Modifier, small: Boolean = false) {
    val tone = toneOf(pill.tone)
    Row(
        modifier
            .height(if (small) 22.dp else 26.dp)
            .background(tone.container, PleyShape.Circle)
            .padding(start = if (small) 8.dp else 10.dp, end = if (small) 10.dp else 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(if (small) 6.dp else 8.dp).background(tone.content, PleyShape.Circle))
        Spacer(Modifier.width(if (small) 6.dp else 8.dp))
        BasicText(
            texts.pill(pill),
            style = (if (small) PleyType.Tv.Caption else PleyType.Tv.Label).copy(color = tone.content),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** A download's progress, 4 dp tall: blue, gray while paused, red with a problem. Screen readers hear its value. */
@Composable
internal fun TvDownloadBar(progress: Float, state: DownloadState, modifier: Modifier = Modifier) {
    val fill = when (state) {
        DownloadState.Warning -> PleyColors.Danger
        DownloadState.Paused -> PleyColors.Subtle
        else -> PleyColors.Info
    }
    val value = progress.coerceIn(0f, 1f)
    Box(
        modifier
            .height(4.dp)
            .progressSemantics(value)
            .clip(PleyShape.Badge)
            .background(PleyColors.Fg.copy(alpha = 0.16f)),
    ) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(value).background(fill))
    }
}

/** [request]'s pill with its download's bar 4 dp under it, as wide as the pill: beside a title's button, at a card's end. */
@Composable
internal fun TvStageBlock(request: MediaRequest, manage: Boolean, texts: RequestTexts, modifier: Modifier = Modifier) {
    val download = request.download
    Column(modifier.width(IntrinsicSize.Max), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        TvStagePill(stagePill(request, manage), texts)
        if (download != null) TvDownloadBar(downloadProgress(request) ?: 0f, download.state, Modifier.fillMaxWidth())
    }
}

/**
 * A request's stage under a library title's facts (packages/tv's
 * ItemRequestLine): the small pill, and a download's 120 dp bar beside it.
 */
@Composable
internal fun TvItemRequestLine(request: MediaRequest, manage: Boolean, modifier: Modifier = Modifier) {
    val texts = rememberRequestTexts()
    val download = request.download
    Row(modifier, verticalAlignment = Alignment.CenterVertically) {
        TvStagePill(stagePill(request, manage), texts, small = true)
        if (download != null) {
            Spacer(Modifier.width(12.dp))
            TvDownloadBar(downloadProgress(request) ?: 0f, download.state, Modifier.width(ItemBarWidth))
        }
    }
}

/** "Details and pictures from TMDB.", at the foot of Discover and of every title page. */
@Composable
internal fun TvTmdbCredit(modifier: Modifier = Modifier) {
    BasicText(
        stringResource(ScreensR.string.requests_credit),
        style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle),
        modifier = modifier.padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal),
    )
}

/** [problem]'s heading: "Can't reach your server", [failed] over any other failure, else its own words. */
@Composable
internal fun problemTitle(problem: RequestProblem, summary: RequestSummary?, texts: RequestTexts, failed: String): String = when (problem) {
    is RequestProblem.Offline -> failedTitle(problem.error, failed)
    is RequestProblem.Other -> failedTitle(problem.error, failed)
    else -> texts.problem(problem, summary)
}

/** The line under [problemTitle], where there is one: what to check, a failure's own sentence, or what can be done. */
@Composable
internal fun problemBody(problem: RequestProblem, summary: RequestSummary?, texts: RequestTexts): String? = when (problem) {
    is RequestProblem.Offline -> failedBody(problem.error)
    is RequestProblem.Other -> failedBody(problem.error)
    else -> texts.problemBody(problem, summary)
}

internal fun problemIcon(problem: RequestProblem): PleyIcon = when (problem) {
    RequestProblem.Off -> PleyIcons.Compass
    RequestProblem.NoAccess, is RequestProblem.Restricted -> PleyIcons.Lock
    is RequestProblem.SignInAgain -> PleyIcons.LogOut
    RequestProblem.TmdbKey -> PleyIcons.TriangleAlert
    RequestProblem.TmdbDown, is RequestProblem.Offline -> PleyIcons.ServerOff
    else -> PleyIcons.CircleAlert
}

/**
 * A page that can't show requests, saying why (design spec §8): off, not
 * open to this account (its requests still reachable when it has some),
 * signed in before requests (Sign in again), TMDB's key, TMDB not
 * answering or the server out of reach (Retry); anything else under
 * [failed] ("Couldn't load Discover"), with Retry. With nothing to press,
 * focus goes to the rail.
 */
@Composable
internal fun TvRequestMessage(
    problem: RequestProblem,
    summary: RequestSummary?,
    texts: RequestTexts,
    failed: String,
    server: String,
    onRetry: () -> Unit,
) {
    val navigator = LocalTvNavigator.current
    var signIn by remember { mutableStateOf(false) }
    val retry = stringResource(ScreensR.string.requests_retry)
    val action: Triple<String, PleyIcon, () -> Unit>? = when {
        problem.retry -> Triple(retry, PleyIcons.RefreshCw, onRetry)
        problem is RequestProblem.SignInAgain -> Triple(stringResource(ScreensR.string.requests_sign_in), PleyIcons.LogOut) { signIn = true }
        problem == RequestProblem.NoAccess && summary != null && (summary.own > 0 || summary.manage) && summary.requestsEntry -> Triple(
            stringResource(if (summary.manage) ScreensR.string.requests_manage else ScreensR.string.requests_yours),
            PleyIcons.ListVideo,
        ) { navigator.open(PleyKey.Requests) }
        else -> null
    }
    val pressed = remember { FocusRequester() }
    TvRequestsNotice(
        title = problemTitle(problem, summary, texts, failed),
        body = problemBody(problem, summary, texts),
        icon = problemIcon(problem),
        action = action,
        actionFocus = pressed,
    )
    if (signIn) TvSignInAgainDialog(server, onDismiss = { signIn = false }, returnTo = pressed)
}

/**
 * A sentence in place of the page (design spec §8): [icon], [title] in
 * tvPageTitle and [body] muted, at the gutter, and one focused button
 * when there is an [action]; without one, focus goes to the rail, as
 * there is nothing on the page to press.
 */
@Composable
internal fun TvRequestsNotice(
    title: String,
    body: String?,
    icon: PleyIcon = PleyIcons.Compass,
    action: Triple<String, PleyIcon, () -> Unit>? = null,
    modifier: Modifier = Modifier,
    top: Dp = NoticeTop,
    actionFocus: FocusRequester? = null,
) {
    val rail = LocalTvRailFocus.current
    Column(
        modifier
            .fillMaxSize()
            .padding(start = PleyTv.Gutter, top = top, end = TvSafeArea.Horizontal)
            .semantics { liveRegion = LiveRegionMode.Polite },
    ) {
        Icon(icon, contentDescription = null, size = 40.dp, tint = PleyColors.Muted)
        Spacer(Modifier.height(16.dp))
        BasicText(title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), modifier = Modifier.widthIn(max = 560.dp).semantics { heading() })
        if (body != null) {
            Spacer(Modifier.height(8.dp))
            BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), modifier = Modifier.widthIn(max = 464.dp))
        }
        if (action != null) {
            Spacer(Modifier.height(24.dp))
            val (label, glyph, onClick) = action
            TvPillButton(
                label,
                onClick = onClick,
                icon = glyph,
                modifier = Modifier.initialFocus().then(if (actionFocus != null) Modifier.focusRequester(actionFocus) else Modifier),
            )
        }
    }
    if (action == null) {
        LaunchedEffect(Unit) {
            withFrameNanos { }
            rail?.focusCurrent()
        }
    }
}

/**
 * Signing in again, so this TV may ask for movies and shows (or decide
 * on them): the link screen of [server] with a new code, whose approval
 * replaces this sign-in. Cancel, or Sign in again with focus; focus goes
 * back to [returnTo] on Cancel.
 */
@Composable
internal fun TvSignInAgainDialog(server: String, onDismiss: () -> Unit, returnTo: FocusRequester? = null) {
    val navigator = LocalTvNavigator.current
    TvDialog(
        title = stringResource(R.string.tv_requests_sign_in_title),
        body = stringResource(R.string.tv_requests_sign_in_body, hostPort(server)),
        onDismiss = onDismiss,
        returnTo = returnTo,
    ) {
        TvTextButton(stringResource(ScreensR.string.requests_cancel), onClick = onDismiss)
        TvPillButton(
            stringResource(ScreensR.string.requests_sign_in),
            onClick = {
                onDismiss()
                navigator.restart(PleyKey.Link(server))
            },
            modifier = Modifier.initialFocus(),
        )
    }
}

/**
 * What a request screen tells once: what asking did ("Asked for “X”: an
 * administrator decides.") and why something failed, in [toasts]; and
 * where to go.
 */
@Composable
internal fun TvRequestEvents(events: Flow<RequestEvent>, texts: RequestTexts, toasts: TvToastState, summary: () -> RequestSummary?) {
    val navigator = LocalTvNavigator.current
    LaunchedEffect(events, texts) {
        events.collect { event ->
            when (event) {
                is RequestEvent.Asked -> toasts.show(texts.asked(event.asked, event.title))
                is RequestEvent.Open -> navigator.open(event.key)
                is RequestEvent.Failed -> toasts.show(texts.problem(event.problem, summary()), error = true)
            }
        }
    }
}

/** Where a notice in place of a page sits (design spec §8). */
private val NoticeTop = 120.dp

/** A library title's download bar, beside its stage. */
private val ItemBarWidth = 120.dp
