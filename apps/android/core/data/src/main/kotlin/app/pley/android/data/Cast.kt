package app.pley.android.data

import app.pley.android.model.CastAction
import app.pley.android.model.CastControlRequest
import app.pley.android.model.CastDevice
import app.pley.android.model.CastPlatform
import app.pley.android.model.CastPlayRequest
import app.pley.android.model.CastPlayState
import app.pley.android.model.CastReport
import app.pley.android.model.CastState
import app.pley.android.model.ItemSummary
import app.pley.android.model.ServerEvent
import java.io.IOException
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * What a device to cast to plays, as it last said: [saidAtMs] is when, on
 * [CastRepository.now]'s clock, so a playing position moves on from there.
 */
data class CastPlaying(
    val item: ItemSummary,
    val positionMs: Long,
    val durationMs: Long?,
    val state: CastPlayState,
    val saidAtMs: Long,
) {
    /** Where it is at [nowMs]: moved on since it said so while playing, never past the end. */
    fun positionAt(nowMs: Long): Long {
        if (state != CastPlayState.Playing) return positionMs
        val moved = positionMs + (nowMs - saidAtMs).coerceAtLeast(0)
        val end = durationMs ?: item.durationMs
        return if (end != null && end > 0) moved.coerceAtMost(end) else moved
    }

    companion object {
        /** [state], heard at [nowMs]: it said so [CastState.ageMs] before. */
        fun of(state: CastState, nowMs: Long): CastPlaying =
            CastPlaying(state.item, state.positionMs, state.durationMs, state.state, nowMs - state.ageMs.coerceAtLeast(0))
    }
}

/** A device the profile may cast to, and what it plays ([playing] `null`: nothing). */
data class CastTarget(
    val id: String,
    val name: String,
    val platform: CastPlatform,
    val playing: CastPlaying?,
) {
    companion object {
        fun of(device: CastDevice, nowMs: Long): CastTarget =
            CastTarget(device.id, device.name, device.platform, device.state?.let { CastPlaying.of(it, nowMs) })
    }
}

/**
 * The device last cast to from here: [itemId] sent to [deviceId] at [atMs]
 * ([CastRepository.now]'s clock), or `null` when its remote was opened on
 * what it played already. [answered]: the device has said it plays [itemId].
 */
data class CastSent(val deviceId: String, val itemId: Long?, val atMs: Long, val answered: Boolean = false)

/**
 * Casting (docs/API.md, "Casting to another device"): phones send what to
 * play to the profile's other devices and steer them; TVs are cast to.
 *
 * Sending: [devices] follows the server's list once [start]ed, while the
 * event stream is open: read when it connects and when a device comes,
 * goes or is renamed (`cast_devices_changed`), each device's state from
 * its `cast_state`. A profile switch starts it over.
 *
 * Being cast to: [commands] are the `cast_play` and `cast_control` that
 * name this device, and [report] says what it plays.
 */
interface CastRepository {
    /** The profile's devices to cast to, by name, with what each plays; empty when none (or not known). */
    val devices: StateFlow<List<CastTarget>>

    /** The device last cast to from here; `null` before (or after a profile switch). */
    val sent: StateFlow<CastSent?>

    /** A device whose remote is to show, each time: one sent something, or one that played already. */
    val remotes: Flow<String>

    /** Starts following [devices], once: the phone's shell, after the first frame. */
    fun start()

    /** The clock of [CastPlaying.saidAtMs] and [CastSent.atMs], in ms. */
    fun now(): Long

    /**
     * Device [deviceId] plays [itemId] (its version [fileId]) from
     * [positionMs]; `null` from where the profile left off. Its remote
     * shows. Throws an IOException when the server refuses or can't be reached.
     */
    suspend fun play(deviceId: String, itemId: Long, fileId: Long? = null, positionMs: Long? = null)

    /** Shows the remote of [deviceId], which plays something already: nothing is sent. */
    fun follow(deviceId: String)

    /** Device [deviceId]'s player pauses, resumes, seeks to [positionMs] or stops. Throws as [play]. */
    suspend fun control(deviceId: String, action: CastAction, positionMs: Long? = null)

    /** `cast_play` and `cast_control` for this device, as they come: TVs only, while they may be cast to. */
    val commands: Flow<ServerEvent>

    /** Says what this device plays: fire and forget, the latest first, failures ignored. Nothing on phones. */
    fun report(report: CastReport)

    companion object {
        /** No casting: graphs made without it (tests of other screens, previews). */
        val None: CastRepository = object : CastRepository {
            override val devices: StateFlow<List<CastTarget>> = MutableStateFlow(emptyList<CastTarget>()).asStateFlow()
            override val sent: StateFlow<CastSent?> = MutableStateFlow<CastSent?>(null).asStateFlow()
            override val remotes: Flow<String> = emptyFlow()
            override val commands: Flow<ServerEvent> = emptyFlow()

            override fun start() = Unit
            override fun now(): Long = System.nanoTime() / 1_000_000
            override suspend fun play(deviceId: String, itemId: Long, fileId: Long?, positionMs: Long?) = throw IOException("no casting here")
            override fun follow(deviceId: String) = Unit
            override suspend fun control(deviceId: String, action: CastAction, positionMs: Long?) = throw IOException("no casting here")
            override fun report(report: CastReport) = Unit
        }
    }
}

