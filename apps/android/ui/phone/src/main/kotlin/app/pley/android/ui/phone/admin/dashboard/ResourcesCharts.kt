package app.pley.android.ui.phone.admin.dashboard

import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.admin.DashboardResources
import app.pley.android.model.admin.ResourceSample
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.formatBytes
import app.pley.android.screens.admin.dashboard.percentText
import app.pley.android.ui.phone.R
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import kotlin.math.abs

/*
 * The processors and the memory over the last five minutes, as the web's
 * ResourcesSection draws them: Pley's line in front of the whole
 * machine's, broken where samples are missing. A finger dragged along a
 * chart reads the sample under it, as the web's pointer does.
 */

/** What the charts span, as the server keeps it. */
private const val WINDOW_MS = 5 * 60_000L

/** The whole machine, behind Pley's line, in the web's gray. */
private val SYSTEM_LINE = Color(0xFF7A8292)

/** One line of a chart. */
private class Series(
    val label: String,
    val color: Color,
    /** 0 to 1 of the chart's height. */
    val value: (ResourceSample) -> Float,
    /** As the legend says it. */
    val text: (ResourceSample) -> String,
)

/** The two charts, CPU then memory. */
@Composable
internal fun ResourcesCharts(data: DashboardResources) {
    val words = formatWords()
    val pley = "Pley"
    val system = stringResource(R.string.admin_dashboard_machine)
    val total = data.memoryTotal.coerceAtLeast(1).toFloat()
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Chart(
            title = stringResource(R.string.admin_dashboard_cpu),
            detail = pluralStringResource(R.plurals.admin_dashboard_processors, data.cpus, data.cpus),
            data = data,
            words = words,
            series = listOf(
                Series(pley, PleyColors.Accent, { (it.pleyCpu / 100).toFloat() }, { percentText(it.pleyCpu, words.locale) }),
                Series(system, SYSTEM_LINE, { (it.cpu / 100).toFloat() }, { percentText(it.cpu, words.locale) }),
            ),
        )
        Chart(
            title = stringResource(R.string.admin_dashboard_memory),
            detail = stringResource(R.string.admin_dashboard_in_all, formatBytes(data.memoryTotal, words)),
            data = data,
            words = words,
            series = listOf(
                Series(pley, PleyColors.Accent, { it.pleyMemory / total }, { formatBytes(it.pleyMemory, words) }),
                Series(system, SYSTEM_LINE, { it.memory / total }, { formatBytes(it.memory, words) }),
            ),
        )
        Text(stringResource(R.string.admin_dashboard_pley_hint), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle)
    }
}

