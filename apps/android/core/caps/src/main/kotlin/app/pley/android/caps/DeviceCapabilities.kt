package app.pley.android.caps

import android.content.Context
import app.pley.android.common.DeviceProfile
import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemDetail
import app.pley.android.model.caps.ClientCaps
import app.pley.android.model.caps.DolbyVisionCaps
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.Serializable

/**
 * What this device plays right now, for the screen and audio output in
 * use: the report the server decides from, and the lines Diagnostics shows.
 */
@Serializable
data class CapsSnapshot(
    /** Sent with every playback request (version 1 and 2 fields filled). */
    val report: ClientCaps,
    /** For Diagnostics: decoders, HDR types, Dolby Vision profiles, passthrough, display modes, quirks. */
    val lines: List<String>,
    /** What the report was made from, and what the player needs beyond it. */
    val details: CapsDetails = CapsDetails(),
) {
    /**
     * This snapshot for a video the screen doesn't show as Dolby Vision (the
     * display mode it plays in doesn't): Dolby Vision left out of what the
     * screen shows, its decoders' profiles kept, so the server sends what
     * plays and says the screen is why.
     */
    fun withoutDolbyVisionScreen(): CapsSnapshot {
        if (DynamicRange.DolbyVision !in report.hdrTypes.orEmpty() + details.screenHdrTypes) return this
        return copy(
            report = report.copy(hdrTypes = report.hdrTypes?.minus(DynamicRange.DolbyVision), dolbyVision = false),
            details = details.copy(screenHdrTypes = details.screenHdrTypes - DynamicRange.DolbyVision),
        )
    }
}

/** The parts of a probe the player and Diagnostics read, besides the report. */
@Serializable
data class CapsDetails(
    /** What the probe read, before any rule; `null` in snapshots made by hand. */
    val input: ProbeInput? = null,
    /** Which video decoders the player may use. */
    val policy: DecoderPolicy = DecoderPolicy(tv = false),
    /** "T0", "T1" or "T2" ([app.pley.android.common.Tier]). */
    val tier: String = "",
    /** What the screen shows, quirks applied, whether a decoder can feed it or not. */
    val screenHdrTypes: List<DynamicRange> = emptyList(),
    /** Dolby Vision profiles decoded, quirks applied, whatever the screen. */
    val dolbyVisionProfiles: List<DolbyVisionCaps> = emptyList(),
    /** Media3's C.ENCODING_* values sent as they are, quirks and overrides applied. */
    val passthroughEncodings: List<Int> = emptyList(),
    /** The ids of the quirks that apply here. */
    val quirks: List<String> = emptyList(),
    val flags: QuirkFlags = QuirkFlags(),
) {
    /** Dolby Vision streams that also carry HDR10+ fail on this model. */
    val dvHdr10PlusHybridsBroken: Boolean get() = flags.dvHdr10PlusHybridsBroken == true

    /** Whether to force async MediaCodec queueing: on Fire TV, unless the model's kill switch says no. */
    val forceAsyncQueueing: Boolean get() = input?.device?.fireTv == true && flags.asyncQueueing != false

    /** Whether tunneled playback is allowed on this model. */
    val tunnelingAllowed: Boolean get() = flags.tunneling == true

    /** Whether the screen may switch modes for Dolby Vision below Android 14 here. */
    val dvModeSwitchAllowed: Boolean get() = flags.dvModeSwitch == true
}

/**
 * The capability probe: hardware decoders and their limits, Dolby Vision
 * profiles, the screen's HDR types and modes, passthrough, and the quirks
 * of known models, which can add capabilities as well as remove them.
 * Probed after the first frame, never on the main thread, and again when
 * the screen or audio output changes.
 */
interface DeviceCapabilities {
    /** The latest probe; `null` until the first one ends. */
    val snapshot: StateFlow<CapsSnapshot?>

    /** Probes now (off the main thread) unless a probe for this screen and output is saved. */
    suspend fun warmUp(): CapsSnapshot

    /**
     * The display the player shows on (its activity's); the default display
     * until this is called. Another display means another probe.
     */
    fun setDisplay(displayId: Int) {}

    /** The person's passthrough overrides and the debug switch for software decoders. */
    fun updateSettings(settings: CapsSettings) {}

    /** A quirk table from the server, laid over the app's own ([QuirkTable.overriddenBy]). */
    fun updateQuirks(table: QuirkTable) {}
}

/** Whether a video can play as it is here, and if not, what the server will do about it. */
sealed interface CapsDecision {
    /** It plays as it is. */
    data object Direct : CapsDecision

    /** Dolby Vision this device can't decode, on an HDR10 screen: the server sends the HDR10 base layer. */
    data object RemuxBaseLayer : CapsDecision

    /** The screen can't show its HDR: the server tone-maps it. */
    data object ServerToneMaps : CapsDecision

    /** Not as it is: [reason] says why, for Diagnostics and logs (not shown to people as it is). */
    data class Unsupported(val reason: String) : CapsDecision
}

/**
 * The client's side of the Dolby Vision and HDR rules: what this device does
 * with each version of an item, and which version plays best here.
 */
interface PlaybackCapsPolicy {
    fun decide(item: ItemDetail, fileId: Long?, caps: CapsSnapshot): CapsDecision

    /** The file of [item] that plays best here; `null` when it has none. */
    fun bestFile(item: ItemDetail, caps: CapsSnapshot): Long?
}

/** The capability side of the app. */
interface CapsGraph {
    val capabilities: DeviceCapabilities
    val policy: PlaybackCapsPolicy

    companion object {
        /** The probe of this device (nothing runs until [DeviceCapabilities.warmUp]) and the policy. */
        fun create(context: Context, profile: DeviceProfile): CapsGraph = object : CapsGraph {
            override val capabilities: DeviceCapabilities by lazy { ProbingDeviceCapabilities.create(context, profile) }
            override val policy: PlaybackCapsPolicy = DefaultPlaybackCapsPolicy
        }
    }
}
