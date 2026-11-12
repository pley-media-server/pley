package app.pley.android.screens.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.auth.Profile
import app.pley.android.data.AppSettings
import app.pley.android.data.FrameRateMatching
import app.pley.android.data.GridDensity
import app.pley.android.data.Passthrough
import app.pley.android.data.Settings
import app.pley.android.data.SubtitleBackground
import app.pley.android.data.SubtitleFont
import app.pley.android.player.session.Qualities
import app.pley.android.player.session.QualityPreset
import app.pley.android.screens.onboarding.FileHouseholdStore
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** The app's settings as the Settings screen shows them. */
data class SettingsUiState(
    val settings: Settings = Settings(),
    /** Read from the device yet: until then the screen shows its rows still. */
    val loaded: Boolean = false,
    /** A TV: refresh rate Auto/Always/Off, passthrough, tunneling and "Always start as". */
    val tv: Boolean = false,
    /** The household as the device last saw it, for "Always start as" (TVs); empty when unknown. */
    val profiles: List<Profile> = emptyList(),
    /** Debug builds may let emulators decode video in software. */
    val debuggable: Boolean = false,
) {
    /** The refresh-rate choice in force: the person's, else the device's default. */
    val frameRate: FrameRateMatching get() = settings.frameRateMatching(tv)

    val frameRateChoices: List<FrameRateMatching> get() = FrameRateMatching.choices(tv)
}

/**
 * The device's own settings (not the profile's, which live on the
 * server): quality per network, matching the refresh rate, passthrough per
 * sound format and tunneling (TVs), the subtitles' size and background,
 * autoplay, the private sign-in tab, the grid's density, the playback
 * details, and the profile a TV starts as. Each change is saved at once.
 */
class SettingsViewModel(
    private val settings: AppSettings,
    tv: Boolean,
    debuggable: Boolean = false,
    profiles: suspend () -> List<Profile> = { emptyList() },
) : ViewModel() {
    private val household = MutableStateFlow<List<Profile>>(emptyList())
    private val channel = Channel<Throwable>(Channel.BUFFERED)

    /** Changes that couldn't be saved. */
    val errors: Flow<Throwable> = channel.receiveAsFlow()

    val state: StateFlow<SettingsUiState> = combine(settings.settings, household) { s, people ->
        SettingsUiState(settings = s, loaded = true, tv = tv, profiles = people, debuggable = debuggable)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), SettingsUiState(tv = tv, debuggable = debuggable))

    init {
        if (tv) {
            viewModelScope.launch {
                household.value = try {
                    profiles()
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    emptyList()
                }
            }
        }
    }

    /** Saves [change] of the settings. */
    fun update(change: (Settings) -> Settings) {
        viewModelScope.launch {
            try {
                settings.update(change)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                channel.send(e)
            }
        }
    }

    fun setQuality(mobile: Boolean, preset: QualityPreset) =
        update { if (mobile) it.copy(qualityOnMobile = idOf(preset)) else it.copy(qualityOnHomeNetwork = idOf(preset)) }

    fun setFrameRate(choice: FrameRateMatching) = update { it.copy(frameRateMatching = choice) }

    fun setPassthrough(format: String, choice: Passthrough) = update { it.copy(passthrough = it.passthrough + (format to choice)) }

    fun setTunneling(on: Boolean) = update { it.copy(tunneling = on) }

    fun setSubtitleSize(size: Float) = update { it.copy(subtitleSize = size) }

    fun setSubtitleBackground(background: SubtitleBackground) = update { it.copy(subtitleBackground = background) }

    fun setSubtitleFont(font: SubtitleFont) = update { it.copy(subtitleFont = font) }

    fun setSubtitleRaise(raise: Float) = update { it.copy(subtitleRaise = raise) }

    fun setAutoplayNext(on: Boolean) = update { it.copy(autoplayNext = on) }

    fun setPrivateTab(on: Boolean) = update { it.copy(privateSignInTab = on) }

    fun setGridDensity(density: GridDensity) = update { it.copy(gridDensity = density) }

    fun setPlaybackDetails(on: Boolean) = update { it.copy(showPlaybackDetails = on) }

    fun setReduceMotion(on: Boolean) = update { it.copy(reduceMotion = on) }

    fun setHomeBanner(on: Boolean) = update { it.copy(homeBanner = on) }

    fun setAlwaysStartAs(profileId: Long?) = update { it.copy(alwaysStartAs = profileId) }

    fun setSoftwareVideo(on: Boolean) = update { it.copy(allowSoftwareVideo = on) }

    fun setDownloadOnMobileData(on: Boolean) = update { it.copy(downloadOnMobileData = on) }

    /** The quality downloads are made at: the original, or a preset the server makes them at. */
    fun setDownloadQuality(preset: QualityPreset) = update { it.copy(downloadQuality = idOf(preset)) }

    companion object {
        /** The qualities offered for playing, in the player's order: Original, Automatic, then the transcodes. */
        val QUALITIES: List<QualityPreset> get() = Qualities.ALL

        /** The qualities downloads are made at: the original, then the transcodes of 1080p and below. */
        val DOWNLOAD_QUALITIES: List<QualityPreset> get() = listOf(Qualities.ORIGINAL) + Qualities.TRANSCODES.filter { (it.maxHeight ?: 0) <= 1080 }

        /** What the settings keep for [preset]: "Original", or its id ("auto", "8m"). */
        fun idOf(preset: QualityPreset): String = if (preset.original) Settings.ORIGINAL else preset.id

        /** The preset a setting's value names. */
        fun presetOf(value: String): QualityPreset = Qualities.byIdOrLabel(value)

        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY]
                val debuggable = app != null && (app.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0
                SettingsViewModel(
                    settings = g.data.settings,
                    tv = g.device.isTv,
                    debuggable = debuggable,
                    profiles = {
                        val server = g.data.servers.active.value
                        if (app == null || server == null) emptyList() else FileHouseholdStore.of(app).read(server).orEmpty()
                    },
                )
            }
        }
    }
}
