package app.pley.android.ui.phone.admin.livetv

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.text.shownLocale
import app.pley.android.model.LivePlan
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.livetv.LineupGroupsViewModel
import app.pley.android.screens.admin.livetv.LineupViewModel
import app.pley.android.screens.admin.livetv.LiveTvAdminViewModel
import app.pley.android.screens.admin.livetv.codecName
import app.pley.android.screens.admin.livetv.encoderName
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import java.text.NumberFormat

/*
 * The pieces Live TV's pages share: the rows that say something rather
 * than open it, and the lines the web's
 * livetv/common.ts writes (what the transcoder makes of a channel, a rate,
 * how long a channel has been pulled).
 */

/**
 * A row that says something rather than opens it: [label] and its
 * [value], [note] under them; a [danger] row reads in
 * [dangerText][PleyColors.DangerText] (a source's error).
 */
@Composable
internal fun InfoRow(
    label: String,
    value: String?,
    modifier: Modifier = Modifier,
    note: String? = null,
    icon: PleyIcon? = null,
    danger: Boolean = false,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    Row(
        modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, size = 24.dp, tint = if (danger) PleyColors.DangerText else PleyColors.Muted)
            Spacer(Modifier.width(16.dp))
        }
        Column(Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodyLarge, color = if (danger) PleyColors.DangerText else PleyColors.Fg)
            if (!value.isNullOrEmpty()) {
                Text(value, style = MaterialTheme.typography.bodyMedium, color = if (danger) PleyColors.DangerText else PleyColors.Muted)
            }
            if (!note.isNullOrEmpty()) Text(note, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
        }
        if (trailing != null) {
            Spacer(Modifier.width(12.dp))
            trailing()
        }
    }
}

/**
 * A page of Live TV whose list is its own, row after row (groups,
 * channels): the top bar, [header] (the counts, the search, the filters)
 * once [load] is read, then [content]; a spinner while it reads, the
 * error with Retry when it couldn't.
 */
@Composable
internal fun <T> LiveListPage(
    title: String,
    load: AdminLoad<T>,
    onRetry: () -> Unit,
    tag: String,
    list: LazyListState = rememberLazyListState(),
    actions: @Composable RowScope.() -> Unit = {},
    header: @Composable ColumnScope.(T) -> Unit = {},
    content: LazyListScope.(T) -> Unit,
) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    Column(Modifier.fillMaxSize().testTag(tag)) {
        PleyTopAppBar(title = title, onBack = navigator::back, scrolled = { list.canScrollBackward }, actions = actions)
        val value = when (load) {
            is AdminLoad.Ready -> load.value
            is AdminLoad.Failed -> load.stale
            AdminLoad.Loading -> null
        }
        when {
            value != null -> {
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    Column(Modifier.adminWidth) { header(value) }
                }
                LazyColumn(
                    state = list,
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, bottom = 32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) { content(value) }
            }
            load is AdminLoad.Failed -> ErrorState(stringResource(R.string.admin_load_failed), load.error, onRetry)
            else -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                DelayedSpinner(stringResource(R.string.admin_loading))
            }
        }
    }
}

/** A line under a page's title: what it is for, or its counts. */
@Composable
internal fun PageLine(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = MaterialTheme.typography.bodyMedium,
        color = PleyColors.Muted,
        modifier = modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp),
        maxLines = 6,
        overflow = TextOverflow.Ellipsis,
    )
}

/** What the transcoder makes of a channel: "1080i MPEG-2 → H.264 on NVIDIA, deinterlaced · MP2 → AAC". */
@Composable
internal fun planLine(plan: LivePlan): String {
    val lines = plan.height?.let { "$it${if (plan.interlaced) "i" else "p"}" }
    val source = listOfNotNull(lines, codecName(plan.videoCodec).ifEmpty { null }).joinToString(" ")
    val encoder = encoderName(plan.video) ?: stringResource(R.string.admin_livetv_plan_processor)
    val video = when (plan.video) {
        "none" -> stringResource(R.string.admin_livetv_plan_radio)
        "copy" -> stringResource(R.string.admin_livetv_plan_as_it_is, source)
        else -> stringResource(
            if (plan.interlaced) R.string.admin_livetv_plan_transcoded_deinterlaced else R.string.admin_livetv_plan_transcoded,
            source,
            encoder,
        )
    }
    val audioIn = codecName(plan.audioCodec)
    val audio = when (plan.audio) {
        "none" -> ""
        "copy" -> stringResource(R.string.admin_livetv_plan_as_it_is, audioIn)
        else -> "$audioIn → AAC"
    }
    return listOf(video, audio).filter { it.isNotEmpty() }.joinToString(" · ")
}

/** "4.2 Mbit/s", "800 kbit/s". */
@Composable
internal fun rateLine(kbps: Long): String =
    if (kbps >= 1000) {
        stringResource(R.string.admin_livetv_rate_mbps, String.format(shownLocale(), "%.1f", kbps / 1000.0))
    } else {
        stringResource(R.string.admin_livetv_rate_kbps, NumberFormat.getIntegerInstance(shownLocale()).format(kbps))
    }

/** "for 12 min", since [since] (unix seconds). */
@Composable
internal fun forLine(since: Long, nowSeconds: Long = System.currentTimeMillis() / 1000): String {
    val minutes = ((nowSeconds - since) / 60).coerceAtLeast(0).toInt()
    return when {
        minutes < 1 -> stringResource(R.string.admin_livetv_since_just_now)
        minutes < 60 -> stringResource(R.string.admin_livetv_since_minutes, minutes)
        else -> stringResource(R.string.admin_livetv_since_hours_minutes, minutes / 60, minutes % 60)
    }
}

/** The words of what Live TV's pages confirm ([AdminDone]). */
@Composable
internal fun liveTvSaid(): (AdminDone) -> String? {
    val resources = LocalResources.current
    return { done ->
        val arg = done.args.firstOrNull()
        when (done.what) {
            LiveTvAdminViewModel.STOPPED -> resources.getString(R.string.admin_livetv_tuner_stopped, arg)
            LineupGroupsViewModel.REGION_HIDDEN -> resources.getString(R.string.admin_livetv_groups_region_hidden, arg)
            LineupGroupsViewModel.REGION_SHOWN -> resources.getString(R.string.admin_livetv_groups_region_shown, arg)
            LineupGroupsViewModel.GROUP_HIDDEN -> resources.getString(R.string.admin_livetv_groups_group_hidden, arg)
            LineupGroupsViewModel.GROUP_SHOWN -> resources.getString(R.string.admin_livetv_groups_group_shown, arg)
            LineupGroupsViewModel.UNNAMED_HIDDEN -> resources.getString(R.string.admin_livetv_groups_unnamed_hidden)
            LineupGroupsViewModel.UNNAMED_SHOWN -> resources.getString(R.string.admin_livetv_groups_unnamed_shown)
            LineupViewModel.HIDDEN -> (arg as Int).let { resources.getQuantityString(R.plurals.admin_livetv_lineup_hidden, it, it) }
            LineupViewModel.SHOWN -> (arg as Int).let { resources.getQuantityString(R.plurals.admin_livetv_lineup_shown, it, it) }
            LineupViewModel.SAVED -> resources.getString(R.string.admin_livetv_editor_saved)
            else -> null
        }
    }
}
