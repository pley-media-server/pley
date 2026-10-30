package app.pley.android.caps

import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemDetail
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.model.caps.VideoDecoderCaps
import java.util.Locale

/**
 * The client's side of what plays Dolby Vision and HDR as it is:
 *
 * - HDR10 plays as it is with an HDR10 screen and a 10-bit hardware decoder;
 *   HDR10+ with an HDR10+ or HDR10 screen; HLG needs HLG.
 * - Dolby Vision plays as it is when the screen shows it and a decoder lists
 *   the profile (or a quirk adds it). Otherwise profiles 8.1 and 8.4 play
 *   their HDR10 or HLG base layer, profile 7 needs the server's HDR10 base
 *   layer, and profile 5, which has no base layer to fall back to, is
 *   tone-mapped by the server.
 * - Anything the screen can't show is tone-mapped by the server; a codec,
 *   size, rate, depth or level no decoder takes is Unsupported.
 *
 * Pure: it reads the item and the snapshot, nothing else.
 */
object DefaultPlaybackCapsPolicy : PlaybackCapsPolicy {
    override fun decide(item: ItemDetail, fileId: Long?, caps: CapsSnapshot): CapsDecision {
        val file = fileId?.let { id -> item.files.firstOrNull { it.id == id } }
            ?: bestFile(item, caps)?.let { id -> item.files.firstOrNull { it.id == id } }
            ?: return CapsDecision.Unsupported("The item has no media file")
        return decide(file, caps)
    }

    override fun bestFile(item: ItemDetail, caps: CapsSnapshot): Long? {
        val screenHeight = caps.report.display?.let { minOf(it.width, it.height) }
        return item.files
            .map { it to decide(it, caps) }
            .sortedWith(
                compareBy<Pair<MediaFile, CapsDecision>> { (_, decision) -> rank(decision) }
                    .thenBy { (file, _) -> if (screenHeight == null || height(file) <= screenHeight) 0 else 1 }
                    .thenByDescending { (file, _) -> height(file) }
                    .thenByDescending { (file, decision) -> if (decision == CapsDecision.Direct) rangeRank(range(video(file))) else 0 }
                    .thenByDescending { (file, _) -> file.bitrate ?: 0 },
            )
            .firstOrNull()
            ?.first
            ?.id
    }

    /** What happens to [file] here. */
    fun decide(file: MediaFile, caps: CapsSnapshot): CapsDecision {
        val video = video(file) ?: return CapsDecision.Direct
        val types = caps.report.hdrTypes.orEmpty()
        return when (val range = range(video)) {
            DynamicRange.DolbyVision -> dolbyVision(video, caps)
            DynamicRange.Hdr10, DynamicRange.Hdr10Plus, DynamicRange.Hlg -> {
                val shown = when (range) {
                    DynamicRange.Hlg -> DynamicRange.Hlg in types
                    DynamicRange.Hdr10Plus -> DynamicRange.Hdr10Plus in types || DynamicRange.Hdr10 in types
                    else -> DynamicRange.Hdr10 in types
                }
                if (shown) fits(video, video.codec, caps, hdr = true) else CapsDecision.ServerToneMaps
            }
            else -> fits(video, video.codec, caps, hdr = false)
        }
    }

    private fun dolbyVision(video: MediaStream, caps: CapsSnapshot): CapsDecision {
        val profile = video.dvProfile
        val types = caps.report.hdrTypes.orEmpty()
        val decoded = caps.report.dolbyVisionProfiles.orEmpty().firstOrNull { it.profile == profile }
        if (DynamicRange.DolbyVision in types && decoded != null) {
            val level = video.dvLevel
            val max = decoded.maxLevel
            return if (level != null && max != null && level > max) {
                CapsDecision.Unsupported("Dolby Vision level $level is above this decoder's $max")
            } else {
                CapsDecision.Direct
            }
        }
        val base = baseCodec(profile, video.codec)
        return when (profile) {
            // No base layer to fall back to.
            5 -> CapsDecision.ServerToneMaps
            // Media3 has no profile 7 fallback: the server sends the HDR10 base layer.
            7 -> if (DynamicRange.Hdr10 in types) CapsDecision.RemuxBaseLayer else CapsDecision.ServerToneMaps
            // The Blu-ray's HDR10 id in profile 8 (a profile 7 converted elsewhere): HDR10.
            else -> when (video.dvBlCompat?.takeUnless { profile == 8 && it == BL_BLU_RAY } ?: if (profile == 8) BL_HDR10 else null) {
                BL_HDR10 -> if (DynamicRange.Hdr10 in types) fits(video, base, caps, hdr = true) else CapsDecision.ServerToneMaps
                BL_HLG -> if (DynamicRange.Hlg in types) fits(video, base, caps, hdr = true) else CapsDecision.ServerToneMaps
                BL_SDR -> fits(video, base, caps, hdr = false)
                else -> CapsDecision.ServerToneMaps
            }
        }
    }

