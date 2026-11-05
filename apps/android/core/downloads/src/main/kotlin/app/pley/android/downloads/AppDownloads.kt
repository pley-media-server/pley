package app.pley.android.downloads

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.net.Uri
import androidx.core.content.ContextCompat
import app.pley.android.data.AppSettings
import app.pley.android.data.DataGraph
import app.pley.android.data.DownloadEntry
import app.pley.android.data.DownloadState
import app.pley.android.data.DownloadStorage
import app.pley.android.data.DownloadTarget
import app.pley.android.data.Downloads
import app.pley.android.data.LocalFile
import app.pley.android.data.Session
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaFile
import app.pley.android.model.PleyJson
import app.pley.android.model.ProgressReport
import app.pley.android.model.ProgressState
import app.pley.android.model.StreamKind
import app.pley.android.model.caps.ClientCaps
import app.pley.android.network.ApiException
import app.pley.android.network.NetworkGraph
import java.io.File
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.transform
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** The Application, which holds the app's downloads: [DownloadService] runs them. */
interface DownloadsOwner {
    val appDownloads: AppDownloads
}

/**
 * The app's downloads ([Downloads]): asked for here, fetched by
 * [DownloadService] while there are some to fetch, kept per server and
 * profile under the app's own storage, and played by the player through
 * [file] and [detail]. Progress made playing them goes to the server at
 * once when it can be reached, else waits in [PendingReports] until a
 * network comes back or the app opens.
 */
