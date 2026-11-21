// Android's profile and level constants are plain numbers compared with what
// decoders list, so using newer ones on older versions is safe.
@file:SuppressLint("InlinedApi")

package app.pley.android.caps

import android.annotation.SuppressLint
import android.media.MediaCodecInfo.CodecProfileLevel

/**
 * How Android names codecs and their profiles, and how the server
 * (ffprobe) does: MIME types to codec names, CodecProfileLevel constants
 * to profile names, levels and bit depths.
 */
object Codecs {
    const val AVC = "video/avc"
    const val HEVC = "video/hevc"
    const val VP9 = "video/x-vnd.on2.vp9"
    const val VP8 = "video/x-vnd.on2.vp8"
    const val AV1 = "video/av01"
    const val MPEG2 = "video/mpeg2"
    const val DOLBY_VISION = "video/dolby-vision"

    /** The video types the probe asks about, with the server's codec names. Dolby Vision is reported apart. */
    val VIDEO: Map<String, String> = linkedMapOf(
        AVC to "h264",
        HEVC to "hevc",
        VP9 to "vp9",
        AV1 to "av1",
        MPEG2 to "mpeg2video",
        VP8 to "vp8",
    )

    /** The audio types the probe asks about, with the server's codec names. */
    val AUDIO: Map<String, List<String>> = linkedMapOf(
        "audio/mp4a-latm" to listOf("aac"),
        "audio/mpeg" to listOf("mp3"),
        "audio/opus" to listOf("opus"),
        "audio/vorbis" to listOf("vorbis"),
        "audio/flac" to listOf("flac"),
        "audio/alac" to listOf("alac"),
        "audio/ac3" to listOf("ac3"),
        "audio/eac3" to listOf("eac3"),
        "audio/eac3-joc" to listOf("eac3"),
        "audio/ac4" to listOf("ac4"),
        "audio/true-hd" to listOf("truehd"),
        "audio/vnd.dts" to listOf("dts"),
        "audio/vnd.dts.hd" to listOf("dts"),
        "audio/raw" to listOf("pcm_s16le", "pcm_s24le"),
    )

    /** The server's codec name for encoded audio of [mime] ("audio/eac3-joc" is "eac3"); `null` for PCM and types not listed. */
    fun audioCodecOf(mime: String?): String? = mime?.takeIf { it != "audio/raw" }?.let { AUDIO[it]?.firstOrNull() }

    /** Every type the probe lists decoders for. */
    val PROBED_MIMES: Set<String> = VIDEO.keys + DOLBY_VISION + AUDIO.keys

    /**
     * Media3's rule for telling software decoders by name, used below
     * Android 10 where isHardwareAccelerated doesn't exist
     * (MediaCodecUtil.isSoftwareOnlyV21).
     */
    fun isSoftwareByName(name: String): Boolean {
        val n = name.lowercase()
        if (n.startsWith("arc.")) return false
        return n.startsWith("omx.google.") ||
            n.startsWith("omx.ffmpeg.") ||
            (n.startsWith("omx.sec.") && n.contains(".sw.")) ||
            n == "omx.qcom.video.decoder.hevcswvdec" ||
            n.startsWith("c2.android.") ||
            n.startsWith("c2.google.") ||
            (!n.startsWith("omx.") && !n.startsWith("c2."))
    }

    /** The profile names ffprobe gives streams of [mime] that the decoder's [profiles] cover. */
    fun profileNames(mime: String, profiles: Collection<Int>): List<String> {
        val names = when (mime) {
            AVC -> AVC_PROFILES
            HEVC -> HEVC_PROFILES
            VP9 -> VP9_PROFILES
            AV1 -> AV1_PROFILES
            MPEG2 -> MPEG2_PROFILES
            else -> return emptyList()
        }
        // MPEG-2's profiles are numbers, not bits (Simple is 0).
        val matches: (Int, Int) -> Boolean = if (mime == MPEG2) { listed, wanted -> listed == wanted } else { listed, bits -> listed and bits != 0 }
        return names.filter { (bits, _) -> profiles.any { matches(it, bits) } }.map { it.second }.distinct()
    }

