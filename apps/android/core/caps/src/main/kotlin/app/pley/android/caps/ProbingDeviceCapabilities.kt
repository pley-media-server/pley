package app.pley.android.caps

import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.view.Display
import app.pley.android.common.AppLanguage
import app.pley.android.common.DeviceProfile
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File

/**
 * The probe: decoders read once per system build (they are the slow part),
 * the screen and audio output read at every probe, the rules applied by
 * [CapsBuilder]. [warmUp] runs it off the main thread, from the saved
 * snapshot when nothing changed, then watches the screen and the audio
 * output and probes again, a moment after they settle.
 */
class ProbingDeviceCapabilities(
    private val device: () -> DeviceFacts,
    private val codecs: CodecInventory,
    private val display: DisplayInfo,
    private val audio: AudioInfo,
    /** The app's own quirk table, read on the first probe. */
    builtInQuirks: () -> QuirkTable,
    private val changes: ChangeSource? = null,
    private val cache: CapsCache? = null,
    /** When the app was installed or updated: a new version probes again. */
    private val installedAt: Long = 0,
    /** Diagnostics' lines for a snapshot, in the language shown. */
    private val describe: (CapsSnapshot) -> List<String> = { emptyList() },
    /** The language shown: a snapshot described in another has its lines described again when it is asked for. */
    private val language: () -> Any? = { null },
    private val dispatcher: CoroutineDispatcher = Dispatchers.Default,
    private val scope: CoroutineScope = CoroutineScope(SupervisorJob() + dispatcher),
    /** How long the screen or output must stay put before probing again. */
    private val settleMs: Long = 300,
    private val displayId: MutableStateFlow<Int>? = null,
) : DeviceCapabilities {
    private val state = MutableStateFlow<CapsSnapshot?>(null)
    override val snapshot: StateFlow<CapsSnapshot?> = state.asStateFlow()

    private val builtInQuirks by lazy(builtInQuirks)
    private val lock = Mutex()
    private val writing = Mutex()
    private var decoders: List<DecoderInfo>? = null
    private var watching = false
    private var pending: Job? = null

    @Volatile
    private var settings = CapsSettings()

    @Volatile
    private var serverQuirks: QuirkTable? = null

    /** The language the lines handed out are in. */
    private var describedIn: Any? = null

    override suspend fun warmUp(): CapsSnapshot = withContext(dispatcher) {
        lock.withLock {
            val known = state.value
            val done = when {
                known == null -> probe()
                language() == describedIn -> known
                // The language shown changed since: the same snapshot, its lines in the new one.
                else -> described(known).also { state.value = it }
            }
            if (!watching) {
                watching = true
                // Registering reads the audio output once more: not on the caller's time.
                changes?.let { source -> scope.launch { source.start { refresh() } } }
            }
            done
        }
    }

    override fun setDisplay(displayId: Int) {
        val ids = this.displayId ?: return
        if (ids.value != displayId) {
            ids.value = displayId
            refresh()
        }
    }

    override fun updateSettings(settings: CapsSettings) {
        if (this.settings != settings) {
            this.settings = settings
            refresh()
        }
    }

    override fun updateQuirks(table: QuirkTable) {
        if (serverQuirks != table) {
            serverQuirks = table
            refresh()
        }
    }

    /** Stops watching the screen and the audio output. */
    fun close() {
        changes?.stop()
        pending?.cancel()
    }

    /** Probes again once things settle, if a first probe has run (until then [warmUp] will). */
    private fun refresh() {
        synchronized(this) {
            pending?.cancel()
            pending = scope.launch {
                delay(settleMs)
                lock.withLock { if (state.value != null) probe() }
            }
        }
    }

    /**
     * One probe. The sources are read side by side: the screen, the audio
     * output, the quirk table, the saved probe, and the decoders when
     * nothing is saved. Once the decoders are known, a probe only rebuilds.
     */
    private suspend fun probe(): CapsSnapshot = coroutineScope {
        val facts = device()
        val known = decoders
        val firstEver = known == null && cache?.exists != true
        val probing = if (firstEver) async { codecs.decoders() } else null
        val saved = if (known == null) async { cache?.load() } else null
        val screen = async { display.read() }
        val output = async { audio.read() }
        val quirks = async { serverQuirks?.let { builtInQuirks.overriddenBy(it) } ?: builtInQuirks }
        val settings = settings
        val key = CapsCache.key(facts, installedAt, screen.await(), output.await(), quirks.await(), settings)
        val entry = saved?.await()
        if (entry != null && entry.key == key) {
            decoders = entry.decoders(facts.fingerprint)
            // Described again: the language may have changed since.
            val done = described(entry.snapshot)
            state.value = done
            return@coroutineScope done
        }
        val found = known ?: probing?.await() ?: entry?.decoders(facts.fingerprint) ?: codecs.decoders()
        decoders = found
        val built = CapsBuilder.build(ProbeInput(facts, found, screen.await(), output.await()), quirks.await(), settings)
        val done = described(built)
        state.value = done
        // Saved after it is handed out: the player needn't wait for the disk.
        cache?.let { saving -> scope.launch { writing.withLock { saving.write(key, done) } } }
        done
    }

    /** [snapshot] with its lines in the language shown now. */
    private fun described(snapshot: CapsSnapshot): CapsSnapshot {
        describedIn = language()
        return snapshot.copy(lines = describe(snapshot))
    }

    companion object {
        /** The probe of this device, saving its snapshot in the app's no-backup files. */
        fun create(context: Context, profile: DeviceProfile): ProbingDeviceCapabilities {
            val app = context.applicationContext
            val displayId = MutableStateFlow(Display.DEFAULT_DISPLAY)
            val resources = app.resources
            return ProbingDeviceCapabilities(
                device = { deviceFacts(app, profile) },
                codecs = AndroidCodecInventory(),
                display = AndroidDisplayInfo(app) { displayId.value },
                audio = AndroidAudioInfo(app),
                builtInQuirks = { QuirkTable.load(resources) },
                changes = AndroidChangeSource(app),
                cache = CapsCache(File(app.noBackupFilesDir, "caps/snapshot.json")),
                installedAt = installedAt(app),
                // The app's resources in the language shown, asked for each time: below Android 13 the
                // application's own stay in the language the app started in.
                describe = { CapsDescription.lines(it, AppLanguage.resources(app)) },
                language = { AppLanguage.locale },
                displayId = displayId,
            )
        }

        /** The device as the probe sees it: [profile] plus what the build says. */
        fun deviceFacts(context: Context, profile: DeviceProfile): DeviceFacts = DeviceFacts(
            model = profile.model,
            manufacturer = Build.MANUFACTURER.orEmpty(),
            sdk = Build.VERSION.SDK_INT,
            fingerprint = Build.FINGERPRINT.orEmpty(),
            form = when {
                profile.isTv -> DeviceFacts.Form.Tv
                context.resources.configuration.smallestScreenWidthDp >= TABLET_MIN_DP -> DeviceFacts.Form.Tablet
                else -> DeviceFacts.Form.Phone
            },
            fireTv = profile.fireTv,
            totalRamBytes = profile.totalRamBytes,
            lowRam = profile.lowRam,
        )

        private const val TABLET_MIN_DP = 600

        private fun installedAt(context: Context): Long = try {
            context.packageManager.getPackageInfo(context.packageName, 0).lastUpdateTime
        } catch (_: PackageManager.NameNotFoundException) {
            0
        }
    }
}
