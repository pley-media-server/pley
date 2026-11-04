package app.pley.android.player.session

import android.annotation.SuppressLint
import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.SystemClock
import app.pley.android.caps.DeviceCapabilities
import app.pley.android.caps.PlaybackCapsPolicy
import app.pley.android.data.AppSettings
import app.pley.android.data.ItemRepository
import app.pley.android.data.Loadable
import app.pley.android.data.LocalMedia
import app.pley.android.model.ItemDetail
import app.pley.android.model.ServerEvent
import app.pley.android.model.StreamChoice
import app.pley.android.network.OfflineException
import app.pley.android.network.PleyApi
import app.pley.android.player.subtitles.Media3VttParser
import app.pley.android.player.subtitles.PictureSource
import app.pley.android.player.subtitles.SubtitleOffsets
import app.pley.android.player.subtitles.SubtitleFetcher
import app.pley.android.player.subtitles.VttParser
import app.pley.android.player.trickplay.RegionSheetDecoder
import app.pley.android.player.trickplay.SheetDecoder
import app.pley.android.player.trickplay.SheetSource
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull

/** An item's details for playing: what was saved first, else the server's. */
fun interface ItemDetails {
    suspend fun detail(itemId: Long): ItemDetail

    companion object {
        /** From the server only. */
        fun of(api: PleyApi): ItemDetails = ItemDetails { api.item(it) }

        /**
         * The repository's first answer (its snapshot, so playing starts
         * without waiting on the network), else the server's. A copy that is
         * being asked for again (older, or out of date since tracks were
         * picked or the profile's defaults changed) names the tracks it
         * started with then, and an old resume point: the server's answer
         * is waited for [FRESH_WAIT_MS] at most.
         */
        fun snapshotFirst(items: ItemRepository, api: PleyApi): ItemDetails = ItemDetails { id ->
            var before = true
            val first = try {
                items.detail(id).first { state ->
                    when {
                        state is Loadable.Loading -> false
                        // The cache shows what it held at once, a failure from before
                        // included, while it asks again: its new answer decides.
                        before && state is Loadable.Failed && state.stale == null && state.error is OfflineException -> {
                            before = false
                            false
                        }
                        else -> true
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                null
            }
            when (first) {
                is Loadable.Ready -> if (first.fresh) first.value else latest(items, id) ?: first.value
                // Out of reach, asking again would only wait as long again.
                is Loadable.Failed -> first.stale ?: if (first.error is OfflineException) throw first.error else api.item(id)
                else -> api.item(id)
            }
        }

        /** The server's answer for [id] if it comes within [FRESH_WAIT_MS], else `null`. */
        private suspend fun latest(items: ItemRepository, id: Long): ItemDetail? = try {
            withTimeoutOrNull(FRESH_WAIT_MS) {
                when (val answer = items.detail(id).first { (it is Loadable.Ready && it.fresh) || it is Loadable.Failed }) {
                    is Loadable.Ready -> answer.value
                    else -> null
                }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }

        /** How long a playback waits for the server's details when what is held may be out of date. */
        const val FRESH_WAIT_MS = 1_500L
    }
}

/** Whether the device is on its home network (Wi-Fi, Ethernet) or on mobile data. */
enum class NetworkKind { Home, Mobile }

/** The quality chosen last, per device and network kind (Settings.qualityOnHomeNetwork, qualityOnMobile). */
interface QualityMemory {
    suspend fun remembered(): String?

    suspend fun remember(id: String)

    companion object {
        /** Remembers nothing: Automatic, as where no quality was ever chosen. */
        val NONE = object : QualityMemory {
            override suspend fun remembered(): String? = null

            override suspend fun remember(id: String) = Unit
        }

        /** In the app's settings, one per network kind. */
        fun of(settings: AppSettings, network: () -> NetworkKind): QualityMemory = object : QualityMemory {
            override suspend fun remembered(): String? {
                val s = settings.settings.first()
                return if (network() == NetworkKind.Mobile) s.qualityOnMobile else s.qualityOnHomeNetwork
            }

            override suspend fun remember(id: String) {
                val mobile = network() == NetworkKind.Mobile
                try {
                    settings.update { if (mobile) it.copy(qualityOnMobile = id) else it.copy(qualityOnHomeNetwork = id) }
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // Not saved: the next video plays the original.
                }
            }
        }
    }
}

/**
 * Android's view of the network: mobile data when the default network is
 * cellular. ACCESS_NETWORK_STATE is :core:network's (its manifest declares it).
 */
@SuppressLint("MissingPermission")
fun networkKindOf(context: Context): () -> NetworkKind {
    val connectivity = context.applicationContext.getSystemService(ConnectivityManager::class.java)
    return {
        val caps = connectivity?.activeNetwork?.let { connectivity.getNetworkCapabilities(it) }
        if (caps?.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) == true) NetworkKind.Mobile else NetworkKind.Home
    }
}

/**
 * This installation, as the server tells players apart (starting a session
 * ends the earlier ones of the same client). The old player's id is kept:
 * it lives in the same place (the "device" preferences' "client_id").
 */
fun clientIdOf(context: Context): () -> String {
    val app = context.applicationContext
    val cached = lazy {
        val prefs = app.getSharedPreferences("device", Context.MODE_PRIVATE)
        prefs.getString("client_id", null) ?: "android-${UUID.randomUUID()}".also { prefs.edit().putString("client_id", it).apply() }
    }
    return { cached.value }
}

/**
 * Everything a playback session uses, so tests can give fakes: the server,
 * the device's capabilities, the engine, settings and the clock.
 */
class PlaybackEnvironment(
    val api: PleyApi,
    val capabilities: DeviceCapabilities,
    val policy: PlaybackCapsPolicy,
    val details: ItemDetails,
    /** Makes the engine link for a playback (on the main thread). */
    val engines: () -> EngineLink,
    val texts: PlayerTexts,
    val clientId: () -> String,
    val reporter: SessionReporter,
    val subtitles: SubtitleFetcher,
    val quality: QualityMemory = QualityMemory.NONE,
    /** What Automatic found the network carries, on this device for this server, for the next titles. */
    val ceiling: CeilingMemory = CeilingMemory.memory(),
    /** Seek previews' sheets; `null` for none. */
    val sheets: SheetSource? = null,
    val sheetDecoder: SheetDecoder = RegionSheetDecoder,
    /** Whether what comes next starts by itself after its countdown (Settings.autoplayNext). */
    val autoplayNext: suspend () -> Boolean = { true },
    /** The smallest TVs: fewer tiles, in RGB_565. */
    val lowMemory: Boolean = false,
    val parser: VttParser = Media3VttParser,
    /** A monotonic clock in ms, for stalls and dropped frames. */
    val now: () -> Long = SystemClock::elapsedRealtime,
    /** Where subtitles are parsed and seek previews cut, off the main thread. */
    val work: CoroutineDispatcher = Dispatchers.Default,
    /** Subtitle offsets per file and profile, on this device. */
    val offsets: SubtitleOffsets = SubtitleOffsets.memory(),
    /** The profile watching: its subtitle offsets are its own. `null`: not known (offsets kept per file alone). */
    val profileId: Long? = null,
    /** Reads picture subtitles beside a stream (the server's .mks); `null`: they are burned in instead. */
    val pictures: PictureSource? = null,
    /** The server's events, to hear of this playback being stopped, and the remote's commands. */
    val events: Flow<ServerEvent> = emptyFlow(),
    /**
     * Told whether a playback has a session on the server now (LiveUpdates.playing):
     * the event stream stays open in the background meanwhile, for the remote.
     */
    val playing: (player: Any, playing: Boolean) -> Unit = { _, _ -> },
    /** Told which session a playback plays now, `null` once none (RemoteRepository.here): no remote shows it here. */
    val here: (player: Any, sessionId: String?) -> Unit = { _, _ -> },
    /** Downloads: files played as they are on the device, without the server. */
    val local: LocalMedia = LocalMedia.None,
    /**
     * A video on a drive plugged into the TV, played from [local] as a
     * download is: no server is behind it, so nothing only a server does is
     * asked for (conversions, seek previews, saved track choices), and its
     * tracks and chapters come from Media3 as it plays.
     */
    val drive: Boolean = false,
    /**
     * Tracks picked in the player, saved by it (PUT /api/items/{id}/streams):
     * what the app holds of the item, and of the show's other episodes, is
     * told (WatchStateRepository.streamsChosen).
     */
    val tracksChosen: (itemId: Long, choice: StreamChoice) -> Unit = { _, _ -> },
    /**
     * Asks for a stream made ready beside the one playing as the engine
     * will, so the server makes it first; [StreamWarmer.NONE] moves over at once.
     */
    val warmer: StreamWarmer = StreamWarmer.NONE,
)
