package app.pley.android.ui.phone.admin.dashboard

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.maxLength
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.errorSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.HistoryPage
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.admin.DashboardActivity
import app.pley.android.model.admin.DashboardSession
import app.pley.android.model.format.formatBytes
import app.pley.android.model.format.formatClockMs
import app.pley.android.model.format.fullTitle
import app.pley.android.screens.PleyKey
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.dashboard.DashboardHistory
import app.pley.android.screens.admin.dashboard.DashboardPlay
import app.pley.android.screens.admin.dashboard.DashboardViewModel
import app.pley.android.screens.admin.dashboard.dashboardTranscodeSummary
import app.pley.android.screens.admin.dashboard.sessionArt
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.account.absolute
import app.pley.android.ui.phone.admin.AdminEvents
import app.pley.android.ui.phone.admin.WhileShown
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.kit.Avatar
import app.pley.android.ui.phone.kit.ConfirmDialog
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * The dashboard (the web's Settings → Dashboard), one page: what plays now
 * (with Stop), the machine's load, everyone's plays, the transcoder's jobs,
 * the libraries' scans and work, the server and its transcoder, and the
 * storage. What changes is read again on the web's timers
 * while the page is on screen, and not at all while it isn't. [id]: none,
 * the dashboard has no page of its own for a thing.
 */
@Suppress("UNUSED_PARAMETER")
@Composable
internal fun DashboardScreen(id: String?) {
    val model: DashboardViewModel = viewModel(factory = DashboardViewModel.factory)
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    val words = formatWords()
    val said: (AdminDone) -> String? = { done ->
        when (done.what) {
            DashboardViewModel.STOPPED -> resources.getString(R.string.admin_dashboard_stopped)
            DashboardViewModel.NOTHING_TO_CLEAN -> resources.getString(R.string.admin_dashboard_nothing_to_clean)
            DashboardViewModel.CLEANED -> {
                val count = done.args[0] as Int
                resources.getQuantityString(R.plurals.admin_dashboard_removed, count, number(count.toLong(), words.locale), formatBytes(done.args[1] as Long, words))
            }
            else -> null
        }
    }
    AdminEvents(model, said)
    // The words of the language shown now, not of when the page opened.
    val saying by rememberUpdatedState(said)
    LaunchedEffect(model) { model.results.collect { done -> saying(done)?.let { shell.toasts.show(it) } } }
    // Read on the web's timers while the page is started, and not at all while it isn't.
    WhileShown(model) { model.keepFresh() }

    val activity by model.activity.collectAsStateWithLifecycle()
    val transcoder by model.transcoder.collectAsStateWithLifecycle()
    val usage by model.resources.collectAsStateWithLifecycle()
    val system by model.system.state.collectAsStateWithLifecycle()
    val storage by model.storage.state.collectAsStateWithLifecycle()
    val historyLoad by model.history.state.collectAsStateWithLifecycle()
    val plays by model.plays.collectAsStateWithLifecycle()
    val loadingMore by model.loadingMore.collectAsStateWithLifecycle()
    val removing by model.removing.collectAsStateWithLifecycle()
    val busy by model.busy.collectAsStateWithLifecycle()
    val origin by model.origin.collectAsStateWithLifecycle()

    var stopping by remember { mutableStateOf<DashboardSession?>(null) }
    var cleaning by rememberSaveable { mutableStateOf(false) }

    val list = rememberLazyListState()
    val metrics = LocalPhoneMetrics.current
    Column(Modifier.fillMaxSize().testTag(DASHBOARD_PAGE)) {
        PleyTopAppBar(title = stringResource(R.string.admin_section_dashboard), onBack = navigator::back, scrolled = { list.canScrollBackward })
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, bottom = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            nowPlaying(activity, origin, onOpen = { navigator.open(opens(it)) }, onStop = { stopping = it })
            item(key = "resources", contentType = "heading") { SectionHeading(stringResource(R.string.admin_dashboard_resources), PleyIcons.Gauge) }
            item(key = "resources.body", contentType = "resources") { ResourcesBody(usage) }
            recentlyPlayed(
                load = historyLoad,
                plays = plays,
                origin = origin,
                loadingMore = loadingMore,
                removing = removing,
                onOpen = { navigator.open(it) },
                onRemove = model::removePlay,
                onMore = model::loadMore,
            )
            transcoderLive(transcoder)
            libraryWork(activity.valueOrNull)
            systemInfo(system, onRetry = model.system::reload)
            storage(storage, system.valueOrNull?.database, busy = busy > 0, onClean = { cleaning = true })
        }
    }

    stopping?.let { session ->
        StopDialog(
            session = session,
            onStop = { message ->
                model.stop(session, message)
                stopping = null
            },
            onDismiss = { stopping = null },
        )
    }
    if (cleaning) {
        ConfirmDialog(
            title = stringResource(R.string.admin_dashboard_clean_up_title),
            body = stringResource(R.string.admin_dashboard_clean_up_body),
            confirmLabel = stringResource(R.string.admin_dashboard_clean_up),
            onConfirm = {
                cleaning = false
                model.cleanUp()
            },
            onDismiss = { cleaning = false },
            destructive = true,
        )
    }
}