    /** 10 when a profile decodes 10-bit pictures, else 8. */
    fun maxBitDepth(mime: String, profiles: Collection<Int>): Int {
        val tenBit = when (mime) {
            AVC -> CodecProfileLevel.AVCProfileHigh10
            HEVC -> CodecProfileLevel.HEVCProfileMain10 or HEVC_HDR
            VP9 -> CodecProfileLevel.VP9Profile2 or CodecProfileLevel.VP9Profile3 or VP9_HDR
            AV1 -> CodecProfileLevel.AV1ProfileMain10 or AV1_HDR
            else -> 0
        }
        return if (profiles.any { it and tenBit != 0 }) 10 else 8
    }

    /** The profile bits of [mime] that are 8-bit: what a software decoder may play on a phone. */
    fun eightBitProfiles(mime: String): Int = when (mime) {
        AVC -> CodecProfileLevel.AVCProfileBaseline or CodecProfileLevel.AVCProfileConstrainedBaseline or
            CodecProfileLevel.AVCProfileMain or CodecProfileLevel.AVCProfileExtended or
            CodecProfileLevel.AVCProfileHigh or CodecProfileLevel.AVCProfileConstrainedHigh
        HEVC -> CodecProfileLevel.HEVCProfileMain
        VP9 -> CodecProfileLevel.VP9Profile0
        AV1 -> CodecProfileLevel.AV1ProfileMain8
        else -> -1
    }

    /** Whether a profile carries HDR10 or HLG (10-bit) pictures. */
    fun decodesHdr(mime: String, profiles: Collection<Int>): Boolean =
        mime in HDR_MIMES && maxBitDepth(mime, profiles) >= 10

    /** Whether a profile says the decoder passes HDR10+ dynamic metadata on. */
    fun decodesHdr10Plus(mime: String, profiles: Collection<Int>): Boolean {
        val bits = when (mime) {
            HEVC -> CodecProfileLevel.HEVCProfileMain10HDR10Plus
            VP9 -> CodecProfileLevel.VP9Profile2HDR10Plus or CodecProfileLevel.VP9Profile3HDR10Plus
            AV1 -> CodecProfileLevel.AV1ProfileMain10HDR10Plus
            else -> 0
        }
        return profiles.any { it and bits != 0 }
    }

    /** The highest of [levels] in ffprobe's numbering (HEVC 5.1 is 153, H.264 4.1 is 41, AV1 5.1 is 13); `null` when unknown. */
    fun ffprobeLevel(mime: String, levels: Collection<Int>): Int? {
        val table = when (mime) {
            AVC -> AVC_LEVELS
            HEVC -> HEVC_LEVELS
            VP9 -> VP9_LEVELS
            AV1 -> AV1_LEVELS
            else -> return null
        }
        return levels.mapNotNull { table[it] }.maxOrNull()
    }

    /** The video types that carry HDR10 and HLG. */
    val HDR_MIMES: Set<String> = setOf(HEVC, VP9, AV1)

    private const val HEVC_HDR = CodecProfileLevel.HEVCProfileMain10HDR10 or CodecProfileLevel.HEVCProfileMain10HDR10Plus
    private const val VP9_HDR = CodecProfileLevel.VP9Profile2HDR or CodecProfileLevel.VP9Profile3HDR or
        CodecProfileLevel.VP9Profile2HDR10Plus or CodecProfileLevel.VP9Profile3HDR10Plus
    private const val AV1_HDR = CodecProfileLevel.AV1ProfileMain10HDR10 or CodecProfileLevel.AV1ProfileMain10HDR10Plus

