package app.pley.android.player.engine

import app.pley.android.caps.CapsBuilder
import app.pley.android.caps.CapsDetails
import app.pley.android.caps.DisplayModeInfo
import app.pley.android.caps.DisplayState
import app.pley.android.caps.MatchContent
import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaStream
import kotlin.math.abs
import kotlin.math.floor

/** The person's choice for matching the TV's refresh rate to the video (Settings → Playback). */
enum class RefreshRateMatching {
    /** As the device's own setting says (Android 12 and up); always below. The default. */
    Auto,

    /** Whatever the device's setting says. */
    Always,

    Off,
}

/** What a video asks of the display: its frame rate, length and HDR. */
data class MatchRequest(
    val fps: Double?,
    val durationMs: Long?,
    /** Display.HdrCapabilities types a mode must list (any one of them) on Android 14 and up; empty for SDR. */
    val hdrTypes: Set<Int> = emptySet(),
    /** The video comes out as Dolby Vision. */
    val dolbyVision: Boolean = false,
    val setting: RefreshRateMatching = RefreshRateMatching.Auto,
    /** Switching for Dolby Vision below Android 14 is known to be safe on this model. */
    val dvSwitchAllowed: Boolean = false,
    /** Auto stopped asking on this display after two timeouts in a row. */
    val gaveUp: Boolean = false,
)

/** Why the display mode stays as it is. */
enum class KeepReason {
    Off,

    /** Under 10 minutes: a trailer or a clip isn't worth a blank screen. */
    Short,
    UnknownRate,

    /** Dolby Vision below Android 14, on a model not known to switch safely. */
    DolbyVisionOldAndroid,

    /** Auto, and the device's own setting is Never. */
    SystemNever,

    /** Auto, and two requests in a row timed out on this display. */
    GaveUp,
    AlreadyMatching,
    NoMatch,
    NoDisplay,
}

sealed interface ModeDecision {
    data class Switch(val modeId: Int, val refreshRate: Float) : ModeDecision

    data class Keep(val reason: KeepReason) : ModeDecision
}

/**
 * Which display mode plays a video without judder. A mode matches when it
 * has the current mode's physical size and a refresh rate equal to the
 * frame rate, within 0.01 Hz, or to an integer multiple of it up to 120 Hz:
 * 23.976 fps never matches 24 Hz. The exact rate is preferred, then the
 * lowest multiple. On Android 14 and up, modes must list the video's HDR
 * type. Nothing changes for items under 10 minutes, or when the current
 * mode already matches.
 */
object ModeChoice {
    const val MIN_DURATION_MS = 10 * 60 * 1000L
    const val TOLERANCE_HZ = 0.01
    const val MAX_HZ = 120.0

    /** Android 12: the device's match-content setting exists. */
    const val SDK_MATCH_CONTENT = 31

    /** Android 14: modes list their HDR types. */
    const val SDK_MODE_HDR = 34

    // Display.HdrCapabilities.HDR_TYPE_*.
    const val HDR_DOLBY_VISION = 1
    const val HDR_HDR10 = 2
    const val HDR_HLG = 3
    const val HDR_HDR10_PLUS = 4

    /** The multiple of [fps] that [refreshRate] is (1 for the same rate); `null` when none up to 120 Hz. */
    fun multipleOf(refreshRate: Float, fps: Double): Int? {
        if (fps <= 0 || refreshRate <= 0) return null
        val most = floor((MAX_HZ + TOLERANCE_HZ) / fps).toInt()
        return (1..most).firstOrNull { k -> abs(refreshRate / k - fps) <= TOLERANCE_HZ }
    }

    fun decide(display: DisplayState?, request: MatchRequest, sdk: Int): ModeDecision {
        if (request.setting == RefreshRateMatching.Off) return ModeDecision.Keep(KeepReason.Off)
        val duration = request.durationMs
        if (duration == null || duration < MIN_DURATION_MS) return ModeDecision.Keep(KeepReason.Short)
        val fps = request.fps?.takeIf { it > 1.0 } ?: return ModeDecision.Keep(KeepReason.UnknownRate)
        if (request.dolbyVision && sdk < SDK_MODE_HDR && !request.dvSwitchAllowed) {
            return ModeDecision.Keep(KeepReason.DolbyVisionOldAndroid)
        }
        val auto = request.setting == RefreshRateMatching.Auto
        if (auto && request.gaveUp) return ModeDecision.Keep(KeepReason.GaveUp)
        val preference = if (auto && sdk >= SDK_MATCH_CONTENT) display?.matchContent ?: MatchContent.Unknown else MatchContent.Always
        if (preference == MatchContent.Never) return ModeDecision.Keep(KeepReason.SystemNever)
        val active = display?.activeMode ?: return ModeDecision.Keep(KeepReason.NoDisplay)

        val showsHdr: (DisplayModeInfo) -> Boolean = { mode ->
            val listed = mode.hdrTypes
            request.hdrTypes.isEmpty() || sdk < SDK_MODE_HDR || listed == null || listed.any { it in request.hdrTypes }
        }
        if (multipleOf(active.refreshRate, fps) != null && showsHdr(active)) return ModeDecision.Keep(KeepReason.AlreadyMatching)

        val seamless = active.alternativeRefreshRates
        val best = display.modes
            .asSequence()
            .filter { it.width == active.width && it.height == active.height }
            .filter(showsHdr)
            .filter { mode -> preference != MatchContent.SeamlessOnly || mode.id == active.id || seamless.any { abs(it - mode.refreshRate) <= TOLERANCE_HZ } }
            .mapNotNull { mode -> multipleOf(mode.refreshRate, fps)?.let { mode to it } }
            .sortedWith(compareBy<Pair<DisplayModeInfo, Int>> { it.second }.thenBy { it.first.id != active.id })
            .firstOrNull()
            ?.first
            ?: return ModeDecision.Keep(KeepReason.NoMatch)
        if (best.id == active.id) return ModeDecision.Keep(KeepReason.AlreadyMatching)
        return ModeDecision.Switch(best.id, best.refreshRate)
    }