/** The page a session's or a play's title opens: a track's album, with the track picked out. */
private fun opens(item: ItemSummary): PleyKey = when (item.kind) {
    ItemKind.Track -> item.parentId?.let { PleyKey.Album(it, trackId = item.id) } ?: PleyKey.Item(item.id)
    else -> PleyKey.Item(item.id)
}

private fun LazyListScope.nowPlaying(
    activity: AdminLoad<DashboardActivity>,
    origin: String,
    onOpen: (ItemSummary) -> Unit,
    onStop: (DashboardSession) -> Unit,
) {
    val sessions = activity.valueOrNull?.sessions.orEmpty()
    item(key = "playing", contentType = "heading") {
        SectionHeading(stringResource(R.string.admin_dashboard_now_playing), PleyIcons.MonitorPlay, count = sessions.size.takeIf { it > 0 })
    }
    when {
        activity is AdminLoad.Loading -> item(key = "playing.loading", contentType = "loading") { Loading() }
        activity is AdminLoad.Failed && activity.stale == null -> item(key = "playing.failed", contentType = "line") {
            QuietLine(errorSentence(activity.error).asString(), danger = true)
        }
        sessions.isEmpty() -> item(key = "playing.none", contentType = "line") { QuietLine(stringResource(R.string.admin_dashboard_nobody_watching)) }
        else -> items(sessions, key = { "session:" + it.sessionId }, contentType = { "session" }) { session ->
            SessionCard(session, origin, onOpen = { onOpen(session.item) }, onStop = { onStop(session) }, modifier = Modifier.animateItem())
        }
    }
}

@Composable
internal fun Loading() {
    Box(Modifier.adminWidth.height(96.dp), contentAlignment = Alignment.Center) {
        DelayedSpinner(stringResource(R.string.admin_loading))
    }
}

