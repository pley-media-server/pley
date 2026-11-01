package app.pley.android.caps

import app.pley.android.common.DeviceProfile
import app.pley.android.common.Tier
import app.pley.android.model.DynamicRange
import app.pley.android.model.caps.ClientCaps
import app.pley.android.model.caps.DeviceCaps
import app.pley.android.model.caps.DeviceForm
import app.pley.android.model.caps.DisplayCaps
import app.pley.android.model.caps.DolbyVisionCaps
import app.pley.android.model.caps.HlsCaps
import app.pley.android.model.caps.VideoDecoderCaps
import kotlin.math.roundToInt

/**
 * Turns what the probe read into the report the server decides from, with
 * the decoder policy, the quirks and the person's overrides applied. Pure:
 * the same input gives the same report, on a device or in a test.
 */
object CapsBuilder {
    /** Containers Media3 reads, by the server's names (file extensions). */
    val CONTAINERS: List<String> = listOf(
        "mkv", "mp4", "m4v", "mov", "webm", "ts", "avi", "flv", "mpg", "mpeg", "3gp",
        "mp3", "flac", "m4a", "m4b", "aac", "ogg", "oga", "opus", "wav", "mka",
    )

    /** Subtitles the player draws itself, by ffprobe's names. */
    val SUBTITLES: List<String> = listOf(
        "subrip", "ass", "ssa", "webvtt", "mov_text", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle",
    )

    /** Display.HdrCapabilities types to the server's dynamic ranges. */
    fun dynamicRange(hdrType: Int): DynamicRange? = when (hdrType) {
        HDR_TYPE_DOLBY_VISION -> DynamicRange.DolbyVision
        HDR_TYPE_HDR10 -> DynamicRange.Hdr10
        HDR_TYPE_HLG -> DynamicRange.Hlg
        HDR_TYPE_HDR10_PLUS -> DynamicRange.Hdr10Plus
        else -> null
    }

    // Display.HdrCapabilities.HDR_TYPE_*.
    const val HDR_TYPE_DOLBY_VISION = 1
    const val HDR_TYPE_HDR10 = 2
    const val HDR_TYPE_HLG = 3
    const val HDR_TYPE_HDR10_PLUS = 4

    /** The tier's name in the report: "low" (T0), "mid" (T1), "high" (T2). */
    fun tierName(tier: Tier): String = when (tier) {
        Tier.T0 -> "low"
        Tier.T1 -> "mid"
        Tier.T2 -> "high"
    }

    fun tierOf(device: DeviceFacts): Tier = DeviceProfile.tierOf(device.lowRam, device.totalRamBytes)

