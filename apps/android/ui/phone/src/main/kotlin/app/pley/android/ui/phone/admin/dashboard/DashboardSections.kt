package app.pley.android.ui.phone.admin.dashboard

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.common.errorSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.ScanPhase
import app.pley.android.model.StreamAction
import app.pley.android.model.admin.DashboardActivity
import app.pley.android.model.admin.DashboardLibraryWork
import app.pley.android.model.admin.DashboardResources
import app.pley.android.model.admin.DashboardScan
import app.pley.android.model.admin.DashboardSystem
import app.pley.android.model.admin.DashboardTranscoderInfo
import app.pley.android.model.admin.DashboardTranscoderJob
import app.pley.android.model.admin.DashboardTranscoderStatus
import app.pley.android.model.admin.DatabaseHealth
import app.pley.android.model.admin.StorageUsage
import app.pley.android.model.format.formatBytes
import app.pley.android.model.format.formatTimestamp
import app.pley.android.model.format.formatUptime
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.screens.admin.dashboard.oneDecimal
import app.pley.android.screens.admin.dashboard.versionLabel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.admin.adminWidth
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyTextButton

/*
 * The dashboard's sections below what plays and everyone's plays: the
 * machine's load, the transcoder's jobs, the libraries' work, the server
 * and the storage, each as the web's.
 */

/** The charts, or why there are none. */
@Composable
internal fun ResourcesBody(load: AdminLoad<DashboardResources>) {
    Box(Modifier.adminWidth) {
        when (load) {
            AdminLoad.Loading -> Loading()
            // As the web: a reading that fails says so, rather than show charts that stopped.
            is AdminLoad.Failed -> QuietLine(stringResource(R.string.admin_dashboard_couldnt_read, errorSentence(load.error).asString()), danger = true)
            is AdminLoad.Ready -> if (load.value.available) {
                ResourcesCharts(load.value)
            } else {
                QuietLine(stringResource(R.string.admin_dashboard_unmeasurable))
            }
        }
    }
}

/** The transcoder's live numbers and its jobs, as the web's TranscoderLive. */
internal fun LazyListScope.transcoderLive(load: AdminLoad<DashboardTranscoderStatus>) {
    item(key = "transcoder", contentType = "heading") { SectionHeading(stringResource(R.string.admin_dashboard_transcoder), PleyIcons.Cpu) }
    when (load) {
        AdminLoad.Loading -> item(key = "transcoder.loading", contentType = "loading") { Loading() }
        is AdminLoad.Failed -> item(key = "transcoder.failed", contentType = "line") {
            QuietLine(stringResource(R.string.admin_dashboard_transcoder_unavailable, errorSentence(load.error).asString()), danger = true)
        }
        is AdminLoad.Ready -> {
            val status = load.value
            item(key = "transcoder.numbers", contentType = "card") { TranscoderNumbers(status) }
            if (status.jobs.isEmpty()) {
                item(key = "transcoder.none", contentType = "line") {
                    QuietLine(stringResource(R.string.admin_dashboard_no_jobs), Modifier.padding(top = 8.dp))
                }
            } else {
                items(status.jobs, key = { "job:" + it.id }, contentType = { "job" }) { job -> JobCard(job) }
            }
        }
    }
}

@Composable
private fun TranscoderNumbers(status: DashboardTranscoderStatus) {
    val words = formatWords()
    val c = status.counters
    val locale = words.locale
    DashboardCard {
        InfoLine(
            stringResource(R.string.admin_dashboard_running_jobs),
            "${status.runningProcesses} / ${status.maxJobs}",
            pluralStringResource(R.plurals.admin_dashboard_created, c.jobsCreated.toQuantity(), number(c.jobsCreated, locale)),
        )
        InfoLine(stringResource(R.string.admin_dashboard_hw_sessions), "${status.hwProcesses} / ${status.maxHwJobs}")
        InfoLine(
            stringResource(R.string.admin_dashboard_segments_served),
            number(c.segmentsServed, locale),
            pluralStringResource(R.plurals.admin_dashboard_produced, c.segmentsProduced.toQuantity(), number(c.segmentsProduced, locale)),
        )
        InfoLine(
            stringResource(R.string.admin_dashboard_data_served),
            formatBytes(c.bytesServed, words),
            stringResource(R.string.admin_dashboard_on_disk, formatBytes(status.workDirBytes, words)),
        )
        InfoLine(
            stringResource(R.string.admin_dashboard_probes),
            number(c.probes, locale),
            if (c.probes > 0) stringResource(R.string.admin_dashboard_average, Math.round(c.probeMsTotal.toDouble() / c.probes).toInt()) else null,
        )
        InfoLine(
            stringResource(R.string.admin_dashboard_failures),
            number(c.jobsFailed + c.processFailures + c.probeFailures, locale),
            stringResource(R.string.admin_dashboard_thumbs_and_subs, number(c.thumbnails, locale), number(c.subtitleExtractions, locale)),
        )
    }
}

