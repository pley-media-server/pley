package app.pley.android.screens.cast

import app.pley.android.data.CastPlaying
import app.pley.android.data.CastRepository
import app.pley.android.data.CastSent
import app.pley.android.data.CastTarget
import app.pley.android.model.CastAction
import app.pley.android.model.CastPlatform
import app.pley.android.model.CastPlayState
import app.pley.android.model.CastReport
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.ServerEvent
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import java.io.IOException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/** Casting as a test drives it: the devices it sets, what was sent and steered, what was reported. */
class FakeCast(private val clock: () -> Long) : CastRepository {
    override val devices = MutableStateFlow<List<CastTarget>>(emptyList())
    override val sent = MutableStateFlow<CastSent?>(null)
    override val remotes = MutableSharedFlow<String>(extraBufferCapacity = 8)
    override val commands = MutableSharedFlow<ServerEvent>(extraBufferCapacity = 8)

    data class Played(val deviceId: String, val itemId: Long, val fileId: Long?, val positionMs: Long?)

    val played = mutableListOf<Played>()
    val controls = mutableListOf<Triple<String, CastAction, Long?>>()
    val reports = mutableListOf<CastReport>()

    /** What sending and steering throw, if anything. */
    var failure: IOException? = null

    override fun start() = Unit

    override fun now(): Long = clock()

    override suspend fun play(deviceId: String, itemId: Long, fileId: Long?, positionMs: Long?) {
        failure?.let { throw it }
        played += Played(deviceId, itemId, fileId, positionMs)
        sent.value = CastSent(deviceId, itemId, clock())
        remotes.tryEmit(deviceId)
    }

    override fun follow(deviceId: String) {
        sent.value = CastSent(deviceId, null, clock())
        remotes.tryEmit(deviceId)
    }

    override suspend fun control(deviceId: String, action: CastAction, positionMs: Long?) {
        failure?.let { throw it }
        controls += Triple(deviceId, action, positionMs)
    }

    override fun report(report: CastReport) {
        reports += report
    }

    /** Device [id] says it plays [item] (`null`: nothing), now; [answered] as the repository marks it. */
    fun says(id: String, item: ItemSummary?, positionMs: Long = 0, state: CastPlayState = CastPlayState.Playing) {
        val playing = item?.let { CastPlaying(it, positionMs, it.durationMs, state, clock()) }
        devices.update { list -> list.map { if (it.id == id) it.copy(playing = playing) else it } }
        sent.update { s -> if (s != null && s.deviceId == id && s.itemId == item?.id && item != null) s.copy(answered = true) else s }
    }

    companion object {
        val MOVIE = ItemSummary(id = 101, libraryId = 1, kind = ItemKind.Movie, title = "A movie", durationMs = 6_000_000)

        fun tv(id: String = "tv-1", name: String = "Living room") = CastTarget(id, name, CastPlatform.AndroidTv, null)
    }
}

/** The player launcher of the tests: what it was asked to play. */
class RecordingLauncher : PlayerLauncher {
    val played = mutableListOf<PlayRequest>()

    override fun play(request: PlayRequest) {
        played += request
    }
}