    fun build(
        input: ProbeInput,
        quirks: QuirkTable,
        settings: CapsSettings = CapsSettings(),
    ): CapsSnapshot {
        val device = input.device
        val policy = DecoderPolicy(tv = device.isTv, allowSoftware = settings.allowSoftwareDecoders)
        val applied = quirks.matching(device)
        val removedCodecs = applied.flatMap { it.remove.videoCodecs }.map { it.lowercase() }.toSet()
        val addedCodecs = applied.flatMap { it.add.videoCodecs }.map { it.lowercase() }.toSet()

        // Video decoders the policy allows, one or two entries per codec.
        val probed = Codecs.VIDEO.flatMap { (mime, codec) ->
            if (codec in removedCodecs) emptyList() else videoEntries(mime, codec, input.decoders, policy)
        }
        // A codec a quirk adds plays without known limits.
        val video = probed + (addedCodecs - removedCodecs - probed.map { it.codec }.toSet())
            .map { VideoDecoderCaps(codec = it, hardware = true) }
        val videoCodecs = video.map { it.codec }.distinct()

        // The screen's HDR types, corrected by the quirks.
        val screen = (screenHdrTypes(input.display, device.isTv, settings.modeSwitching) + applied.flatMap { it.add.hdrTypes }) -
            applied.flatMap { it.remove.hdrTypes }.toSet()

        // Dolby Vision profiles: hardware decoders, plus and minus the quirks.
        val dvProbed = DolbyVision.profiles(input.decoders)
        val dvRemoved = applied.flatMap { it.remove.dolbyVisionProfiles }.toSet()
        val dvProfiles = (dvProbed.keys + applied.flatMap { it.add.dolbyVisionProfiles }).filter { it !in dvRemoved }
            .toSortedSet()
            .map { DolbyVisionCaps(profile = it, maxLevel = dvProbed[it]) }

        // HDR types a decoder can feed to the screen.
        val hdrDecoders = input.decoders.filter {
            it.hardware && it.mime in Codecs.HDR_MIMES && Codecs.VIDEO[it.mime] !in removedCodecs
        }
        val tenBit = hdrDecoders.any { Codecs.decodesHdr(it.mime, it.profileLevels.map { pl -> pl.profile }) }
        val hdr10Plus = hdrDecoders.any { Codecs.decodesHdr10Plus(it.mime, it.profileLevels.map { pl -> pl.profile }) }
        val dvScreen = DynamicRange.DolbyVision in screen
        val hdrTypes = buildList {
            if (DynamicRange.Hdr10 in screen && tenBit) add(DynamicRange.Hdr10)
            if (DynamicRange.Hdr10Plus in screen && tenBit && hdr10Plus) add(DynamicRange.Hdr10Plus)
            if (DynamicRange.Hlg in screen && tenBit) add(DynamicRange.Hlg)
            if (dvScreen && dvProfiles.isNotEmpty()) add(DynamicRange.DolbyVision)
        }

        // Audio: decoded here, and sent as it is.
        val audioMimes = input.decoders.filter { !it.isVideo }.map { it.mime }.toSet()
        val audioDecode = Codecs.AUDIO.filterKeys { it in audioMimes }.values.flatten().distinct()
        val passthrough = passthroughEncodings(input.audio, applied, settings)
        val passthroughNames = PassthroughFormat.entries
            .filter { format -> format.encodings.any { it in passthrough } }
            .map { it.codec }
            .distinct()
        val channels = when {
            passthrough.isNotEmpty() -> input.audio.maxChannels
            input.audio.route in STEREO_ROUTES -> minOf(2, input.audio.maxChannels)
            else -> input.audio.maxChannels
        }

        val tier = tierOf(device)
        val report = ClientCaps(
            containers = CONTAINERS,
            videoCodecs = videoCodecs,
            audioCodecs = (audioDecode + passthroughNames).distinct(),
            hdr = DynamicRange.Hdr10 in hdrTypes || DynamicRange.Hlg in hdrTypes,
            dolbyVision = dvScreen && dvProfiles.any { it.profile == 5 },
            capsVersion = ClientCaps.VERSION_2,
            video = video,
            hdrTypes = hdrTypes,
            // Whatever the screen: without Dolby Vision in hdrTypes, the server
            // says the screen is what doesn't show it.
            dolbyVisionProfiles = dvProfiles,
            audioDecode = audioDecode,
            audioPassthrough = passthroughNames,
            audioChannels = channels,
            subtitleCodecs = SUBTITLES,
            audioTrackSwitching = true,
            pictureSubtitlesSideload = true,
            // Media3 moves between a transcode's sizes by itself (adaptive bitrate).
            hls = HlsCaps(masterPlaylist = true, segments = listOf("ts", "fmp4"), variants = true),
            display = displayCaps(input.display, device.isTv),
            device = DeviceCaps(
                form = when (device.form) {
                    DeviceFacts.Form.Phone -> DeviceForm.Phone
                    DeviceFacts.Form.Tablet -> DeviceForm.Tablet
                    DeviceFacts.Form.Tv -> DeviceForm.Tv
                },
                tier = tierName(tier),
                fireTv = device.fireTv,
                model = device.model,
            ),
        )
        val flags = applied.fold(QuirkFlags()) { acc, quirk ->
            QuirkFlags(
                dvHdr10PlusHybridsBroken = quirk.flags.dvHdr10PlusHybridsBroken ?: acc.dvHdr10PlusHybridsBroken,
                asyncQueueing = quirk.flags.asyncQueueing ?: acc.asyncQueueing,
                tunneling = quirk.flags.tunneling ?: acc.tunneling,
                dvModeSwitch = quirk.flags.dvModeSwitch ?: acc.dvModeSwitch,
            )
        }
        return CapsSnapshot(
            report = report,
            lines = emptyList(),
            details = CapsDetails(
                input = input,
                policy = policy,
                tier = tier.name,
                screenHdrTypes = screen.distinct(),
                dolbyVisionProfiles = dvProfiles,
                passthroughEncodings = passthrough.sorted(),
                quirks = applied.map { it.id },
                flags = flags,
            ),
        )
    }