/** One of the transcoder's jobs: the web's table row, as a card. */
@Composable
private fun JobCard(job: DashboardTranscoderJob) {
    val words = formatWords()
    val i = job.info
    DashboardCard(Modifier.padding(top = 8.dp), padding = 12.dp) {
        Row(verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f)) {
                Text(job.label ?: job.id, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 2, overflow = TextOverflow.Ellipsis)
                val about = listOfNotNull(
                    job.owner ?: "—",
                    if (job.restarts > 0) pluralStringResource(R.plurals.admin_dashboard_restarts, job.restarts, job.restarts) else null,
                    if (job.servedBytes > 0) formatBytes(job.servedBytes, words) else null,
                ).joinToString(" · ")
                Text(about, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Spacer(Modifier.width(8.dp))
            JobStateBadge(job.state)
        }
        i.lastError?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.DangerText, modifier = Modifier.padding(top = 4.dp)) }
        if (job.warnings.isNotEmpty()) {
            Text(job.warnings.joinToString("; "), style = MaterialTheme.typography.bodySmall, color = PleyColors.AccentBright, modifier = Modifier.padding(top = 4.dp))
        }
        Column(Modifier.padding(top = 6.dp)) {
            InfoLine(stringResource(R.string.admin_dashboard_job_video), streamText(i.video, i.videoIn, i.videoOut))
            InfoLine(stringResource(R.string.admin_dashboard_job_audio), streamText(i.audio, i.audioIn, i.audioOut))
            InfoLine(stringResource(R.string.admin_dashboard_job_codecs)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        i.encoder ?: if (i.video == StreamAction.Copy) "copy" else "—",
                        style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                        color = PleyColors.Fg,
                    )
                    Spacer(Modifier.width(6.dp))
                    PipelineBadge(i.pipeline)
                }
                i.preset?.let { Text(stringResource(R.string.admin_dashboard_preset, it), style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle) }
                i.decoder?.let { Text(it, style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace), color = PleyColors.Subtle) }
            }
            InfoLine(stringResource(R.string.admin_dashboard_job_tone_map), i.tonemap ?: "—", mono = true)
            if (i.subtitleBurn) InfoLine(stringResource(R.string.admin_dashboard_burn_in), "✓")
            InfoLine(stringResource(R.string.admin_dashboard_job_speed), i.speed?.let { "${oneDecimal(it, words)}×" } ?: "—")
            InfoLine(stringResource(R.string.admin_dashboard_job_fps), i.fps?.let { Math.round(it).toString() } ?: "—")
            InfoLine(stringResource(R.string.admin_dashboard_job_segments)) {
                if (i.segmentsTotal > 0) {
                    Text("${number(i.segmentsReady.toLong(), words.locale)} / ${number(i.segmentsTotal.toLong(), words.locale)}", style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg)
                    Bar(i.segmentsReady.toFloat() / i.segmentsTotal, Modifier.padding(top = 4.dp))
                } else {
                    Text("—", style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
                }
            }
        }
    }
}

/** What a job does with a stream: "—", "EAC3 5.1 (copy)", "HEVC → H.264". */
@Composable
private fun streamText(action: StreamAction, input: String?, output: String?): String = when (action) {
    StreamAction.None -> "—"
    StreamAction.Copy -> stringResource(R.string.admin_dashboard_copied, input.orEmpty())
    else -> "${input ?: "?"} → ${output ?: "?"}"
}

