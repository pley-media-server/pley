package app.pley.android.model.admin

import app.pley.android.model.ItemSummary
import app.pley.android.model.JobState
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.ScanPhase
import app.pley.android.model.StreamAction
import kotlinx.serialization.Serializable

/*
 * The dashboard of managing the server (the web's Settings → Dashboard):
 * what plays and what the server is busy with (GET /api/activity), the
 * server and its transcoder (GET /api/system, GET /api/transcoder), the
 * machine's load (GET /api/system/resources) and the space Pley uses
 * (GET /api/system/storage). Everyone's watch
 * history reads as the account page's does (app.pley.android.model.HistoryPage).
 */

/** GET /api/activity, as an administrator sees it: everyone's sessions and the libraries' work. */
@Serializable
data class DashboardActivity(
    val scans: List<DashboardScan> = emptyList(),
    val sessions: List<DashboardSession> = emptyList(),
    /** Intro and credits detection going on now. */
    val analysis: DashboardAnalysis? = null,
    /** Seasons waiting for intro and credits detection. */
    val seasonsPending: Int = 0,
    /** Files waiting for seek previews, those being made included. */
    val previewsPending: Int = 0,
    /** Files whose seek previews are being made now. */
    val previewsRunning: Int = 0,
    /** Why what waits waits, while some does. */
    val libraryWork: DashboardLibraryWork? = null,
)

/** A library's scan (apps/server/src/events.rs, `ScanStatus`). */
@Serializable
data class DashboardScan(
    val libraryId: Long,
    val libraryName: String,
    val phase: ScanPhase = ScanPhase.Unknown,
    val processed: Int = 0,
    val total: Int = 0,
)

/** The season whose audio is being fingerprinted for its intros and credits. */
@Serializable
data class DashboardAnalysis(
    val title: String,
    val done: Int = 0,
    val total: Int = 0,
)

/** When the libraries' work gets done (docs/API.md, "Library work"). */
@Serializable
data class DashboardLibraryWork(
    /** "auto", "hours" or "idle". */
    val mode: String = "auto",
    /** "playing", "busy" or "hours"; `null` while nothing waits. */
    val waiting: String? = null,
    /** Unix seconds: when the hours it waits for begin. */
    val resumesAt: Long? = null,
)

/** Someone's playback (apps/server/src/api/activity.rs, `SessionInfo`). */
@Serializable
data class DashboardSession(
    val sessionId: String,
    val userId: Long = 0,
    val username: String = "",
    /** "Living room TV", "Firefox on macOS"; `null` when it isn't known. */
    val device: String? = null,
    val item: ItemSummary,
    val method: PlaybackMethod = PlaybackMethod.Unknown,
    val positionMs: Long = 0,
    /** "playing", "paused" or "stopped". */
    val state: String = "",
    val transcode: DashboardTranscode? = null,
)

/**
 * What a transcode does, live (apps/server/src/playback.rs, `TranscodeInfo`):
 * the player's details declare the fields they read, this the dashboard's.
 */
@Serializable
data class DashboardTranscode(
    val state: JobState = JobState.Unknown,
    val video: StreamAction = StreamAction.Unknown,
    val audio: StreamAction = StreamAction.Unknown,
    val videoIn: String? = null,
    val videoOut: String? = null,
    val audioIn: String? = null,
    val audioOut: String? = null,
    val encoder: String? = null,
    /** The software encoder's preset now. */
    val preset: String? = null,
    val decoder: String? = null,
    /** "gpu", "hybrid", "cpu" or "copy". */
    val pipeline: String? = null,
    val tonemap: String? = null,
    val subtitleBurn: Boolean = false,
    val speed: Double? = null,
    val fps: Double? = null,
    val segmentsReady: Int = 0,
    val segmentsTotal: Int = 0,
    val lastError: String? = null,
    /** Of a transcode in several sizes: the one the player takes now, 0 the largest… */
    val rendition: Int = 0,
    /** …and how many smaller ones there are (0: one size). */
    val variants: Int = 0,
)

/** GET /api/system: the server, its transcoder and what the libraries hold. */
@Serializable
data class DashboardSystem(
    val name: String = "",
    val version: String = "",
    val commit: String? = null,
    val os: String = "",
    val arch: String = "",
    val uptimeSecs: Long = 0,
    val dataDir: String = "",
    /** The check of the database at start. */
    val database: DatabaseHealth = DatabaseHealth(),
    val transcoder: DashboardTranscoderInfo = DashboardTranscoderInfo(),
    val counts: DashboardCounts = DashboardCounts(),
)