    private val STEREO_ROUTES = setOf(AudioState.ROUTE_SPEAKER, AudioState.ROUTE_BLUETOOTH, AudioState.ROUTE_WIRED)

    /**
     * The entries of one codec: the hardware decoders' (merged), else on
     * phones the software decoders' within their limit. One entry for the
     * biggest size probed, and one for 1080p when it plays faster there.
     */
    private fun videoEntries(mime: String, codec: String, decoders: List<DecoderInfo>, policy: DecoderPolicy): List<VideoDecoderCaps> {
        val all = decoders.filter { it.mime == mime }
        val hardware = all.filter { it.hardware }
        val chosen = hardware.ifEmpty { if (policy.softwareAllowed) all else emptyList() }
        if (chosen.isEmpty()) return emptyList()
        val isHardware = hardware.isNotEmpty()
        val limited = !isHardware && policy.softwareLimited
        val profiles = chosen.flatMap { d -> d.profileLevels.map { it.profile } }
        val levels = chosen.flatMap { d -> d.profileLevels.map { it.level } }
        val bitDepth = if (limited) DecoderPolicy.SOFTWARE_MAX_BIT_DEPTH else Codecs.maxBitDepth(mime, profiles)
        val profileNames = Codecs.profileNames(mime, if (limited) profiles.map { it and Codecs.eightBitProfiles(mime) } else profiles)
        val level = Codecs.ffprobeLevel(mime, levels)

        fun entry(width: Int?, height: Int?, fps: Double?) = VideoDecoderCaps(
            codec = codec,
            hardware = isHardware,
            profiles = profileNames,
            maxWidth = width,
            maxHeight = height,
            maxFps = fps,
            maxBitDepth = bitDepth,
            maxLevel = level,
        )

        val rates = chosen.flatMap { it.sizeRates }
        val bySize = SizeRate.SIZES.reversed().mapNotNull { (w, h) ->
            rates.filter { it.width == w && it.height == h }.maxOfOrNull { it.fps }?.let { Triple(w, h, it) }
        }
        if (limited) {
            val at1080 = bySize.firstOrNull { it.second == DecoderPolicy.SOFTWARE_MAX_HEIGHT }
            return if (at1080 != null) {
                listOf(entry(DecoderPolicy.SOFTWARE_MAX_WIDTH, DecoderPolicy.SOFTWARE_MAX_HEIGHT, minOf(at1080.third, DecoderPolicy.SOFTWARE_MAX_FPS)))
            } else {
                val w = chosen.mapNotNull { it.maxWidth }.maxOrNull()?.coerceAtMost(DecoderPolicy.SOFTWARE_MAX_WIDTH)
                val h = chosen.mapNotNull { it.maxHeight }.maxOrNull()?.coerceAtMost(DecoderPolicy.SOFTWARE_MAX_HEIGHT)
                listOf(entry(w, h, DecoderPolicy.SOFTWARE_MAX_FPS))
            }
        }
        if (bySize.isEmpty()) {
            // Nothing probed (an old or odd decoder): its own limits.
            return listOf(entry(chosen.mapNotNull { it.maxWidth }.maxOrNull(), chosen.mapNotNull { it.maxHeight }.maxOrNull(), chosen.mapNotNull { it.maxFps }.maxOrNull()))
        }
        val biggest = bySize.first()
        return bySize.filterIndexed { i, (_, _, fps) -> i == 0 || fps > biggest.third }
            .map { (w, h, fps) -> entry(w, h, fps) }
    }