class AppDownloads internal constructor(
    private val context: Context,
    private val session: StateFlow<Session?>,
    private val settings: AppSettings,
    private val sources: DownloadSources,
    private val store: DownloadStore,
    private val reports: PendingReports,
    gate: NetworkGate,
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher = Dispatchers.IO,
    private val clock: () -> Long = System::currentTimeMillis,
    /** Whether a version of an item plays here as it is: away from the server, nothing converts it. */
    private val playsHere: suspend (ItemDetail, MediaFile) -> Boolean = { _, _ -> true },
    /** What Settings → Downloads → Quality asks for: `null` for the original. */
    private val target: suspend () -> DownloadTarget? = { null },
) : Downloads {
    init {
        // Small, and read once: made off the main thread by the app's warm-up.
        store.load()
    }

    private val runner = DownloadRunner(store, sources, gate, ::mobileAllowed, io, clock, profile = ::current)
    private val flushing = Mutex()

    /** Until when reports go straight to the queue: the server didn't answer a moment ago. */
    @Volatile
    private var offlineUntil = 0L

    override val entries: StateFlow<List<DownloadEntry>> = combine(store.all, session) { all, s ->
        val server = s?.issuer
        val userId = s?.me?.id
        all.filter { it.server == server && it.userId == userId }.sortedByDescending { it.addedAt }.map(::entryOf)
    }.flowOn(Dispatchers.Default).stateIn(scope, SharingStarted.Eagerly, emptyList())

    // Measured at once, then at most once a [STORAGE_EVERY_MS] while the downloads change (the
    // StateFlow keeps only the newest while it waits): a download's progress changes them
    // several times a second, which a debounce never let settle.
    override val storage: StateFlow<DownloadStorage> = store.all
        .transform {
            emit(DownloadStorage(usedBytes = store.usedBytes(), freeBytes = store.freeBytes()))
            delay(STORAGE_EVERY_MS)
        }
        .flowOn(io)
        .stateIn(scope, SharingStarted.Lazily, DownloadStorage())

    // ---------------------------------------------------------------- asking

    /**
     * Downloads the movies and episodes [itemIds] (with the signed-in
     * profile), each the file that suits a phone best. Throws what the
     * server or the network does when the items can't be read.
     */
    override suspend fun download(itemIds: List<Long>) {
        val s = session.value ?: return
        val userId = s.me?.id ?: return
        val api = sources.api(s.issuer, userId)
        val wanted = itemIds.distinct().filter { id -> store.get(Key(s.issuer, userId, id)) == null }
        if (wanted.isEmpty()) return
        val saved = wanted.mapNotNull { id -> api.item(id).let { detail -> savedOf(detail, s.issuer, userId)?.let { it to detail } } }
        withContext(io) { store.add(saved.map { it.first }) }
        if (saved.isEmpty()) return
        start()
        // Their pictures now, not each in its turn: a season's list shows its stills at once.
        scope.launch(io) { saved.forEach { (one, detail) -> runner.savePicture(one, detail) } }
    }

    override fun remove(itemId: Long) {
        val key = keyOf(itemId) ?: return
        runner.cancel(key)
        val saved = store.get(key)
        scope.launch(io) {
            store.remove(key)
            saved?.let { runner.forget(it) }
        }
    }

    override fun removeAll() {
        val all = store.all.value
        all.forEach { runner.cancel(it.key) }
        scope.launch(io) {
            store.clear()
            all.forEach { runner.forget(it) }
        }
    }

    override fun retry(itemId: Long) {
        val key = keyOf(itemId) ?: return
        scope.launch(io) {
            // A failed conversion is asked for anew: the server makes a failed one again.
            store.update(key) {
                it.copy(
                    state = DownloadState.Queued,
                    failure = null,
                    error = null,
                    status = null,
                    conversion = it.conversion.takeIf { _ -> it.mediaUrl != null },
                    converting = null,
                    notBefore = 0,
                )
            }
            start()
        }
    }

    override fun resume() {
        scope.launch {
            // Only the signed-in profile's: known once the session says who it is.
            val signedIn = current() ?: withTimeoutOrNull(PROFILE_WAIT_MS) { session.first { it?.me != null } }?.let { current() }
                ?: return@launch
            flushReports(signedIn.first, signedIn.second)
            if (store.all.value.any { it.state == DownloadState.Queued || it.state == DownloadState.Waiting }) {
                start()
            } else {
                // Subtitles kept cut short: no service for a few requests (it does them first when it runs).
                runner.finishCutSubtitles(signedIn)
            }
        }
    }

    /** Starts the service; the app is in the foreground (it asked, or it opened), as Android requires. */
    private fun start() {
        try {
            ContextCompat.startForegroundService(context, Intent(context, DownloadService::class.java))
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // Not allowed from the background: they start when the app opens.
        }
    }

    /** Runs the downloads until none is left: the service's work. Returns whether some still wait for a network. */
    internal suspend fun runQueue(): Boolean = runner.run()

    /** The downloads of every server and profile, for the service's notification. */
    internal val all: StateFlow<List<Saved>> get() = store.all

    // ---------------------------------------------------------------- playing

    override fun file(fileId: Long): LocalFile? {
        val (server, userId) = current() ?: return null
        val s = store.all.value.firstOrNull {
            it.server == server && it.userId == userId && it.fileId == fileId && it.state == DownloadState.Done
        } ?: return null
        return LocalFile(
            uri = Uri.fromFile(store.mediaOf(s)).toString(),
            subtitles = s.subtitles.associateWith { Uri.fromFile(store.subtitleOf(s, it)).toString() },
            converted = s.mediaUrl != null,
        )
    }

    override suspend fun detail(itemId: Long): ItemDetail? {
        val key = keyOf(itemId) ?: return null
        val s = store.get(key)?.takeIf { it.extrasSaved } ?: return null
        return withContext(io) {
            try {
                PleyJson.decodeFromString(ItemDetail.serializer(), store.detailOf(s).readText())
            } catch (_: Exception) {
                null
            }
        }
    }

    override suspend fun nextAfter(itemId: Long): ItemSummary? {
        val (server, userId) = current() ?: return null
        val mine = store.all.value.filter { it.server == server && it.userId == userId }
        val now = mine.firstOrNull { it.itemId == itemId } ?: return null
        val next = nextEpisode(mine, now) ?: return null
        val saved = detail(next.itemId)?.toSummary() ?: ItemSummary(id = next.itemId, libraryId = 0, kind = next.kind, title = next.title)
        return saved.copy(
            index = next.episodeIndex,
            parentIndex = next.seasonIndex,
            grandparentId = next.showId,
            grandparentTitle = next.showTitle,
            // The server's pictures can't come: the one saved can.
            thumbUrl = if (next.image) Uri.fromFile(store.imageOf(next)).toString() else null,
            backdropUrl = null,
            // Where it was left here is the latest this device knows.
            userState = saved.userState.copy(positionMs = next.positionMs, watched = next.watched),
        )
    }

    override suspend fun report(itemId: Long, report: ProgressReport) {
        val (server, userId) = current() ?: return
        val key = Key(server, userId, itemId)
        val duration = report.durationMs ?: store.get(key)?.durationMs
        val finished = duration != null && duration > 0 && report.positionMs >= duration * WATCHED_SHARE
        withContext(io) {
            // Where it was left, on the download too: Resume works offline.
            store.update(key, persist = report.state != ProgressState.Playing) {
                it.copy(positionMs = if (finished) 0 else report.positionMs, watched = it.watched || finished)
            }
        }
        val pending = PendingReport(server, userId, itemId, report.positionMs, report.durationMs, report.state, clock())
        if (clock() < offlineUntil) {
            withContext(io) { reports.add(pending) }
            return
        }
        try {
            sources.api(server, userId).reportProgress(itemId, report.copy(sessionId = null, ageMs = null))
            flushReports(server, userId)
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiException) {
            // The server answered: there is nothing to send again, unless what
            // answered was trouble on the way (a proxy, a tunnel).
            if (e.status >= 500) withContext(io) { reports.add(pending) }
        } catch (_: IOException) {
            offlineUntil = clock() + OFFLINE_MS
            withContext(io) { reports.add(pending) }
        } catch (_: Exception) {
            withContext(io) { reports.add(pending) }
        }
    }

    /** Sends the reports kept for [server] and [userId], each with its age, until one can't go. */
    private suspend fun flushReports(server: String, userId: Long) {
        if (!flushing.tryLock()) return
        try {
            val waiting = withContext(io) { reports.of(server, userId) }
            for (p in waiting) {
                try {
                    sources.api(server, userId).reportProgress(
                        p.itemId,
                        ProgressReport(positionMs = p.positionMs, durationMs = p.durationMs, state = p.state, ageMs = (clock() - p.atMs).coerceAtLeast(1)),
                    )
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    // Trouble on the way: kept for another time. Else answered: it won't go another time either.
                    if (e.status >= 500) {
                        offlineUntil = clock() + OFFLINE_MS
                        return
                    }
                } catch (_: Exception) {
                    offlineUntil = clock() + OFFLINE_MS
                    return
                }
                withContext(io) { reports.sent(p) }
            }
            offlineUntil = 0
        } finally {
            flushing.unlock()
        }
    }

    /** Sends what waits as soon as a network comes up, while the app runs. */
    @SuppressLint("MissingPermission")
    private fun followNetworks() {
        val cm = context.getSystemService(ConnectivityManager::class.java) ?: return
        try {
            cm.registerDefaultNetworkCallback(
                object : ConnectivityManager.NetworkCallback() {
                    override fun onAvailable(network: Network) {
                        offlineUntil = 0
                        scope.launch { current()?.let { (server, userId) -> flushReports(server, userId) } }
                    }
                },
            )
        } catch (_: Exception) {
            // Too many callbacks registered app-wide: the app opening sends them instead.
        }
    }

    init {
        followNetworks()
    }

    // ---------------------------------------------------------------- the rest

    private suspend fun mobileAllowed(): Boolean = try {
        settings.settings.first().downloadOnMobileData
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        false
    }

    private fun current(): Pair<String, Long>? {
        val s = session.value ?: return null
        return s.issuer to (s.me?.id ?: return null)
    }

    private fun keyOf(itemId: Long): Key? = current()?.let { (server, userId) -> Key(server, userId, itemId) }

    private suspend fun savedOf(detail: ItemDetail, server: String, userId: Long): Saved? {
        if (detail.kind != ItemKind.Movie && detail.kind != ItemKind.Episode) return null
        val playable = detail.files.filter { playsHere(detail, it) }.map { it.id }.toSet()
        val file = pickFile(detail.files) { it.id in playable } ?: return null
        // The server makes it for this device when asked to make it smaller, or when it doesn't play here as it is.
        val smaller = target()
        val convert = smaller != null || file.id !in playable
        val episode = detail.kind == ItemKind.Episode
        return Saved(
            server = server,
            userId = userId,
            itemId = detail.id,
            fileId = file.id,
            kind = detail.kind,
            title = detail.title,
            showId = if (episode) detail.grandparentId else null,
            showTitle = if (episode) detail.grandparentTitle else null,
            seasonIndex = if (episode) detail.parentIndex else null,
            episodeIndex = if (episode) detail.index else null,
            year = detail.year,
            durationMs = file.durationMs ?: detail.durationMs,
            // A converted file is Matroska, its size known once the server made it.
            container = if (convert) "mkv" else file.container,
            sizeBytes = if (convert) 0 else file.size,
            convert = convert,
            maxHeight = smaller?.maxHeight,
            maxBitrateKbps = smaller?.maxBitrateKbps,
            addedAt = clock(),
            positionMs = if (detail.userState.watched) 0 else detail.userState.positionMs,
            watched = detail.userState.watched,
        )
    }

    private fun entryOf(s: Saved): DownloadEntry = DownloadEntry(
        itemId = s.itemId,
        fileId = s.fileId,
        kind = s.kind,
        title = s.title,
        showId = s.showId,
        showTitle = s.showTitle,
        seasonIndex = s.seasonIndex,
        episodeIndex = s.episodeIndex,
        year = s.year,
        durationMs = s.durationMs,
        sizeBytes = s.sizeBytes,
        downloadedBytes = if (s.state == DownloadState.Done) s.sizeBytes else s.downloadedBytes,
        state = s.state,
        imageUri = if (s.image) Uri.fromFile(store.imageOf(s)).toString() else null,
        addedAt = s.addedAt,
        positionMs = s.positionMs,
        watched = s.watched,
        error = s.errorText(),
        converting = s.converting?.takeIf { s.mediaUrl == null && s.state != DownloadState.Done && s.state != DownloadState.Failed },
        serverAway = s.state == DownloadState.Waiting && s.unreachable,
    )

    companion object {
        /** How long the app's opening waits for the session to say who is signed in. */
        const val PROFILE_WAIT_MS = 10_000L

        /**
         * Whether [file] has a sound track this device decodes ([caps]'
         * decoders, not what it sends on to a receiver: a download plays
         * anywhere), or no sound at all.
         */
        fun soundPlays(file: MediaFile, caps: ClientCaps): Boolean {
            val tracks = file.streams.filter { it.kind == StreamKind.Audio }
            if (tracks.isEmpty()) return true
            // The track the player starts on, as it chooses it: the profile's, else the file's default, else the first.
            val playing = tracks.firstOrNull { it.index == file.selectedAudio } ?: tracks.firstOrNull { it.default } ?: tracks.first()
            return playing.codec.lowercase() in (caps.audioDecode ?: caps.audioCodecs)
        }

        /** The episode of [now]'s show downloaded whole after it, by season and number, among [mine]. */
        internal fun nextEpisode(mine: List<Saved>, now: Saved): Saved? {
            if (now.kind != ItemKind.Episode || now.showId == null) return null
            val order = compareBy<Saved>({ it.seasonIndex ?: Int.MAX_VALUE }, { it.episodeIndex ?: Int.MAX_VALUE })
            return mine
                .filter { it.kind == ItemKind.Episode && it.showId == now.showId && it.state == DownloadState.Done && order.compare(it, now) > 0 }
                .minWithOrNull(order)
        }

        /** A share of the duration past which a download played here counts as watched, as on the server. */
        const val WATCHED_SHARE = 0.9

        /** After a report couldn't go, the next ones wait this long before trying the network again. */
        const val OFFLINE_MS = 60_000L

        const val STORAGE_EVERY_MS = 1_000L

        /**
         * The app's downloads: kept in the app's own storage (removed with
         * the app), the server and profile signed in to now from [data].
         */
        /**
         * [playsHere]: whether a version of an item plays on this device as
         * it is; [caps]: what it plays, for the server to convert it (the
         * player's capabilities); [target]: the quality downloads are made
         * at, `null` for the original (Settings → Downloads).
         */
        fun create(
            context: Context,
            data: DataGraph,
            network: NetworkGraph,
            playsHere: suspend (ItemDetail, MediaFile) -> Boolean = { _, _ -> true },
            caps: suspend () -> ClientCaps? = { null },
            target: suspend () -> DownloadTarget? = { null },
        ): AppDownloads {
            val app = context.applicationContext
            val root = File(app.getExternalFilesDir(null) ?: app.filesDir, "downloads")
            val sources = object : DownloadSources {
                override fun api(server: String, userId: Long) = network.api(server, userId)

                override fun media(server: String) = network.media(server)

                override fun subtitles(server: String) = network.subtitles(server)

                override val images get() = network.images

                override suspend fun caps(): ClientCaps? = caps()
            }
            return AppDownloads(
                context = app,
                session = data.session.session,
                settings = data.settings,
                sources = sources,
                store = DownloadStore(root),
                reports = PendingReports(File(app.filesDir, "downloads/reports.json")),
                gate = AndroidNetworkGate(app),
                scope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
                playsHere = playsHere,
                target = target,
            )
        }

        /**
         * The file of [files] that suits a phone best: of those that play
         * here as they are ([playsHere], all when none does), the sharpest
         * up to 1080p (the size a phone shows, far smaller than 4K), else
         * the smallest there is.
         */
        internal fun pickFile(files: List<MediaFile>, playsHere: (MediaFile) -> Boolean = { true }): MediaFile? {
            if (files.size <= 1) return files.firstOrNull()
            val candidates = files.filter(playsHere).ifEmpty { files }
            val fitting = candidates.filter { (it.height ?: 0) in 1..MAX_HEIGHT }
            return fitting.maxByOrNull { it.height ?: 0 } ?: candidates.minByOrNull { if (it.size > 0) it.size else Long.MAX_VALUE }
        }

        private const val MAX_HEIGHT = 1080
    }
}
