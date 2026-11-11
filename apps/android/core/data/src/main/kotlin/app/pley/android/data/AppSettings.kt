package app.pley.android.data

import kotlinx.coroutines.flow.Flow

/** Matching the TV's display mode to the video. */
enum class FrameRateMatching {
    /** Never. */
    Off,

    /** Phones: only when the switch is seamless. */
    Seamless,

    /** TVs: for videos of 10 minutes or more, following the TV's own setting. */
    Auto,

    /** TVs: always. */
    Always,
    ;

    companion object {
        /** The choices a phone ([tv] false) or a TV offers, the default first. */
        fun choices(tv: Boolean): List<FrameRateMatching> = if (tv) listOf(Auto, Always, Off) else listOf(Seamless, Off)

        /** What a device does without being asked: Seamless on phones, Auto on TVs. */
        fun default(tv: Boolean): FrameRateMatching = if (tv) Auto else Seamless
    }
}

/** Sending a sound format to the receiver as it is, or decoding it here. */
enum class Passthrough { Auto, On, Off }

/** Subtitle backgrounds. */
enum class SubtitleBackground { None, DarkBox }

/** Subtitle typefaces: the system's sans serif (the default), serif and monospace, and its casual hand where it has one. */
enum class SubtitleFont { Sans, Serif, Monospace, Casual }

/** How big the cards of a grid are: more small ones, or fewer large ones. */
enum class GridDensity { Small, Medium, Large }

/**
 * This device's settings (not the profile's: those live on the server).
 * Defaults are what the app does without being asked.
 */
data class Settings(
    /** "auto" (Automatic, the default), "Original" or a preset's id, on Wi-Fi or Ethernet. */
    val qualityOnHomeNetwork: String = AUTOMATIC,
    /** "auto" (Automatic, the default), "Original" or a preset's id, on mobile data. */
    val qualityOnMobile: String = AUTOMATIC,
    /** `null`: the device's default, Seamless on phones and Auto on TVs ([frameRateMatching] of a device). */
    val frameRateMatching: FrameRateMatching? = null,
    /** Per format of [PASSTHROUGH_FORMATS]; a format left out is Auto. */
    val passthrough: Map<String, Passthrough> = emptyMap(),
    /** TVs, under Advanced. */
    val tunneling: Boolean = false,
    /** A share of the video's height: one of [SUBTITLE_SIZES]. */
    val subtitleSize: Float = 0.048f,
    val subtitleBackground: SubtitleBackground = SubtitleBackground.None,
    val subtitleFont: SubtitleFont = SubtitleFont.Sans,
    /** How far text subtitles sit above the bottom, a share of the video's height: one of [SUBTITLE_RAISES]. */
    val subtitleRaise: Float = 0f,
    val autoplayNext: Boolean = true,
    /** Sign in from a private browser tab, which keeps no cookie behind. */
    val privateSignInTab: Boolean = true,
    /** TVs: skip "Who's watching?" and start as this profile. */
    val alwaysStartAs: Long? = null,
    val showPlaybackDetails: Boolean = false,
    val gridDensity: GridDensity = GridDensity.Medium,
    /** TVs: Home opens on its banner of featured titles; off (the default), on its rows. Phones always show theirs. */
    val homeBanner: Boolean = false,
    /** Move less (design spec §5.3), whatever the system says: Fire OS has no "Remove animations". */
    val reduceMotion: Boolean = false,
    /** Debug builds only (emulators): let software decoders play video beyond what the device's policy allows. */
    val allowSoftwareVideo: Boolean = false,
    /** Downloads go on over mobile data too; else they wait for Wi-Fi or Ethernet. */
    val downloadOnMobileData: Boolean = false,
    /**
     * "Original", or a preset's name: downloads the server makes at that
     * quality (smaller), else the original when it plays here as it is.
     */
    val downloadQuality: String = ORIGINAL,
) {
    /** The frame-rate matching a phone ([tv] false) or a TV uses: the choice, if it is one of the device's, else its default. */
    fun frameRateMatching(tv: Boolean): FrameRateMatching =
        frameRateMatching?.takeIf { it in FrameRateMatching.choices(tv) } ?: FrameRateMatching.default(tv)

    /** The passthrough choice for [format] ("eac3"…). */
    fun passthrough(format: String): Passthrough = passthrough[format] ?: Passthrough.Auto

    companion object {
        const val ORIGINAL = "Original"

        /** Automatic: the original, lower only when it doesn't play well (docs/QUALITY.md). */
        const val AUTOMATIC = "auto"

        /** Subtitle sizes offered: 3.8, 4.8, 6 and 7.6 % of the video's height. */
        val SUBTITLE_SIZES: List<Float> = listOf(0.038f, 0.048f, 0.06f, 0.076f)

        /** Subtitle positions offered: at the bottom, then raised by 5, 10 and 15 % of the video's height. */
        val SUBTITLE_RAISES: List<Float> = listOf(0f, 0.05f, 0.1f, 0.15f)

        /**
         * The sound formats with a passthrough choice: Dolby Digital, Dolby
         * Digital Plus, Atmos (in Dolby Digital Plus), TrueHD, DTS and DTS-HD.
         */
        val PASSTHROUGH_FORMATS: List<String> = listOf("ac3", "eac3", "eac3-joc", "truehd", "dts", "dts-hd")
    }
}

interface AppSettings {
    val settings: Flow<Settings>

    /**
     * Changes the settings. What can't be (a size not offered, a frame-rate
     * choice of the other kind of device, software video outside debug
     * builds) is put back to what it may be.
     */
    suspend fun update(change: (Settings) -> Settings)
}