    /**
     * The HDR types of the screen in the modes a video plays in: per mode on
     * Android 14 and up (on a TV the mode in use and those the player may
     * switch to, on a phone the mode in use), less the ones the person
     * turned off; else what getHdrCapabilities says. Media3 decodes Dolby
     * Vision as its base layer in a mode that doesn't show it, so a mode the
     * player never uses mustn't promise it.
     */
    fun screenHdrTypes(display: DisplayState?, tv: Boolean, switching: ModeSwitching = ModeSwitching.Auto): List<DynamicRange> {
        display ?: return emptyList()
        val modes = if (tv) display.videoModes(switching) else listOfNotNull(display.activeMode)
        if (modes.isEmpty()) return ranges(display.hdrCapabilities)
        return ranges(modes.flatMap { hdrTypesIn(display, it) })
    }

    /** The HDR types [display] shows in [mode], as [screenHdrTypes] counts them. */
    fun hdrTypesShownIn(display: DisplayState, mode: DisplayModeInfo?): List<DynamicRange> = ranges(hdrTypesIn(display, mode))

    /** Modes that all list nothing (or the version before Android 14): getHdrCapabilities. */
    private fun hdrTypesIn(display: DisplayState, mode: DisplayModeInfo?): List<Int> =
        if (display.modes.all { it.hdrTypes.isNullOrEmpty() }) display.hdrCapabilities else display.hdrTypesIn(mode)

    private fun ranges(types: List<Int>): List<DynamicRange> = types.mapNotNull { dynamicRange(it) }.distinct().sortedBy { it.ordinal }

    /**
     * The encodings sent as they are: Media3's, corrected by the quirks, then
     * by the person's overrides. None to Bluetooth, whatever was chosen.
     */
    private fun passthroughEncodings(audio: AudioState, quirks: List<Quirk>, settings: CapsSettings): Set<Int> {
        if (audio.route == AudioState.ROUTE_BLUETOOTH) return emptySet()
        val out = audio.passthroughEncodings.toMutableSet()
        quirks.flatMap { it.remove.passthrough }.forEach { out -= it.encodings.toSet() }
        quirks.flatMap { it.add.passthrough }.forEach { out += it.encodings }
        PassthroughFormat.entries.forEach { format ->
            when (settings.passthrough(format)) {
                Override.On -> out += format.encodings
                Override.Off -> out -= format.encodings.toSet()
                Override.Auto -> Unit
            }
        }
        return out
    }

    /**
     * The screen's mode in use, and the rates it can switch to: on a TV the
     * modes of the same size (the player switches between them), on a phone
     * the mode's own rate and its seamless alternatives.
     */
    fun displayCaps(display: DisplayState?, tv: Boolean): DisplayCaps? {
        val active = display?.activeMode ?: return null
        val rates = if (tv) {
            display.modes.filter { it.width == active.width && it.height == active.height }.map { it.refreshRate }
        } else {
            listOf(active.refreshRate) + active.alternativeRefreshRates
        }
        return DisplayCaps(
            width = active.width,
            height = active.height,
            refreshRates = rates.map { roundRate(it) }.distinct().sorted(),
        )
    }

    /** 60.000004 Hz is 60, 23.976025 is 23.976. */
    fun roundRate(rate: Float): Float = (rate * 1000).roundToInt() / 1000f
}
