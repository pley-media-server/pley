package app.pley.android.player.engine

import android.content.Context
import android.media.MediaCodecInfo.CodecProfileLevel
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.CodecSpecificDataUtil
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.mediacodec.MediaCodecInfo
import androidx.media3.exoplayer.mediacodec.MediaCodecSelector
import androidx.media3.exoplayer.mediacodec.MediaCodecUtil
import app.pley.android.caps.DecoderPolicy

/**
 * The caps' decoder policy applied to Media3's decoder lists: TVs keep
 * hardware video decoders only; phones put hardware first and keep software
 * ones for pictures up to 1080p30, 8-bit and SDR; the debug switch allows
 * software at any size. Dolby Vision decoders are left out when the video
 * doesn't come out as Dolby Vision, so Media3 falls back to the HEVC (or
 * AVC, AV1) decoder of the base layer, and come first when it does
 * ([dolbyVisionDecoders]). A repackaged stream's samples are named Dolby
 * Vision as their track is ([named]). Profiles 5 and 7 have no base layer
 * another decoder plays: their Dolby Vision decoders stay, as Media3 keeps
 * them, but for profile 7 on a drive ([profile7BaseLayer]), which goes to
 * the HEVC decoder. Audio decoders are never limited.
 */
@OptIn(markerClass = [UnstableApi::class])
class DecoderRules(
    private val policy: () -> DecoderPolicy,
    /**
     * Whether the video comes out as Dolby Vision: the display shows it in
     * the mode the video plays in. `null` when unknown (Media3 then decides).
     */
    private val dolbyVisionShown: () -> Boolean? = { null },
    /**
     * Dolby Vision profile 7 plays on the HEVC decoder, as its HDR10 base
     * layer, as on the Samsung app: a file on a drive, which no server can
     * make 8.1 of (a server's stream of it is 8.1 or HDR10 already, or
     * profile 7 for a player that decodes it).
     */
    private val profile7BaseLayer: () -> Boolean = { false },
) {
    /** A selector for a track of [format]: [base]'s decoders that may play it, hardware first. */
    fun selectorFor(base: MediaCodecSelector, format: Format?): MediaCodecSelector =
        MediaCodecSelector { mimeType, secure, tunneling ->
            if (mimeType == MimeTypes.VIDEO_DOLBY_VISION && baseLayerOnly(format)) {
                // The enhancement layer and Dolby Vision's metadata go unread: the HEVC decoder plays what is under them.
                return@MediaCodecSelector selectorFor(base, format).getDecoderInfos(MimeTypes.VIDEO_H265, secure, tunneling)
            }
            val listed = base.getDecoderInfos(mimeType, secure, tunneling)
            if (!MimeTypes.isVideo(mimeType)) return@MediaCodecSelector listed
            if (mimeType == MimeTypes.VIDEO_DOLBY_VISION && !dolbyVisionDecoded(format)) return@MediaCodecSelector emptyList()
            val picture = format?.let(Picture::of) ?: Picture.UNKNOWN
            val allowed = listed.filter { allows(mimeType, it.hardwareAccelerated, picture) }
            hardwareFirst(allowed) { it.hardwareAccelerated }
        }

    /** The selector for renderers that don't know the format (audio, and the factory's default). */
    fun selector(base: MediaCodecSelector = MediaCodecSelector.DEFAULT): MediaCodecSelector = selectorFor(base, null)

    /**
     * [format] as its decoder takes it, for weighing whether one plays it:
     * profile 7 whose base layer plays ([profile7BaseLayer]) as the HEVC
     * under it, as Media3 counts profile 7 as no HEVC decoder's and would
     * leave its track unplayed; any other format as it is.
     */
    fun decoded(format: Format): Format =
        if (format.sampleMimeType == MimeTypes.VIDEO_DOLBY_VISION && baseLayerOnly(format)) {
            format.buildUpon().setSampleMimeType(MimeTypes.VIDEO_H265).setCodecs(null).build()
        } else {
            format
        }

    /**
     * [sample] named as its track is. A stream the server repackages keeps
     * its Dolby Vision in MPEG-TS segments, which can't say so: its
     * playlist does. Media3 names the track ([tracks], the renderer's) by
     * the playlist, Dolby Vision, and each sample by its segment, HEVC, and
     * picks the decoder by the sample: the HEVC decoder, which plays the
     * base layer (profile 5, which has none, in wrong colours). Named as
     * its track, the sample goes to the Dolby Vision decoder, as a file
     * played as it is does. Media3 gives a sample its track's id (none, for
     * the server's stream of one size): a sample goes by the one track with
     * its id, and stays as it is when that isn't Dolby Vision over the
     * sample's codec.
     */
    fun named(sample: Format, tracks: Array<Format>): Format {
        if (sample.sampleMimeType == MimeTypes.VIDEO_DOLBY_VISION) return sample
        val track = tracks.singleOrNull { it.id == sample.id } ?: return sample
        if (track.sampleMimeType != MimeTypes.VIDEO_DOLBY_VISION || baseLayerOf(track.codecs) != sample.sampleMimeType) return sample
        return sample.buildUpon().setSampleMimeType(MimeTypes.VIDEO_DOLBY_VISION).setCodecs(track.codecs).build()
    }

    /**
     * The decoders of [format], best first, when it is Dolby Vision that
     * comes out as Dolby Vision: its own decoders, then its base layer's.
     * Media3 lists them so only when the display's mode in use shows Dolby
     * Vision, and a TV's decoder can start before the switch to the video's
     * mode: its base layer would then play as HDR10. `null` otherwise:
     * Media3's own list.
     */
    fun dolbyVisionDecoders(
        context: Context,
        base: MediaCodecSelector,
        format: Format,
        secure: Boolean,
        tunneling: Boolean,
    ): List<MediaCodecInfo>? {
        if (format.sampleMimeType != MimeTypes.VIDEO_DOLBY_VISION || dolbyVisionShown() != true) return null
        val listed = MediaCodecUtil.getDecoderInfosSoftMatch(selectorFor(base, format), format, secure, tunneling)
        return MediaCodecUtil.getDecoderInfosSortedByFormatSupport(context, listed, format)
    }

    /**
     * Whether Dolby Vision decoders may play [format]: not when it doesn't
     * come out as Dolby Vision and its base layer has a decoder of its own
     * (Media3's alternative: HEVC for profiles 4 and 8, AVC for 9, AV1 for
     * 10). Without one (5, 7) nothing else plays it.
     */
    private fun dolbyVisionDecoded(format: Format?): Boolean =
        dolbyVisionShown() != false || format == null || MediaCodecUtil.getAlternativeCodecMimeType(format) == null

    /** Dolby Vision profile 7 whose base layer is to play ([profile7BaseLayer]). */
    private fun baseLayerOnly(format: Format?): Boolean =
        format != null && profile7BaseLayer() &&
            CodecSpecificDataUtil.getCodecProfileAndLevel(format)?.first == CodecProfileLevel.DolbyVisionProfileDvheDtb

    /** Whether a [hardware] (or software) decoder of [mimeType] may play [picture] here. */
    fun allows(mimeType: String, hardware: Boolean, picture: Picture): Boolean {
        if (!MimeTypes.isVideo(mimeType)) return true
        return policy().allows(hardware, picture.width, picture.height, picture.fps, picture.bitDepth, picture.hdr)
    }

    /**
     * A video picture as the policy weighs it. Unknown values count as
     * small: a format without its size is judged again with the size when
     * its decoder starts.
     */
    data class Picture(val width: Int, val height: Int, val fps: Double, val bitDepth: Int, val hdr: Boolean) {
        companion object {
            val UNKNOWN = Picture(0, 0, 0.0, 8, false)

            fun of(format: Format): Picture {
                val color = format.colorInfo
                val transfer = color?.colorTransfer ?: Format.NO_VALUE
                val hdr = format.sampleMimeType == MimeTypes.VIDEO_DOLBY_VISION ||
                    transfer == C.COLOR_TRANSFER_ST2084 || transfer == C.COLOR_TRANSFER_HLG
                val depth = color?.lumaBitdepth?.takeIf { it != Format.NO_VALUE } ?: bitDepthOf(format.codecs)
                return Picture(
                    width = format.width.coerceAtLeast(0),
                    height = format.height.coerceAtLeast(0),
                    fps = format.frameRate.toDouble().coerceAtLeast(0.0),
                    bitDepth = depth,
                    hdr = hdr,
                )
            }

            /**
             * The bit depth an RFC 6381 codec string tells: HEVC Main 10
             * (`hvc1.2…`), H.264 High 10 (`avc1.6E…`), VP9 profile 2 or 3
             * (`vp09.02…`), AV1 with 10 bits (`av01.0.08M.10`), Dolby Vision
             * (always 10). 8 otherwise.
             */
            fun bitDepthOf(codecs: String?): Int {
                val c = codecs?.substringBefore(',')?.trim()?.lowercase() ?: return 8
                val parts = c.split('.')
                return when (parts.firstOrNull()) {
                    "hvc1", "hev1" -> if (parts.getOrNull(1)?.trimStart('a', 'b', 'c') == "2") 10 else 8
                    "avc1", "avc3" -> if (parts.getOrNull(1)?.take(2) == "6e") 10 else 8
                    "vp09" -> if (parts.getOrNull(1) == "02" || parts.getOrNull(1) == "03") 10 else parts.getOrNull(3)?.toIntOrNull() ?: 8
                    "av01" -> parts.getOrNull(3)?.toIntOrNull() ?: 8
                    "dvh1", "dvhe", "dav1", "dva1", "dvav" -> 10
                    else -> 8
                }
            }
        }
    }

    companion object {
        /**
         * The codec under a Dolby Vision codec string (RFC 6381): HEVC
         * under `dvhe` and `dvh1`, H.264 under `dvav` and `dva1`, AV1 under
         * `dav1`. `null` for anything else. Media3's own
         * (`getDolbyVisionBaseLayerMimeType`) has none for profile 7, which
         * the server names for a player that decodes it.
         */
        fun baseLayerOf(codecs: String?): String? = when (codecs?.substringBefore(',')?.trim()?.lowercase()?.substringBefore('.')) {
            "dvhe", "dvh1" -> MimeTypes.VIDEO_H265
            "dvav", "dva1" -> MimeTypes.VIDEO_H264
            "dav1" -> MimeTypes.VIDEO_AV1
            else -> null
        }

        /** [list] with its hardware entries first, each kind in Media3's order. */
        fun <T> hardwareFirst(list: List<T>, hardware: (T) -> Boolean): List<T> {
            val (hw, sw) = list.partition(hardware)
            return hw + sw
        }
    }
}