    /**
     * Whether [display] shows Dolby Vision in the mode a video plays in:
     * the one [asked] switches to, else the one in use; `null` without a
     * display. Media3 asks about the mode in use when the decoder starts,
     * which on a TV comes before the switch to the video's mode. Dolby Vision
     * a quirk adds, which no mode lists, is shown in every mode; turned off
     * in the system's settings ([turnedOff], as the display told before), in
     * none.
     */
    fun dolbyVisionShown(
        caps: CapsDetails,
        asked: MatchRequest?,
        sdk: Int,
        display: DisplayState? = caps.input?.display,
        turnedOff: Set<Int> = emptySet(),
    ): Boolean? {
        if (display == null) return null
        if (DynamicRange.DolbyVision !in caps.screenHdrTypes || HDR_DOLBY_VISION in turnedOff) return false
        val shownIn = { mode: DisplayModeInfo? -> DynamicRange.DolbyVision in CapsBuilder.hdrTypesShownIn(display, mode) }
        if (display.modes.none(shownIn)) return true
        val switchTo = (asked?.let { decide(display, it, sdk) } as? ModeDecision.Switch)?.modeId
        return shownIn(display.modes.firstOrNull { it.id == switchTo } ?: display.activeMode)
    }

    /**
     * What [video] asks of a TV's display; `null` where the mode stays as it
     * is: a phone, matching turned off, an unknown frame rate, an item under
     * 10 minutes.
     */
    fun matchFor(
        tv: Boolean,
        video: MediaStream?,
        durationMs: Long?,
        caps: CapsDetails?,
        setting: RefreshRateMatching,
        gaveUp: Boolean = false,
    ): MatchRequest? {
        if (!tv || setting == RefreshRateMatching.Off || video == null) return null
        if ((video.frameRate ?: 0.0) <= 1.0 || (durationMs ?: 0) < MIN_DURATION_MS) return null
        return requestFor(video, durationMs, caps, setting, gaveUp)
    }

    /**
     * What [video] asks of the display here: its frame rate, and the HDR
     * types a mode must list. Dolby Vision comes out as Dolby Vision when
     * the screen shows it and a decoder plays its profile; otherwise as its
     * base layer (HDR10, HLG or SDR). HDR10+ is happy with HDR10 modes.
     */
    fun requestFor(
        video: MediaStream?,
        durationMs: Long?,
        caps: CapsDetails?,
        setting: RefreshRateMatching,
        gaveUp: Boolean = false,
    ): MatchRequest {
        val range = video?.dynamicRange
        val screen = caps?.screenHdrTypes.orEmpty()
        val dvOut = range == DynamicRange.DolbyVision && DynamicRange.DolbyVision in screen &&
            caps?.dolbyVisionProfiles.orEmpty().any { it.profile == video.dvProfile }
        val types = when {
            dvOut -> setOf(HDR_DOLBY_VISION)
            range == DynamicRange.DolbyVision -> when (video.dvBlCompat) {
                1, 6 -> setOf(HDR_HDR10)
                4 -> setOf(HDR_HLG)
                else -> emptySet()
            }
            range == DynamicRange.Hdr10Plus -> setOf(HDR_HDR10_PLUS, HDR_HDR10)
            range == DynamicRange.Hdr10 -> setOf(HDR_HDR10)
            range == DynamicRange.Hlg -> setOf(HDR_HLG)
            else -> emptySet()
        }
        return MatchRequest(
            fps = video?.frameRate,
            durationMs = durationMs,
            hdrTypes = types,
            dolbyVision = dvOut,
            setting = setting,
            dvSwitchAllowed = caps?.dvModeSwitchAllowed == true,
            gaveUp = gaveUp,
        )
    }
}
