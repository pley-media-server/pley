package app.pley.android.ui.phone.admin.libraries

import android.content.res.Resources
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.ScanPhase
import app.pley.android.model.admin.LibraryScan
import app.pley.android.model.format.formatRelative
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.libraries.LibraryAdmin
import app.pley.android.ui.phone.R

/*
 * What the pages of managing the libraries share: their words for kinds,
 * scans and confirmations, and a scan's progress.
 */

/** The kinds a new library can be, in the web's order. */
internal val LIBRARY_KINDS = listOf(LibraryKind.Movies, LibraryKind.Shows, LibraryKind.Music)

@StringRes
internal fun kindLabel(kind: LibraryKind): Int = when (kind) {
    LibraryKind.Shows -> R.string.admin_libraries_kind_shows
    LibraryKind.Music -> R.string.admin_libraries_kind_music
    else -> R.string.admin_libraries_kind_movies
}

/** How the kind's folders are best laid out: "Show Name/Season 01/…". */
@StringRes
internal fun kindLayout(kind: LibraryKind): Int = when (kind) {
    LibraryKind.Shows -> R.string.admin_libraries_layout_shows
    LibraryKind.Music -> R.string.admin_libraries_layout_music
    else -> R.string.admin_libraries_layout_movies
}

/** The name a library of [kind] gets when none is typed. */
@StringRes
internal fun kindDefaultName(kind: LibraryKind): Int = when (kind) {
    LibraryKind.Shows -> R.string.admin_libraries_default_shows
    LibraryKind.Music -> R.string.admin_libraries_default_music
    else -> R.string.admin_libraries_default_movies
}

/** "1,204 movies · scanned 3 hours ago". */
@Composable
internal fun libraryLine(library: Library): String {
    val count = when (library.kind) {
        LibraryKind.Shows -> R.plurals.admin_libraries_items_shows
        LibraryKind.Music -> R.plurals.admin_libraries_items_music
        else -> R.plurals.admin_libraries_items_movies
    }
    val items = pluralStringResource(count, library.itemCount, library.itemCount)
    val scanned = library.scannedAt?.let { stringResource(R.string.admin_libraries_scanned, formatRelative(it, words = formatWords())) }
        ?: stringResource(R.string.admin_libraries_never_scanned)
    return "$items · $scanned"
}

/** What a scan is doing ("Finding files"); "Scanning" before the server says. */
@Composable
internal fun scanPhase(scan: LibraryScan?): String = stringResource(
    when (scan?.phase) {
        ScanPhase.Queued -> R.string.admin_libraries_phase_queued
        ScanPhase.Walking -> R.string.admin_libraries_phase_walking
        ScanPhase.Probing -> R.string.admin_libraries_phase_probing
        ScanPhase.Metadata -> R.string.admin_libraries_phase_metadata
        ScanPhase.Done -> R.string.admin_libraries_phase_done
        else -> R.string.admin_libraries_scanning
    },
)

/**
 * A scan under way, as the web's row shows it: what it is doing and how
 * many files of how many, over a bar that fills as they are done (a
 * moving one while nobody knows how many there are).
 */
@Composable
internal fun ScanProgress(scan: LibraryScan?, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(PleyIcons.RefreshCw, contentDescription = null, size = 14.dp, tint = PleyColors.AccentBright)
            Spacer(Modifier.width(6.dp))
            Text(scanPhase(scan), style = MaterialTheme.typography.bodySmall, color = PleyColors.AccentBright, modifier = Modifier.weight(1f))
            if (scan != null && scan.total > 0) {
                Text(
                    stringResource(R.string.admin_libraries_progress, scan.processed, scan.total),
                    style = MaterialTheme.typography.bodySmall,
                    color = PleyColors.Muted,
                )
            }
        }
        Spacer(Modifier.height(6.dp))
        if (scan != null && scan.total > 0) {
            LinearProgressIndicator(
                progress = { (scan.processed.toFloat() / scan.total).coerceIn(0f, 1f) },
                modifier = Modifier.fillMaxWidth(),
                color = PleyColors.Accent,
                trackColor = PleyColors.Surface4,
                drawStopIndicator = {},
            )
        } else {
            LinearProgressIndicator(Modifier.fillMaxWidth(), color = PleyColors.Accent, trackColor = PleyColors.Surface4)
        }
    }
}

/** The toasts of what the libraries' pages confirm, in the web's words. */
internal fun librariesSaid(resources: Resources, done: AdminDone): String? {
    val name = done.args.firstOrNull()?.toString().orEmpty()
    return when (done.what) {
        LibraryAdmin.SCAN_STARTED -> resources.getString(R.string.admin_libraries_scan_started, name)
        LibraryAdmin.RESCAN_STARTED -> resources.getString(R.string.admin_libraries_rescan_started, name)
        LibraryAdmin.SCANNING_ALL -> resources.getString(R.string.admin_libraries_scanning_all)
        LibraryAdmin.ADDED -> resources.getString(R.string.admin_libraries_added, name)
        LibraryAdmin.UPDATED -> resources.getString(R.string.admin_libraries_updated)
        LibraryAdmin.DELETED -> resources.getString(R.string.admin_libraries_deleted, name)
        else -> null
    }
}