@Composable
private fun Chart(title: String, detail: String, data: DashboardResources, words: FormatWords, series: List<Series>) {
    // The sample pointed at, by its time: new samples move the others along.
    var pointedAt by remember { mutableStateOf<Long?>(null) }
    val samples = data.samples
    val latest = samples.lastOrNull()
    val pointed = pointedAt?.let { at -> samples.firstOrNull { it.at == at } }
    val shown = pointed ?: latest
    val end = latest?.at ?: 0L
    val start = end - WINDOW_MS
    val gapMs = maxOf(data.intervalMs, 1_000L) * 3
    // The gesture outlives the samples it started on.
    val currentSamples by rememberUpdatedState(samples)
    val currentStart by rememberUpdatedState(start)
    val summary = if (latest != null) {
        stringResource(R.string.admin_dashboard_summary, title, series.joinToString(", ") { "${it.label} ${it.text(latest)}" })
    } else {
        stringResource(R.string.admin_dashboard_no_readings, title)
    }
    val timeFormat = remember(words.locale) { DateTimeFormatter.ofLocalizedTime(FormatStyle.MEDIUM).withLocale(words.locale) }

    DashboardCard {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(title, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, modifier = Modifier.weight(1f))
            Text(
                if (pointed != null) timeFormat.format(Instant.ofEpochMilli(pointed.at).atZone(ZoneId.systemDefault())) else detail,
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Subtle,
            )
        }
        Row(Modifier.padding(top = 8.dp), horizontalArrangement = Arrangement.spacedBy(20.dp)) {
            for (s in series) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    LineKey(s.color)
                    Spacer(Modifier.width(8.dp))
                    Text(s.label, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
                    Spacer(Modifier.width(8.dp))
                    Text(shown?.let(s.text) ?: "—", style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg)
                }
            }
        }
        Box(
            Modifier
                .padding(top = 20.dp)
                .fillMaxWidth()
                .height(128.dp)
                .semantics { contentDescription = summary }
                .pointerInput(Unit) {
                    // Horizontal only, so the page still scrolls when a finger goes up or down over a chart.
                    fun nearest(x: Float): Long? {
                        val list = currentSamples
                        if (list.isEmpty() || size.width == 0) return null
                        val at = currentStart + (x / size.width * WINDOW_MS).toLong()
                        return list.minByOrNull { abs(it.at - at) }?.at
                    }
                    detectHorizontalDragGestures(
                        onDragStart = { pointedAt = nearest(it.x) },
                        onDragEnd = { pointedAt = null },
                        onDragCancel = { pointedAt = null },
                        onHorizontalDrag = { change, _ -> pointedAt = nearest(change.position.x) },
                    )
                }
                .drawWithCache {
                    val w = size.width
                    val h = size.height
                    fun x(at: Long) = ((at - start).toFloat() / WINDOW_MS) * w
                    fun y(v: Float) = h - v.coerceIn(0f, 1f) * h
                    // Back to front: the machine's line, then Pley's over it.
                    val drawn = series.asReversed().map { s ->
                        val points = samples.map { Offset(x(it.at), y(s.value(it))) }
                        Triple(s.color, linePath(points, samples, gapMs), areaPath(points, samples, gapMs, h))
                    }
                    val grid = 1.dp.toPx()
                    val stroke = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round)
                    onDrawBehind {
                        for (v in listOf(0f, 0.5f, 1f)) drawLine(PleyColors.Surface4, Offset(0f, y(v)), Offset(w, y(v)), grid)
                        for ((color, line, area) in drawn) {
                            drawPath(area, color.copy(alpha = 0.1f))
                            drawPath(line, color, style = stroke)
                        }
                    }
                }
                .drawBehind {
                    val at = pointed ?: return@drawBehind
                    val x = ((at.at - start).toFloat() / WINDOW_MS) * size.width
                    drawLine(PleyColors.Muted, Offset(x, 0f), Offset(x, size.height), 1.dp.toPx())
                    for (s in series) {
                        val y = size.height - s.value(at).coerceIn(0f, 1f) * size.height
                        drawCircle(PleyColors.Surface, 6.dp.toPx(), Offset(x, y))
                        drawCircle(s.color, 4.dp.toPx(), Offset(x, y))
                    }
                },
        ) {
            // Each just above its line, out of the lines' way.
            Text(
                percentText(100.0, words.locale),
                style = MaterialTheme.typography.labelSmall,
                color = PleyColors.Subtle,
                modifier = Modifier.align(Alignment.TopEnd).padding(top = 2.dp),
            )
            Text(
                percentText(50.0, words.locale),
                style = MaterialTheme.typography.labelSmall,
                color = PleyColors.Subtle,
                modifier = Modifier.align(Alignment.CenterEnd).padding(bottom = 18.dp),
            )
        }
        Row(Modifier.fillMaxWidth().padding(top = 6.dp)) {
            Text(stringResource(R.string.admin_dashboard_five_minutes_ago), style = MaterialTheme.typography.labelSmall, color = PleyColors.Subtle)
            Spacer(Modifier.weight(1f))
            Text(stringResource(R.string.admin_dashboard_right_now), style = MaterialTheme.typography.labelSmall, color = PleyColors.Subtle)
        }
    }
}

@Composable
private fun LineKey(color: Color) {
    Box(Modifier.size(12.dp, 2.dp).drawBehind { drawRoundRect(color, cornerRadius = androidx.compose.ui.geometry.CornerRadius(size.height)) })
}

/** The line through [points], broken where samples are further apart than [gapMs] (the server was busy or restarted). */
private fun linePath(points: List<Offset>, samples: List<ResourceSample>, gapMs: Long): Path {
    val path = Path()
    points.forEachIndexed { i, p ->
        val joined = i > 0 && samples[i].at - samples[i - 1].at <= gapMs
        if (joined) path.lineTo(p.x, p.y) else path.moveTo(p.x, p.y)
    }
    return path
}

/** The area under each unbroken run of [points], down to [bottom]. */
private fun areaPath(points: List<Offset>, samples: List<ResourceSample>, gapMs: Long, bottom: Float): Path {
    val path = Path()
    var from = 0
    for (i in 1..points.size) {
        val broken = i == points.size || samples[i].at - samples[i - 1].at > gapMs
        if (!broken) continue
        if (i - from > 1) {
            path.moveTo(points[from].x, points[from].y)
            for (j in from + 1 until i) path.lineTo(points[j].x, points[j].y)
            path.lineTo(points[i - 1].x, bottom)
            path.lineTo(points[from].x, bottom)
            path.close()
        }
        from = i
    }
    return path
}