/** Someone's playback, as the web's SessionCard: its picture, what, where, who, how far, and how it plays. */
@Composable
private fun SessionCard(session: DashboardSession, origin: String, onOpen: () -> Unit, onStop: () -> Unit, modifier: Modifier = Modifier) {
    val words = formatWords()
    val item = session.item
    val title = remember(item, words) { fullTitle(item, words) }
    val art = remember(item, origin) { ArtworkImage(sessionArt(item).map { absolute(it, origin) }) }
    val duration = item.durationMs ?: 0L
    val device = session.device
    val stopLabel = if (device != null) {
        stringResource(R.string.admin_dashboard_stop_label_on, title, session.username, device)
    } else {
        stringResource(R.string.admin_dashboard_stop_label, title, session.username)
    }
    DashboardCard(modifier.padding(bottom = 8.dp), padding = 12.dp) {
        Row {
            Box(
                Modifier
                    .width(112.dp)
                    .aspectRatio(if (item.kind == ItemKind.Track) 1f else 16f / 9f)
                    .clip(PleyShape.Badge)
                    .background(PleyColors.Surface2)
                    .clickable(role = Role.Button, onClick = onOpen),
            ) {
                Artwork(art, item.title, Modifier.matchParentSize())
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.Top) {
                    Text(
                        title,
                        style = MaterialTheme.typography.titleSmall,
                        color = PleyColors.Fg,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f).clickable(role = Role.Button, onClick = onOpen),
                    )
                    Spacer(Modifier.width(8.dp))
                    MethodBadge(session.method)
                }
                session.device?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    val (icon, tint) = when (session.state) {
                        "playing" -> PleyIcons.Play to PleyColors.Success
                        "paused" -> PleyIcons.Pause to PleyColors.Muted
                        else -> PleyIcons.Square to PleyColors.Muted
                    }
                    Icon(icon, contentDescription = null, size = 12.dp, tint = tint)
                    Spacer(Modifier.width(6.dp))
                    val time = formatClockMs(session.positionMs) + if (duration > 0) " / " + formatClockMs(duration) else ""
                    Text(
                        "${session.username} · $time",
                        style = MaterialTheme.typography.bodySmall,
                        color = PleyColors.Muted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        if (duration > 0) Bar(session.positionMs.toFloat() / duration, Modifier.padding(top = 10.dp), height = 3.dp)
        session.transcode?.let { tc ->
            val size = sizeWords()
            val summary = remember(tc, words) { dashboardTranscodeSummary(tc, words, size) }
            Column(Modifier.padding(top = 10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(verticalAlignment = Alignment.Top) {
                    PipelineBadge(tc.pipeline, Modifier.padding(end = 6.dp))
                    Text(
                        summary.ifEmpty { stringResource(R.string.admin_dashboard_preparing) },
                        style = MaterialTheme.typography.bodySmall,
                        color = PleyColors.Fg.copy(alpha = 0.75f),
                    )
                }
                if (tc.segmentsTotal > 0) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Bar(tc.segmentsReady.toFloat() / tc.segmentsTotal, Modifier.weight(1f), height = 2.dp, color = PleyColors.Info)
                        Spacer(Modifier.width(8.dp))
                        Text(
                            stringResource(R.string.admin_dashboard_segments, tc.segmentsReady, tc.segmentsTotal),
                            style = MaterialTheme.typography.labelSmall,
                            color = PleyColors.Subtle,
                        )
                    }
                }
                tc.lastError?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.DangerText) }
            }
        }
        Row(Modifier.fillMaxWidth().padding(top = 4.dp), horizontalArrangement = Arrangement.End) {
            TextButton(
                onClick = onStop,
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.DangerText),
                // The button says only "Stop"; screen readers hear what stops for whom.
                modifier = Modifier.testTag(STOP_BUTTON).semantics { contentDescription = stopLabel },
            ) {
                Icon(PleyIcons.CircleStop, contentDescription = null, size = 18.dp, tint = PleyColors.DangerText)
                Spacer(Modifier.width(6.dp))
                Text(stringResource(R.string.admin_dashboard_stop), style = MaterialTheme.typography.labelLarge)
            }
        }
    }
}

/** "size 2 of 3", in the words shown. */
@Composable
private fun sizeWords(): (Int, Int) -> String {
    val resources = LocalResources.current
    return { n, total -> resources.getString(R.string.admin_dashboard_size, n, total) }
}

/**
 * Asks before stopping someone's playback, as the web's: what stops for
 * whom, and a message for their screen, up to the server's 200 characters.
 */
@Composable
private fun StopDialog(session: DashboardSession, onStop: (String) -> Unit, onDismiss: () -> Unit) {
    val words = formatWords()
    val state = rememberTextFieldState()
    AlertDialog(
        onDismissRequest = onDismiss,
        confirmButton = {
            TextButton(
                onClick = { onStop(state.text.toString()) },
                colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.DangerText),
                modifier = Modifier.testTag(STOP_CONFIRM),
            ) { Text(stringResource(R.string.admin_dashboard_stop), style = MaterialTheme.typography.labelLarge) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, colors = ButtonDefaults.textButtonColors(contentColor = PleyColors.Fg)) {
                Text(stringResource(R.string.admin_cancel), style = MaterialTheme.typography.labelLarge)
            }
        },
        title = { Text(stringResource(R.string.admin_dashboard_stop_title), style = MaterialTheme.typography.headlineSmall) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(
                    stringResource(R.string.admin_dashboard_stops_for, fullTitle(session.item, words), session.username),
                    style = MaterialTheme.typography.bodyMedium,
                )
                OutlinedTextField(
                    state = state,
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text(stringResource(R.string.admin_dashboard_stop_message)) },
                    supportingText = { Text(stringResource(R.string.admin_dashboard_stop_message_hint)) },
                    inputTransformation = InputTransformation.maxLength(STOP_MESSAGE_MAX),
                    lineLimits = TextFieldLineLimits.MultiLine(minHeightInLines = 2, maxHeightInLines = 4),
                    shape = PleyShape.Field,
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = PleyColors.AccentBright,
                        unfocusedBorderColor = PleyColors.Control,
                        focusedLabelColor = PleyColors.AccentBright,
                        unfocusedLabelColor = PleyColors.Muted,
                        cursorColor = PleyColors.AccentBright,
                        focusedSupportingTextColor = PleyColors.Muted,
                        unfocusedSupportingTextColor = PleyColors.Muted,
                    ),
                )
            }
        },
        shape = PleyShape.Dialog,
        containerColor = PleyColors.Surface2,
        titleContentColor = PleyColors.Fg,
        textContentColor = PleyColors.Muted,
        tonalElevation = 0.dp,
    )
}