    private val AVC_PROFILES = listOf(
        (CodecProfileLevel.AVCProfileBaseline or CodecProfileLevel.AVCProfileConstrainedBaseline) to "Constrained Baseline",
        CodecProfileLevel.AVCProfileBaseline to "Baseline",
        CodecProfileLevel.AVCProfileMain to "Main",
        CodecProfileLevel.AVCProfileExtended to "Extended",
        (CodecProfileLevel.AVCProfileHigh or CodecProfileLevel.AVCProfileConstrainedHigh) to "High",
        CodecProfileLevel.AVCProfileHigh10 to "High 10",
        CodecProfileLevel.AVCProfileHigh422 to "High 4:2:2",
        CodecProfileLevel.AVCProfileHigh444 to "High 4:4:4 Predictive",
    )
    private val HEVC_PROFILES = listOf(
        CodecProfileLevel.HEVCProfileMain to "Main",
        (CodecProfileLevel.HEVCProfileMain10 or HEVC_HDR) to "Main 10",
        CodecProfileLevel.HEVCProfileMainStill to "Main Still Picture",
    )
    private val VP9_PROFILES = listOf(
        CodecProfileLevel.VP9Profile0 to "Profile 0",
        CodecProfileLevel.VP9Profile1 to "Profile 1",
        (CodecProfileLevel.VP9Profile2 or CodecProfileLevel.VP9Profile2HDR or CodecProfileLevel.VP9Profile2HDR10Plus) to "Profile 2",
        (CodecProfileLevel.VP9Profile3 or CodecProfileLevel.VP9Profile3HDR or CodecProfileLevel.VP9Profile3HDR10Plus) to "Profile 3",
    )

    // ffprobe calls AV1's Main profile "Main" for 8 and 10 bits alike.
    private val AV1_PROFILES = listOf(
        (CodecProfileLevel.AV1ProfileMain8 or CodecProfileLevel.AV1ProfileMain10 or AV1_HDR) to "Main",
    )
    private val MPEG2_PROFILES = listOf(
        CodecProfileLevel.MPEG2ProfileSimple to "Simple",
        CodecProfileLevel.MPEG2ProfileMain to "Main",
        CodecProfileLevel.MPEG2ProfileHigh to "High",
    )

    // H.264: ffprobe's level is ten times the level (4.1 is 41; 1b is 9).
    private val AVC_LEVELS = mapOf(
        CodecProfileLevel.AVCLevel1 to 10, CodecProfileLevel.AVCLevel1b to 9, CodecProfileLevel.AVCLevel11 to 11,
        CodecProfileLevel.AVCLevel12 to 12, CodecProfileLevel.AVCLevel13 to 13, CodecProfileLevel.AVCLevel2 to 20,
        CodecProfileLevel.AVCLevel21 to 21, CodecProfileLevel.AVCLevel22 to 22, CodecProfileLevel.AVCLevel3 to 30,
        CodecProfileLevel.AVCLevel31 to 31, CodecProfileLevel.AVCLevel32 to 32, CodecProfileLevel.AVCLevel4 to 40,
        CodecProfileLevel.AVCLevel41 to 41, CodecProfileLevel.AVCLevel42 to 42, CodecProfileLevel.AVCLevel5 to 50,
        CodecProfileLevel.AVCLevel51 to 51, CodecProfileLevel.AVCLevel52 to 52, CodecProfileLevel.AVCLevel6 to 60,
        CodecProfileLevel.AVCLevel61 to 61, CodecProfileLevel.AVCLevel62 to 62,
    )

