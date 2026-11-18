package app.pley.android.model.admin

import app.pley.android.model.LibraryKind
import app.pley.android.model.ScanPhase
import kotlinx.serialization.Serializable

/*
 * Managing the libraries (docs/API.md, "Libraries", and "System" for
 * /api/fs; apps/server/src/api/libraries.rs and admin.rs): what Settings →
 * Libraries reads and sends besides [app.pley.android.model.Library],
 * which GET /api/libraries answers with its folders for this app.
 */

/** GET /api/activity, for its scans only: the rest is the dashboard's. */
@Serializable
data class LibraryScans(
    val scans: List<LibraryScan> = emptyList(),
)

/** A library's scan, queued, running or just done (`ScanStatus`). */
@Serializable
data class LibraryScan(
    val libraryId: Long,
    val phase: ScanPhase = ScanPhase.Unknown,
    /** Files done of [total], in the phase that counts them; 0 of 0 while nobody knows. */
    val processed: Long = 0,
    val total: Long = 0,
) {
    /** Queued or under way. */
    val running: Boolean get() = phase != ScanPhase.Done
}

/** POST /api/libraries: a new library, scanned at once; [tmdbCollections] only matters to a film library. */
@Serializable
data class NewLibrary(
    val name: String,
    val kind: LibraryKind,
    val paths: List<String>,
    val tmdbCollections: Boolean = true,
)

/**
 * PATCH /api/libraries/{id}: its name, folders and whether Pley makes its
 * collections from TMDB (each left out when `null`); new folders are
 * scanned. Turning [tmdbCollections] off removes the ones Pley made, edited
 * or not; turning it on reads the library's metadata again.
 */
@Serializable
data class LibraryChange(
    val name: String? = null,
    val paths: List<String>? = null,
    val tmdbCollections: Boolean? = null,
)

/** POST /api/libraries/{id}/scan; [force] probes every file again and refreshes the metadata. */
@Serializable
data class LibraryScanRequest(
    val force: Boolean = false,
)

/** POST /api/fs: the folders in [path], or the roots and drives without one. */
@Serializable
data class FolderQuery(
    val path: String? = null,
)

/**
 * What POST /api/fs answers: [path]'s folders, by name, with the folder
 * above it ([parent], `null` at a drive's top). At the roots, [path] is
 * `null` and [entries] are the drives and places to start from.
 */
@Serializable
data class FolderListing(
    val path: String? = null,
    val parent: String? = null,
    val entries: List<FolderEntry> = emptyList(),
)

/** A folder of a [FolderListing], with its whole path. */
@Serializable
data class FolderEntry(
    val name: String,
    val path: String,
)