/**
 * [CastRepository] over [cache] (the API of the profile in use) and the
 * server's events from [live]. [ownId] is this device's id where it is
 * cast to (TVs, Settings' castDeviceId); `null` on phones, which only send.
 */
internal class ServerCast(
    private val cache: ResourceCache,
    private val live: LiveUpdates,
    private val ownId: (suspend () -> String)?,
    private val app: CoroutineScope,
    private val clock: () -> Long,
) : CastRepository {
    private val _devices = MutableStateFlow<List<CastTarget>>(emptyList())
    override val devices: StateFlow<List<CastTarget>> = _devices.asStateFlow()

    private val _sent = MutableStateFlow<CastSent?>(null)
    override val sent: StateFlow<CastSent?> = _sent.asStateFlow()

    private val _remotes = MutableSharedFlow<String>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val remotes: Flow<String> = _remotes.asSharedFlow()

    private val started = AtomicBoolean(false)

    // Only the latest matters, and they go in order: one at a time.
    private val reports = Channel<CastReport>(Channel.CONFLATED)
    private val reporting = AtomicBoolean(false)

    override fun now(): Long = clock()

    override fun start() {
        if (!started.compareAndSet(false, true)) return
        app.launch {
            cache.scopes.map { (it as? ScopeState.Ready)?.profile }.distinctUntilChanged().collectLatest { profile ->
                // Another profile's devices, and the one it cast to, aren't this one's.
                _devices.value = emptyList()
                _sent.value = null
                if (profile != null) follow(profile)
            }
        }
    }

    /** Reads the list now and whenever it may have changed; updates each state as it is heard. */
    private suspend fun follow(profile: ProfileScope) = coroutineScope {
        val asks = Channel<Unit>(Channel.CONFLATED)
        asks.trySend(Unit)
        launch { live.answered.collect { asks.trySend(Unit) } }
        launch {
            live.events.collect { event ->
                when (event) {
                    ServerEvent.CastDevicesChanged -> asks.trySend(Unit)
                    is ServerEvent.CastStateChanged -> heard(event)
                    else -> Unit
                }
            }
        }
        for (ask in asks) {
            try {
                val list = cache.api(profile).castDevices()
                val now = clock()
                _devices.value = list.map { CastTarget.of(it, now) }
                list.forEach { answered(it.id, it.state?.item?.id) }
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // An older server (404), or none in reach: what was listed stays, and the next connection asks again.
            }
        }
    }

    private fun heard(event: ServerEvent.CastStateChanged) {
        val playing = event.state?.let { CastPlaying.of(it, clock()) }
        _devices.update { list -> list.map { if (it.id == event.deviceId) it.copy(playing = playing) else it } }
        answered(event.deviceId, playing?.item?.id)
    }

    /** Device [deviceId] plays [itemId]: when that is what it was sent, it has answered. */
    private fun answered(deviceId: String, itemId: Long?) {
        _sent.update { sent -> if (sent != null && sent.deviceId == deviceId && itemId != null && sent.itemId == itemId) sent.copy(answered = true) else sent }
    }

    override suspend fun play(deviceId: String, itemId: Long, fileId: Long?, positionMs: Long?) {
        cache.api(cache.profile()).castPlay(deviceId, CastPlayRequest(itemId, fileId, positionMs?.coerceAtLeast(0)))
        _sent.value = CastSent(deviceId, itemId, clock())
        // It may have said so before the server's answer came.
        answered(deviceId, _devices.value.firstOrNull { it.id == deviceId }?.playing?.item?.id)
        _remotes.tryEmit(deviceId)
    }

    override fun follow(deviceId: String) {
        _sent.value = CastSent(deviceId, null, clock())
        _remotes.tryEmit(deviceId)
    }

    override suspend fun control(deviceId: String, action: CastAction, positionMs: Long?) {
        cache.api(cache.profile()).castControl(deviceId, CastControlRequest(action, positionMs?.coerceAtLeast(0)))
    }

    override val commands: Flow<ServerEvent> = flow {
        val id = ownId?.invoke() ?: return@flow
        live.events.filter { event ->
            (event is ServerEvent.CastPlay && event.deviceId == id) || (event is ServerEvent.CastControl && event.deviceId == id)
        }.collect { emit(it) }
    }

    override fun report(report: CastReport) {
        val id = ownId ?: return
        reports.trySend(report)
        if (!reporting.compareAndSet(false, true)) return
        app.launch {
            val device = id()
            for (next in reports) {
                try {
                    cache.api(cache.profile()).reportCastState(device, next)
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // Not listed (yet, or any more), or the server is away: the next report says it again.
                }
            }
        }
    }
}