/** The libraries' scans, intro and credits detection, and seek previews, while any goes on. */
internal fun LazyListScope.libraryWork(activity: DashboardActivity?) {
    if (activity == null) return
    val scans = activity.scans.filter { it.phase != ScanPhase.Done }
    if (scans.isNotEmpty()) {
        item(key = "scans", contentType = "heading") { SectionHeading(stringResource(R.string.admin_dashboard_library_scans), PleyIcons.RefreshCw) }
        items(scans, key = { "scan:" + it.libraryId }, contentType = { "scan" }) { scan -> ScanCard(scan) }
    }
    val analysis = activity.analysis
    if (analysis != null || activity.seasonsPending > 0) {
        item(key = "markers", contentType = "heading") { SectionHeading(stringResource(R.string.admin_dashboard_finding_markers), PleyIcons.FastForward) }
        item(key = "markers.body", contentType = "card") {
            val words = formatWords()
            val waits = waitsText(activity.libraryWork)
            DashboardCard {
                if (analysis != null) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(analysis.title, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg, modifier = Modifier.weight(1f))
                        Text(
                            stringResource(R.string.admin_dashboard_fingerprints, analysis.done, analysis.total),
                            style = MaterialTheme.typography.bodySmall,
                            color = PleyColors.Muted,
                        )
                    }
                    Bar(if (analysis.total > 0) analysis.done.toFloat() / analysis.total else 0f, Modifier.padding(top = 6.dp), height = 6.dp)
                }
                if (activity.seasonsPending > 0) {
                    val left = pluralStringResource(R.plurals.admin_dashboard_seasons_left, activity.seasonsPending, number(activity.seasonsPending.toLong(), words.locale))
                    Text(
                        listOf(left, waits).filter { it.isNotEmpty() }.joinToString(" · "),
                        style = MaterialTheme.typography.bodyMedium,
                        color = PleyColors.Muted,
                        modifier = Modifier.padding(top = if (analysis != null) 8.dp else 0.dp),
                    )
                }
            }
        }
    }
    if (activity.previewsPending > 0) {
        item(key = "previews", contentType = "heading") { SectionHeading(stringResource(R.string.admin_dashboard_making_previews), PleyIcons.Images) }
        item(key = "previews.body", contentType = "card") {
            val words = formatWords()
            val parts = listOf(
                pluralStringResource(R.plurals.admin_dashboard_files_left, activity.previewsPending, number(activity.previewsPending.toLong(), words.locale)),
                if (activity.previewsRunning > 1) stringResource(R.string.admin_dashboard_at_a_time, activity.previewsRunning) else "",
                waitsText(activity.libraryWork),
            )
            DashboardCard {
                Text(parts.filter { it.isNotEmpty() }.joinToString(" · "), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            }
        }
    }
}

