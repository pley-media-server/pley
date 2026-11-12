package app.pley.android.ui.phone.admin.dashboard

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.JobState
import app.pley.android.model.PlaybackMethod
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.admin.adminWidth

/*
 * The dashboard's pieces: a section's heading (the web's SectionTitle),
 * a card, a line of a label and its value (InfoRow and Stat), badges,
 * chips and progress bars, in the phone's colours.
 */

/** A section's heading: [icon] in accent, [title], an optional [count] and [action] at the end. */
@Composable
internal fun SectionHeading(
    title: String,
    icon: PleyIcon,
    modifier: Modifier = Modifier,
    count: Int? = null,
    action: (@Composable RowScope.() -> Unit)? = null,
) {
    Row(
        modifier.adminWidth.padding(top = 28.dp, bottom = 8.dp).heightIn(min = 40.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
        Spacer(Modifier.width(8.dp))
        Text(
            title,
            style = MaterialTheme.typography.titleMedium,
            color = PleyColors.Fg,
            modifier = Modifier.semantics { heading() },
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (count != null) {
            Spacer(Modifier.width(8.dp))
            Text(number(count.toLong(), formatWords().locale), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
        }
        Spacer(Modifier.weight(1f))
        if (action != null) action()
    }
}

/** A card of the dashboard: [surface][PleyColors.Surface] with the card's corners. */
@Composable
internal fun DashboardCard(modifier: Modifier = Modifier, padding: Dp = 16.dp, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier
            .adminWidth
            .clip(PleyShape.Card)
            .background(PleyColors.Surface)
            .padding(padding),
        content = content,
    )
}

/** A line that says something in a section with nothing else to show ("Nobody is watching anything right now."). */
@Composable
internal fun QuietLine(text: String, modifier: Modifier = Modifier, danger: Boolean = false) {
    Text(
        text,
        style = MaterialTheme.typography.bodyMedium,
        color = if (danger) PleyColors.DangerText else PleyColors.Subtle,
        textAlign = if (danger) TextAlign.Start else TextAlign.Center,
        modifier = modifier
            .adminWidth
            .clip(PleyShape.Card)
            .background(if (danger) PleyColors.Danger.copy(alpha = 0.1f) else PleyColors.Surface)
            .padding(horizontal = 16.dp, vertical = 20.dp),
    )
}

/**
 * A label and its value on one line, the value's [sub] under it (the web's
 * InfoRow and Stat): "Uptime   2d 4h". [content] replaces the value's text.
 */
@Composable
internal fun InfoLine(
    label: String,
    value: String? = null,
    sub: String? = null,
    mono: Boolean = false,
    content: (@Composable () -> Unit)? = null,
) {
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.Top) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = PleyColors.Subtle,
            modifier = Modifier.width(132.dp).padding(end = 12.dp),
        )
        Column(Modifier.weight(1f)) {
            if (content != null) {
                content()
            } else if (value != null) {
                Text(
                    value,
                    style = if (mono) MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace) else MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Fg,
                )
            }
            if (!sub.isNullOrEmpty()) Text(sub, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        }
    }
}

/** How a badge reads: the web's tones. */
internal enum class Tone(val fg: Color) {
    Success(PleyColors.Success),
    Info(PleyColors.Info),
    Accent(PleyColors.AccentBright),
    Danger(PleyColors.DangerText),
    Neutral(PleyColors.Muted),
}

/** A small label on a tinted pill (the web's Badge); [description] is what screen readers hear instead. */
@Composable
internal fun Badge(text: String, tone: Tone, modifier: Modifier = Modifier, description: String? = null) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = tone.fg,
        maxLines = 1,
        modifier = modifier
            .clip(PleyShape.Control)
            .background(tone.fg.copy(alpha = 0.14f))
            .padding(horizontal = 8.dp, vertical = 2.dp)
            .then(if (description != null) Modifier.semantics { contentDescription = description } else Modifier),
    )
}

/** Names in a row of chips, [active] picked out (the encoders, the tone mappers); [empty] when there are none. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun Chips(items: List<String>, active: String? = null, empty: String? = null) {
    if (items.isEmpty()) {
        Text(empty ?: stringResource(R.string.admin_dashboard_none), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
        return
    }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        for (item in items) {
            val on = item == active
            Text(
                item,
                style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                color = if (on) PleyColors.AccentBright else PleyColors.Fg.copy(alpha = 0.8f),
                modifier = Modifier
                    .clip(PleyShape.Control)
                    .background(if (on) PleyColors.Accent.copy(alpha = 0.15f) else PleyColors.Surface3)
                    .padding(horizontal = 8.dp, vertical = 2.dp),
            )
        }
    }
}

/** A thin bar filled to [fraction], drawn in one pass. */
@Composable
internal fun Bar(fraction: Float, modifier: Modifier = Modifier, height: Dp = 4.dp, color: Color = PleyColors.Accent) {
    Box(
        modifier
            .fillMaxWidth()
            .height(height)
            .drawBehind {
                val r = CornerRadius(size.height / 2, size.height / 2)
                drawRoundRect(PleyColors.Surface4, cornerRadius = r)
                val w = size.width * fraction.coerceIn(0f, 1f)
                if (w > 0f) drawRoundRect(color, size = Size(w, size.height), cornerRadius = r)
            },
    )
}

/** How a playback is delivered, as the web's badge says it. */
@Composable
internal fun MethodBadge(method: PlaybackMethod, modifier: Modifier = Modifier) {
    val (words, tone) = when (method) {
        PlaybackMethod.Direct -> R.string.admin_dashboard_method_direct to Tone.Success
        PlaybackMethod.Remux -> R.string.admin_dashboard_method_remux to Tone.Info
        PlaybackMethod.Transcode -> R.string.admin_dashboard_method_transcode to Tone.Accent
        PlaybackMethod.Unknown -> return
    }
    Badge(stringResource(words), tone, modifier)
}

/** Where a transcode's frames are processed (zero-copy on a GPU, mixed, CPU only, copied); nothing for a pipeline it doesn't know. */
@Composable
internal fun PipelineBadge(pipeline: String?, modifier: Modifier = Modifier) {
    val (label, title, tone) = when (pipeline) {
        "gpu" -> Triple(R.string.admin_dashboard_pipeline_gpu, R.string.admin_dashboard_pipeline_gpu_title, Tone.Success)
        "hybrid" -> Triple(R.string.admin_dashboard_pipeline_hybrid, R.string.admin_dashboard_pipeline_hybrid_title, Tone.Info)
        "cpu" -> Triple(R.string.admin_dashboard_pipeline_cpu, R.string.admin_dashboard_pipeline_cpu_title, Tone.Neutral)
        "copy" -> Triple(R.string.admin_dashboard_pipeline_copy, R.string.admin_dashboard_pipeline_copy_title, Tone.Neutral)
        else -> return
    }
    Badge(stringResource(label), tone, modifier, description = stringResource(title))
}

/** A transcode job's state, as the web's badge says it. */
@Composable
internal fun JobStateBadge(state: JobState) {
    val (words, tone) = when (state) {
        JobState.Starting -> R.string.admin_dashboard_job_starting to Tone.Info
        JobState.Running -> R.string.admin_dashboard_job_running to Tone.Success
        JobState.Idle -> R.string.admin_dashboard_job_idle to Tone.Neutral
        JobState.Finished -> R.string.admin_dashboard_job_finished to Tone.Neutral
        JobState.Failed -> R.string.admin_dashboard_job_failed to Tone.Danger
        JobState.Unknown -> return
    }
    Badge(stringResource(words), tone)
}
