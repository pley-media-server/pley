package app.pley.android.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.floatPreferencesKey
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import java.io.IOException
import kotlin.math.abs
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

/**
 * The device's settings in a DataStore. A setting never changed isn't
 * stored, so a new default reaches everyone who kept the old one. [tv]
 * picks the frame-rate choices; [debuggable] alone allows software video.
 */
internal class DataStoreSettings(
    private val store: DataStore<Preferences>,
    private val tv: Boolean,
    private val debuggable: Boolean,
) : AppSettings {
    override val settings: Flow<Settings> = store.data
        .catch { e -> if (e is IOException) emit(emptyPreferences()) else throw e }
        .map(::read)
        .distinctUntilChanged()

    override suspend fun update(change: (Settings) -> Settings) {
        store.edit { p -> write(p, allowed(change(read(p)))) }
    }

    /** [settings] as this device may have them. */
    fun allowed(settings: Settings): Settings = settings.copy(
        frameRateMatching = settings.frameRateMatching?.takeIf { it in FrameRateMatching.choices(tv) },
        passthrough = settings.passthrough.filterKeys { it in Settings.PASSTHROUGH_FORMATS },
        subtitleSize = Settings.SUBTITLE_SIZES.minBy { abs(it - settings.subtitleSize) },
        subtitleRaise = Settings.SUBTITLE_RAISES.minBy { abs(it - settings.subtitleRaise) },
        allowSoftwareVideo = settings.allowSoftwareVideo && debuggable,
        qualityOnHomeNetwork = settings.qualityOnHomeNetwork.ifBlank { Settings.AUTOMATIC },
        qualityOnMobile = settings.qualityOnMobile.ifBlank { Settings.AUTOMATIC },
        downloadQuality = settings.downloadQuality.ifBlank { Settings.ORIGINAL },
    )

    private fun read(p: Preferences): Settings {
        val defaults = Settings()
        return allowed(
            Settings(
                qualityOnHomeNetwork = p[QUALITY_HOME] ?: defaults.qualityOnHomeNetwork,
                qualityOnMobile = p[QUALITY_MOBILE] ?: defaults.qualityOnMobile,
                frameRateMatching = p[FRAME_RATE]?.let { enumOrNull<FrameRateMatching>(it) },
                passthrough = Settings.PASSTHROUGH_FORMATS.mapNotNull { format ->
                    p[passthroughKey(format)]?.let { enumOrNull<Passthrough>(it) }?.let { format to it }
                }.toMap(),
                tunneling = p[TUNNELING] ?: defaults.tunneling,
                subtitleSize = p[SUBTITLE_SIZE] ?: defaults.subtitleSize,
                subtitleBackground = p[SUBTITLE_BACKGROUND]?.let { enumOrNull<SubtitleBackground>(it) } ?: defaults.subtitleBackground,
                subtitleFont = p[SUBTITLE_FONT]?.let { enumOrNull<SubtitleFont>(it) } ?: defaults.subtitleFont,
                subtitleRaise = p[SUBTITLE_RAISE] ?: defaults.subtitleRaise,
                autoplayNext = p[AUTOPLAY_NEXT] ?: defaults.autoplayNext,
                privateSignInTab = p[PRIVATE_TAB] ?: defaults.privateSignInTab,
                alwaysStartAs = p[ALWAYS_START_AS],
                showPlaybackDetails = p[PLAYBACK_DETAILS] ?: defaults.showPlaybackDetails,
                gridDensity = p[GRID_DENSITY]?.let { enumOrNull<GridDensity>(it) } ?: defaults.gridDensity,
                homeBanner = p[HOME_BANNER] ?: defaults.homeBanner,
                reduceMotion = p[REDUCE_MOTION] ?: defaults.reduceMotion,
                allowSoftwareVideo = p[SOFTWARE_VIDEO] ?: defaults.allowSoftwareVideo,
                downloadOnMobileData = p[DOWNLOAD_ON_MOBILE] ?: defaults.downloadOnMobileData,
                downloadQuality = p[DOWNLOAD_QUALITY] ?: defaults.downloadQuality,
            ),
        )
    }

    /** Keeps what differs from the defaults; removes the rest. */
    private fun write(p: MutablePreferences, s: Settings) {
        val d = Settings()
        p.keep(QUALITY_HOME, s.qualityOnHomeNetwork, d.qualityOnHomeNetwork)
        p.keep(QUALITY_MOBILE, s.qualityOnMobile, d.qualityOnMobile)
        p.keep(FRAME_RATE, s.frameRateMatching?.name, null)
        for (format in Settings.PASSTHROUGH_FORMATS) {
            p.keep(passthroughKey(format), s.passthrough[format]?.takeIf { it != Passthrough.Auto }?.name, null)
        }
        p.keep(TUNNELING, s.tunneling, d.tunneling)
        p.keep(SUBTITLE_SIZE, s.subtitleSize, d.subtitleSize)
        p.keep(SUBTITLE_BACKGROUND, s.subtitleBackground.name, d.subtitleBackground.name)
        p.keep(SUBTITLE_FONT, s.subtitleFont.name, d.subtitleFont.name)
        p.keep(SUBTITLE_RAISE, s.subtitleRaise, d.subtitleRaise)
        p.keep(AUTOPLAY_NEXT, s.autoplayNext, d.autoplayNext)
        p.keep(PRIVATE_TAB, s.privateSignInTab, d.privateSignInTab)
        p.keep(ALWAYS_START_AS, s.alwaysStartAs, null)
        p.keep(PLAYBACK_DETAILS, s.showPlaybackDetails, d.showPlaybackDetails)
        p.keep(GRID_DENSITY, s.gridDensity.name, d.gridDensity.name)
        p.keep(HOME_BANNER, s.homeBanner, d.homeBanner)
        p.keep(REDUCE_MOTION, s.reduceMotion, d.reduceMotion)
        p.keep(SOFTWARE_VIDEO, s.allowSoftwareVideo, d.allowSoftwareVideo)
        p.keep(DOWNLOAD_ON_MOBILE, s.downloadOnMobileData, d.downloadOnMobileData)
        p.keep(DOWNLOAD_QUALITY, s.downloadQuality, d.downloadQuality)
    }

    private fun <T : Any> MutablePreferences.keep(key: Preferences.Key<T>, value: T?, default: T?) {
        if (value == null || value == default) remove(key) else set(key, value)
    }

    companion object {
        /** The DataStore's file, in filesDir. */
        const val FILE = "datastore/settings.preferences_pb"

        private val QUALITY_HOME = stringPreferencesKey("quality_home_network")
        private val QUALITY_MOBILE = stringPreferencesKey("quality_mobile")
        private val FRAME_RATE = stringPreferencesKey("frame_rate_matching")
        private val TUNNELING = booleanPreferencesKey("tunneling")
        private val SUBTITLE_SIZE = floatPreferencesKey("subtitle_size")
        private val SUBTITLE_BACKGROUND = stringPreferencesKey("subtitle_background")
        private val SUBTITLE_FONT = stringPreferencesKey("subtitle_font")
        private val SUBTITLE_RAISE = floatPreferencesKey("subtitle_raise")
        private val AUTOPLAY_NEXT = booleanPreferencesKey("autoplay_next")
        private val PRIVATE_TAB = booleanPreferencesKey("private_sign_in_tab")
        private val ALWAYS_START_AS = longPreferencesKey("always_start_as")
        private val PLAYBACK_DETAILS = booleanPreferencesKey("playback_details")
        private val GRID_DENSITY = stringPreferencesKey("grid_density")
        private val HOME_BANNER = booleanPreferencesKey("home_banner")
        private val REDUCE_MOTION = booleanPreferencesKey("reduce_motion")
        private val SOFTWARE_VIDEO = booleanPreferencesKey("allow_software_video")
        private val DOWNLOAD_ON_MOBILE = booleanPreferencesKey("download_on_mobile_data")
        private val DOWNLOAD_QUALITY = stringPreferencesKey("download_quality")

        private fun passthroughKey(format: String) = stringPreferencesKey("passthrough:$format")

        private inline fun <reified E : Enum<E>> enumOrNull(name: String): E? = enumValues<E>().firstOrNull { it.name == name }
    }
}