    // HEVC: ffprobe gives general_level_idc, thirty times the level (5.1 is 153), for either tier.
    private val HEVC_LEVELS = mapOf(
        CodecProfileLevel.HEVCMainTierLevel1 to 30, CodecProfileLevel.HEVCHighTierLevel1 to 30,
        CodecProfileLevel.HEVCMainTierLevel2 to 60, CodecProfileLevel.HEVCHighTierLevel2 to 60,
        CodecProfileLevel.HEVCMainTierLevel21 to 63, CodecProfileLevel.HEVCHighTierLevel21 to 63,
        CodecProfileLevel.HEVCMainTierLevel3 to 90, CodecProfileLevel.HEVCHighTierLevel3 to 90,
        CodecProfileLevel.HEVCMainTierLevel31 to 93, CodecProfileLevel.HEVCHighTierLevel31 to 93,
        CodecProfileLevel.HEVCMainTierLevel4 to 120, CodecProfileLevel.HEVCHighTierLevel4 to 120,
        CodecProfileLevel.HEVCMainTierLevel41 to 123, CodecProfileLevel.HEVCHighTierLevel41 to 123,
        CodecProfileLevel.HEVCMainTierLevel5 to 150, CodecProfileLevel.HEVCHighTierLevel5 to 150,
        CodecProfileLevel.HEVCMainTierLevel51 to 153, CodecProfileLevel.HEVCHighTierLevel51 to 153,
        CodecProfileLevel.HEVCMainTierLevel52 to 156, CodecProfileLevel.HEVCHighTierLevel52 to 156,
        CodecProfileLevel.HEVCMainTierLevel6 to 180, CodecProfileLevel.HEVCHighTierLevel6 to 180,
        CodecProfileLevel.HEVCMainTierLevel61 to 183, CodecProfileLevel.HEVCHighTierLevel61 to 183,
        CodecProfileLevel.HEVCMainTierLevel62 to 186, CodecProfileLevel.HEVCHighTierLevel62 to 186,
    )

    // VP9: ten times the level (5.1 is 51); ffprobe often reports none.
    private val VP9_LEVELS = mapOf(
        CodecProfileLevel.VP9Level1 to 10, CodecProfileLevel.VP9Level11 to 11, CodecProfileLevel.VP9Level2 to 20,
        CodecProfileLevel.VP9Level21 to 21, CodecProfileLevel.VP9Level3 to 30, CodecProfileLevel.VP9Level31 to 31,
        CodecProfileLevel.VP9Level4 to 40, CodecProfileLevel.VP9Level41 to 41, CodecProfileLevel.VP9Level5 to 50,
        CodecProfileLevel.VP9Level51 to 51, CodecProfileLevel.VP9Level52 to 52, CodecProfileLevel.VP9Level6 to 60,
        CodecProfileLevel.VP9Level61 to 61, CodecProfileLevel.VP9Level62 to 62,
    )

    // AV1: seq_level_idx, (major - 2) * 4 + minor (5.1 is 13).
    private val AV1_LEVELS = mapOf(
        CodecProfileLevel.AV1Level2 to 0, CodecProfileLevel.AV1Level21 to 1, CodecProfileLevel.AV1Level22 to 2,
        CodecProfileLevel.AV1Level23 to 3, CodecProfileLevel.AV1Level3 to 4, CodecProfileLevel.AV1Level31 to 5,
        CodecProfileLevel.AV1Level32 to 6, CodecProfileLevel.AV1Level33 to 7, CodecProfileLevel.AV1Level4 to 8,
        CodecProfileLevel.AV1Level41 to 9, CodecProfileLevel.AV1Level42 to 10, CodecProfileLevel.AV1Level43 to 11,
        CodecProfileLevel.AV1Level5 to 12, CodecProfileLevel.AV1Level51 to 13, CodecProfileLevel.AV1Level52 to 14,
        CodecProfileLevel.AV1Level53 to 15, CodecProfileLevel.AV1Level6 to 16, CodecProfileLevel.AV1Level61 to 17,
        CodecProfileLevel.AV1Level62 to 18, CodecProfileLevel.AV1Level63 to 19, CodecProfileLevel.AV1Level7 to 20,
        CodecProfileLevel.AV1Level71 to 21, CodecProfileLevel.AV1Level72 to 22, CodecProfileLevel.AV1Level73 to 23,
    )
}