    /** Whether a decoder entry of [codec] takes the stream's size, rate, depth, profile and level. */
    private fun fits(video: MediaStream, codec: String, caps: CapsSnapshot, hdr: Boolean): CapsDecision {
        val entries = caps.report.video.orEmpty().filter { it.codec.equals(codec, ignoreCase = true) }
        if (entries.isEmpty()) return CapsDecision.Unsupported("No decoder for $codec here")
        val depth = bitDepth(video)
        if (hdr && entries.none { it.hardware && (it.maxBitDepth ?: 8) >= 10 }) {
            return CapsDecision.Unsupported("No 10-bit hardware decoder for $codec here")
        }
        return if (entries.any { takes(it, video, depth) }) {
            CapsDecision.Direct
        } else {
            val size = listOfNotNull(video.width, video.height).joinToString("×")
            val fps = video.frameRate?.let { " at %.3f fps".format(Locale.ROOT, it) }.orEmpty()
            CapsDecision.Unsupported("$codec $size$fps, $depth-bit${video.profile?.let { ", $it" }.orEmpty()}, is beyond this device's decoders")
        }
    }

    private fun takes(entry: VideoDecoderCaps, video: MediaStream, depth: Int): Boolean {
        val w = video.width
        val h = video.height
        val maxW = entry.maxWidth
        val maxH = entry.maxHeight
        if (w != null && h != null && maxW != null && maxH != null) {
            val fitsAsIs = w <= maxW && h <= maxH
            val fitsTurned = h <= maxW && w <= maxH
            if (!fitsAsIs && !fitsTurned) return false
        }
        val fps = video.frameRate
        val maxFps = entry.maxFps
        if (fps != null && maxFps != null && fps > maxFps + RATE_SLACK) return false
        if (depth > (entry.maxBitDepth ?: 8)) return false
        val level = video.level
        val maxLevel = entry.maxLevel
        if (level != null && level > 0 && maxLevel != null && level > maxLevel) return false
        val profile = video.profile
        if (profile != null && entry.profiles.isNotEmpty() && entry.profiles.none { it.equals(profile, ignoreCase = true) }) {
            // ffprobe and Android name some profiles differently; only refuse those known to be missing.
            if (profile.lowercase() in KNOWN_PROFILES) return false
        }
        return true
    }

    private const val RATE_SLACK = 0.01

    // Dolby Vision base-layer compatibility ids (ffprobe's dv_bl_signal_compatibility_id).
    private const val BL_HDR10 = 1
    private const val BL_SDR = 2
    private const val BL_HLG = 4
    private const val BL_BLU_RAY = 6

    private val KNOWN_PROFILES = setOf(
        "main", "main 10", "main still picture", "rext", "baseline", "constrained baseline", "high", "high 10",
        "high 4:2:2", "high 4:4:4 predictive", "profile 0", "profile 1", "profile 2", "profile 3", "professional",
    )

    /** The codec under a Dolby Vision profile: AVC for 9, AV1 for 10, HEVC for the rest. */
    private fun baseCodec(profile: Int?, codec: String): String = when (profile) {
        9 -> "h264"
        10 -> "av1"
        else -> if (codec.equals("dvhe", true) || codec.equals("dvh1", true)) "hevc" else codec
    }

    private fun video(file: MediaFile): MediaStream? {
        val videos = file.streams.filter { it.kind == StreamKind.Video }
        return videos.firstOrNull { it.default } ?: videos.firstOrNull()
    }

    private fun range(video: MediaStream?): DynamicRange = when {
        video == null -> DynamicRange.Sdr
        video.dynamicRange != null && video.dynamicRange != DynamicRange.Unknown -> video.dynamicRange!!
        video.dvProfile != null -> DynamicRange.DolbyVision
        video.colorTransfer == "smpte2084" -> DynamicRange.Hdr10
        video.colorTransfer == "arib-std-b67" -> DynamicRange.Hlg
        else -> DynamicRange.Sdr
    }

    /** The stream's bit depth, else from its pixel format ("yuv420p10le", "p010le") or profile ("Main 10"). */
    private fun bitDepth(video: MediaStream): Int = video.bitDepth
        ?: video.pixFmt?.let { PIX_FMT_DEPTH.find(it)?.groupValues?.get(1)?.toInt() }
        ?: if (video.profile?.contains("10") == true) 10 else 8

    private val PIX_FMT_DEPTH = Regex("(1[02])(?:le|be)$")

    private fun height(file: MediaFile): Int = video(file)?.height ?: file.height ?: 0

    private fun rank(decision: CapsDecision): Int = when (decision) {
        CapsDecision.Direct -> 0
        CapsDecision.RemuxBaseLayer -> 1
        CapsDecision.ServerToneMaps -> 2
        is CapsDecision.Unsupported -> 3
    }

    private fun rangeRank(range: DynamicRange): Int = when (range) {
        DynamicRange.DolbyVision -> 4
        DynamicRange.Hdr10Plus -> 3
        DynamicRange.Hdr10 -> 2
        DynamicRange.Hlg -> 1
        else -> 0
    }
}