/** The check of the database at start; [ok] is `null` until it finished. */
@Serializable
data class DatabaseHealth(
    val ok: Boolean? = null,
    /** What SQLite found, when the database is damaged. */
    val problem: String? = null,
)

/** The transcoder as the server found it (`SystemInfo.transcoder`). */
@Serializable
data class DashboardTranscoderInfo(
    val available: Boolean = false,
    /** Started by Pley, rather than one run on its own. */
    val managed: Boolean = false,
    val url: String? = null,
    val version: String? = null,
    val commit: String? = null,
    val ffmpegVersion: String? = null,
    val encoders: List<String> = emptyList(),
    val activeEncoder: String? = null,
    val hwaccels: List<String> = emptyList(),
    val tonemapping: List<String> = emptyList(),
    val dolbyVision: Boolean = false,
    val maxJobs: Int = 0,
    val activeJobs: Int = 0,
    val error: String? = null,
)

/** How much the libraries hold, and who uses them. */
@Serializable
data class DashboardCounts(
    val libraries: Int = 0,
    val movies: Int = 0,
    val shows: Int = 0,
    val episodes: Int = 0,
    val artists: Int = 0,
    val albums: Int = 0,
    val tracks: Int = 0,
    val files: Int = 0,
    val users: Int = 0,
)

/** GET /api/transcoder: the transcoder's live numbers and every job it has. */
@Serializable
data class DashboardTranscoderStatus(
    val available: Boolean = false,
    val maxJobs: Int = 0,
    val runningProcesses: Int = 0,
    val hwProcesses: Int = 0,
    val maxHwJobs: Int = 0,
    val workDirBytes: Long = 0,
    val counters: DashboardTranscoderCounters = DashboardTranscoderCounters(),
    val jobs: List<DashboardTranscoderJob> = emptyList(),
)

/** What the transcoder has done since it started. */
@Serializable
data class DashboardTranscoderCounters(
    val jobsCreated: Long = 0,
    val jobsFailed: Long = 0,
    val processFailures: Long = 0,
    val segmentsProduced: Long = 0,
    val segmentsServed: Long = 0,
    val bytesServed: Long = 0,
    val probes: Long = 0,
    val probeFailures: Long = 0,
    val probeMsTotal: Long = 0,
    val thumbnails: Long = 0,
    val subtitleExtractions: Long = 0,
)

/** One of the transcoder's jobs. */
@Serializable
data class DashboardTranscoderJob(
    val id: String,
    /** What it converts, as the server named it. */
    val label: String? = null,
    /** Who it converts for. */
    val owner: String? = null,
    val state: JobState = JobState.Unknown,
    val info: DashboardTranscode = DashboardTranscode(),
    val restarts: Int = 0,
    val servedBytes: Long = 0,
    val warnings: List<String> = emptyList(),
)

/** GET /api/system/resources: the machine's processors and memory over the last five minutes. */
@Serializable
data class DashboardResources(
    /** False where the system can't be measured. */
    val available: Boolean = false,
    /** How often a sample is taken, in ms. */
    val intervalMs: Long = 2_000,
    val cpus: Int = 0,
    val memoryTotal: Long = 0,
    /** Oldest first. */
    val samples: List<ResourceSample> = emptyList(),
)

/** The machine and Pley (the server, its transcoder and their FFmpegs) at one moment. */
@Serializable
data class ResourceSample(
    /** Unix ms. */
    val at: Long,
    /** Share of all the processors' time, 0 to 100. */
    val cpu: Double = 0.0,
    val pleyCpu: Double = 0.0,
    /** Bytes in use. */
    val memory: Long = 0,
    val pleyMemory: Long = 0,
)

/** GET /api/system/storage: the space what Pley stores takes (docs/STORAGE.md). */
@Serializable
data class StorageUsage(
    val blobs: Int = 0,
    val blobBytes: Long = 0,
    val mediaFolders: Int = 0,
    val mediaBytes: Long = 0,
    val cacheBytes: Long = 0,
)

/** What POST /api/system/storage/clean removed. */
@Serializable
data class StorageCleanReport(
    val blobsRemoved: Int = 0,
    val mediaFoldersRemoved: Int = 0,
    val bytesFreed: Long = 0,
)