@Composable
private fun ScanCard(scan: DashboardScan) {
    val words = formatWords()
    val phase = when (scan.phase) {
        ScanPhase.Queued -> R.string.admin_dashboard_scan_queued
        ScanPhase.Walking -> R.string.admin_dashboard_scan_walking
        ScanPhase.Probing -> R.string.admin_dashboard_scan_probing
        ScanPhase.Metadata -> R.string.admin_dashboard_scan_metadata
        else -> R.string.admin_dashboard_scan_done
    }
    DashboardCard(Modifier.padding(bottom = 8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(scan.libraryName, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg, modifier = Modifier.weight(1f))
            val counted = if (scan.total > 0) " · ${number(scan.processed.toLong(), words.locale)} / ${number(scan.total.toLong(), words.locale)}" else ""
            Text(stringResource(phase) + counted, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        }
        Bar(if (scan.total > 0) scan.processed.toFloat() / scan.total else 0f, Modifier.padding(top = 6.dp), height = 6.dp)
    }
}

/** Why the libraries' intros, credits and previews wait, if they do, as the web says it. */
@Composable
private fun waitsText(work: DashboardLibraryWork?): String {
    val words = formatWords()
    return when (work?.waiting) {
        "playing" -> stringResource(R.string.admin_dashboard_waits_playing)
        "busy" -> stringResource(R.string.admin_dashboard_waits_busy)
        "hours" -> {
            val auto = work.mode == "auto"
            val from = work.resumesAt?.let { formatTimestamp(it, words.locale) }
            when {
                from != null && auto -> stringResource(R.string.admin_dashboard_waits_auto_from, from)
                from != null -> stringResource(R.string.admin_dashboard_waits_hours_from, from)
                auto -> stringResource(R.string.admin_dashboard_waits_auto)
                else -> stringResource(R.string.admin_dashboard_waits_hours)
            }
        }
        else -> ""
    }
}

/** The server, what its libraries hold, and its transcoder, as the web's System section. */
internal fun LazyListScope.systemInfo(load: AdminLoad<DashboardSystem>, onRetry: () -> Unit) {
    item(key = "system", contentType = "heading") { SectionHeading(stringResource(R.string.admin_dashboard_system), PleyIcons.Activity) }
    val system = when (load) {
        is AdminLoad.Ready -> load.value
        is AdminLoad.Failed -> load.stale
        AdminLoad.Loading -> null
    }
    when {
        system != null -> {
            item(key = "system.server", contentType = "card") { ServerCard(system) }
            item(key = "system.counts", contentType = "card") { CountsCard(system) }
            item(key = "system.transcoder", contentType = "card") { TranscoderCard(system.transcoder) }
        }
        load is AdminLoad.Failed -> item(key = "system.failed", contentType = "error") {
            Box(Modifier.adminWidth) { ErrorState(stringResource(R.string.admin_dashboard_couldnt_load_system), load.error, onRetry) }
        }
        else -> item(key = "system.loading", contentType = "loading") { Loading() }
    }
}

@Composable
private fun ServerCard(system: DashboardSystem) {
    val words = formatWords()
    DashboardCard {
        Text(system.name, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, modifier = Modifier.padding(bottom = 6.dp))
        InfoLine(stringResource(R.string.admin_dashboard_version), "Pley " + versionLabel(system.version, system.commit))
        InfoLine(stringResource(R.string.admin_dashboard_platform), "${system.os} · ${system.arch}")
        InfoLine(stringResource(R.string.admin_dashboard_uptime), formatUptime(system.uptimeSecs, words))
        InfoLine(stringResource(R.string.admin_dashboard_data_directory), system.dataDir, mono = true)
    }
}

@Composable
private fun CountsCard(system: DashboardSystem) {
    val locale = formatWords().locale
    val c = system.counts
    DashboardCard(Modifier.padding(top = 8.dp)) {
        InfoLine(stringResource(R.string.admin_dashboard_movies), number(c.movies.toLong(), locale))
        InfoLine(
            stringResource(R.string.admin_dashboard_shows),
            number(c.shows.toLong(), locale),
            pluralStringResource(R.plurals.admin_dashboard_episodes, c.episodes, number(c.episodes.toLong(), locale)),
        )
        InfoLine(
            stringResource(R.string.admin_dashboard_artists),
            number(c.artists.toLong(), locale),
            pluralStringResource(R.plurals.admin_dashboard_albums, c.albums, number(c.albums.toLong(), locale)),
        )
        InfoLine(stringResource(R.string.admin_dashboard_tracks), number(c.tracks.toLong(), locale))
        InfoLine(stringResource(R.string.admin_dashboard_files), number(c.files.toLong(), locale))
        InfoLine(
            stringResource(R.string.admin_dashboard_users),
            number(c.users.toLong(), locale),
            pluralStringResource(R.plurals.admin_dashboard_libraries, c.libraries, number(c.libraries.toLong(), locale)),
        )
    }
}

/** The transcoder as the server found it, as the web's TranscoderCard. */
@Composable
private fun TranscoderCard(tc: DashboardTranscoderInfo) {
    val available = stringResource(if (tc.available) R.string.admin_dashboard_available else R.string.admin_dashboard_unavailable)
    DashboardCard(Modifier.padding(top = 8.dp)) {
        Row(Modifier.padding(bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier
                    .size(10.dp)
                    .clip(CircleShape)
                    .background(if (tc.available) PleyColors.Success else PleyColors.Danger)
                    .semantics { contentDescription = available },
            )
            Spacer(Modifier.width(10.dp))
            Text(stringResource(R.string.admin_dashboard_transcoder), style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg)
            Spacer(Modifier.width(10.dp))
            Text(
                stringResource(if (tc.managed) R.string.admin_dashboard_managed else R.string.admin_dashboard_external),
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Muted,
            )
        }
        tc.error?.let { QuietLine(it, Modifier.padding(bottom = 8.dp), danger = true) }
        InfoLine(stringResource(R.string.admin_dashboard_status), available)
        tc.version?.let { InfoLine(stringResource(R.string.admin_dashboard_version), "pley-transcoder " + versionLabel(it, tc.commit)) }
        tc.url?.let { InfoLine(stringResource(R.string.admin_dashboard_url), it, mono = true) }
        InfoLine(stringResource(R.string.admin_dashboard_ffmpeg), tc.ffmpegVersion ?: "—")
        InfoLine(stringResource(R.string.admin_dashboard_encoders)) { Chips(tc.encoders, active = tc.activeEncoder) }
        InfoLine(stringResource(R.string.admin_dashboard_active_encoder), tc.activeEncoder ?: "—", mono = true)
        InfoLine(stringResource(R.string.admin_dashboard_hw_accel)) { Chips(tc.hwaccels) }
        InfoLine(stringResource(R.string.admin_dashboard_tone_mapping)) {
            Chips(tc.tonemapping, empty = stringResource(R.string.admin_dashboard_not_available))
        }
        InfoLine(stringResource(R.string.admin_dashboard_dolby_vision)) {
            if (tc.dolbyVision) {
                Badge(stringResource(R.string.admin_dashboard_supported), Tone.Success)
            } else {
                Badge(stringResource(R.string.admin_dashboard_not_supported), Tone.Neutral)
            }
        }
        InfoLine(stringResource(R.string.admin_dashboard_jobs), stringResource(R.string.admin_dashboard_job_count, tc.activeJobs, tc.maxJobs))
    }
}

/**
 * The space Pley's stores take and the database's health, as the web's
 * StorageSection; its backups stay in the web app.
 */
internal fun LazyListScope.storage(
    usage: AdminLoad<StorageUsage>,
    health: DatabaseHealth?,
    busy: Boolean,
    onClean: () -> Unit,
) {
    item(key = "storage", contentType = "heading") {
        SectionHeading(stringResource(R.string.admin_dashboard_storage), PleyIcons.HardDrive) {
            PleyTextButton(stringResource(R.string.admin_dashboard_clean_up), onClick = onClean, enabled = !busy)
        }
    }
    if (health?.ok == false) {
        item(key = "storage.damaged", contentType = "card") {
            Row(
                Modifier
                    .adminWidth
                    .padding(bottom = 8.dp)
                    .clip(PleyShape.Card)
                    .background(PleyColors.Danger.copy(alpha = 0.1f))
                    .padding(16.dp),
            ) {
                Icon(PleyIcons.TriangleAlert, contentDescription = null, size = 18.dp, tint = PleyColors.DangerText)
                Spacer(Modifier.width(12.dp))
                Column {
                    Text(stringResource(R.string.admin_dashboard_damaged), style = MaterialTheme.typography.bodyMedium, color = PleyColors.DangerText)
                    Text(
                        stringResource(R.string.admin_dashboard_damaged_hint, health.problem.orEmpty()),
                        style = MaterialTheme.typography.bodySmall,
                        color = PleyColors.Muted,
                    )
                }
            }
        }
    }
    item(key = "storage.usage", contentType = "card") {
        when (usage) {
            AdminLoad.Loading -> Loading()
            is AdminLoad.Failed -> usage.stale?.let { UsageCard(it) }
                ?: QuietLine(errorSentence(usage.error).asString(), danger = true)
            is AdminLoad.Ready -> UsageCard(usage.value)
        }
    }
}

@Composable
private fun UsageCard(usage: StorageUsage) {
    val words = formatWords()
    DashboardCard {
        InfoLine(
            stringResource(R.string.admin_dashboard_pictures),
            formatBytes(usage.blobBytes, words),
            pluralStringResource(R.plurals.admin_dashboard_stored_once, usage.blobs, number(usage.blobs.toLong(), words.locale)),
        )
        InfoLine(
            stringResource(R.string.admin_dashboard_generated),
            formatBytes(usage.mediaBytes, words),
            pluralStringResource(R.plurals.admin_dashboard_previews_of, usage.mediaFolders, number(usage.mediaFolders.toLong(), words.locale)),
        )
        InfoLine(stringResource(R.string.admin_dashboard_cache), formatBytes(usage.cacheBytes, words), stringResource(R.string.admin_dashboard_safe_to_delete))
    }
}

/** A count's plural form for a [Long]: as many as an Int holds, which is all that changes the form. */
private fun Long.toQuantity(): Int = coerceAtMost(Int.MAX_VALUE.toLong()).toInt()