private fun LazyListScope.recentlyPlayed(
    load: AdminLoad<HistoryPage>,
    plays: DashboardHistory?,
    origin: String,
    loadingMore: Boolean,
    removing: Set<Long>,
    onOpen: (PleyKey) -> Unit,
    onRemove: (Long) -> Unit,
    onMore: () -> Unit,
) {
    item(key = "history", contentType = "heading") { SectionHeading(stringResource(R.string.admin_dashboard_recently_played), PleyIcons.History) }
    when {
        plays != null && plays.plays.isEmpty() -> item(key = "history.none", contentType = "line") {
            QuietLine(stringResource(R.string.admin_dashboard_nothing_played))
        }
        plays != null -> {
            items(plays.plays, key = { "play:" + it.row.entryId }, contentType = { "play" }) { play ->
                PlayLine(
                    play,
                    origin,
                    removing = play.row.entryId in removing,
                    onOpen = { onOpen(play.row.opens) },
                    onRemove = { onRemove(play.row.entryId) },
                    modifier = Modifier.animateItem(),
                )
            }
            if (plays.canLoadMore) {
                item(key = "history.more", contentType = "more") {
                    Box(Modifier.adminWidth.padding(top = 8.dp), contentAlignment = Alignment.CenterStart) {
                        if (loadingMore) {
                            DelayedSpinner(stringResource(R.string.admin_loading), delayMs = 0)
                        } else {
                            TonalButton(stringResource(R.string.admin_dashboard_show_more), onClick = onMore)
                        }
                    }
                }
            }
        }
        load is AdminLoad.Failed -> item(key = "history.failed", contentType = "line") {
            QuietLine(errorSentence(load.error).asString(), danger = true)
        }
        else -> item(key = "history.loading", contentType = "loading") { Loading() }
    }
}

/** A play, as the web's HistoryList row with its player: picture, what, when and where, who, and the cross that removes it. */
@Composable
private fun PlayLine(play: DashboardPlay, origin: String, removing: Boolean, onOpen: () -> Unit, onRemove: () -> Unit, modifier: Modifier = Modifier) {
    val row = play.row
    val art = remember(row.artPaths, origin) { ArtworkImage(row.artPaths.map { absolute(it, origin) }) }
    val avatar = remember(play.user.avatarUrl, origin) { play.user.avatarUrl?.takeIf { it.isNotEmpty() }?.let { absolute(it, origin) } }
    Row(
        modifier
            .adminWidth
            .heightIn(min = 72.dp)
            .clip(PleyShape.Card)
            .clickable(role = Role.Button, onClick = onOpen)
            .padding(top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .width(40.dp)
                .height(if (row.square) 40.dp else 60.dp)
                .clip(PleyShape.Badge)
                .background(PleyColors.Surface2),
        ) {
            Artwork(art, row.title, Modifier.matchParentSize())
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(row.title, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val line = listOf(row.subtitle, row.time, row.device.orEmpty()).filter { it.isNotEmpty() }.joinToString(" · ")
            Text(line, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                Avatar(play.user.name, color = play.user.avatarColor, imageUrl = avatar, size = 18.dp)
                Spacer(Modifier.width(6.dp))
                Text(play.user.name, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        PleyIconButton(PleyIcons.X, stringResource(R.string.admin_dashboard_remove), onClick = onRemove, enabled = !removing, tint = PleyColors.Muted)
    }
}

/** [n] written as the language shown writes it: "12,345". */
internal fun number(n: Long, locale: java.util.Locale): String = java.text.NumberFormat.getIntegerInstance(locale).format(n)

/** The server's limit on a stop's message. */
private const val STOP_MESSAGE_MAX = 200

internal const val DASHBOARD_PAGE = "pley.admin.dashboard.page"
internal const val STOP_BUTTON = "pley.admin.dashboard.stop"
internal const val STOP_CONFIRM = "pley.admin.dashboard.stop.confirm"
