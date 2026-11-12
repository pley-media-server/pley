package app.pley.android.player.engine

import android.net.Uri
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.DataReader
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.ExperimentalApi
import androidx.media3.common.util.ParsableByteArray
import androidx.media3.common.util.TimestampAdjuster
import androidx.media3.common.util.UnstableApi
import androidx.media3.common.util.Util
import androidx.media3.exoplayer.analytics.PlayerId
import androidx.media3.exoplayer.hls.DefaultHlsExtractorFactory
import androidx.media3.exoplayer.hls.HlsExtractorFactory
import androidx.media3.exoplayer.hls.HlsMediaChunkExtractor
import androidx.media3.extractor.ExtractorInput
import androidx.media3.extractor.ExtractorOutput
import androidx.media3.extractor.SeekMap
import androidx.media3.extractor.TrackOutput
import androidx.media3.extractor.text.SubtitleParser
import kotlin.math.max
import kotlin.math.min

/**
 * Media3's HLS extractors, with a Dolby Vision stream's metadata kept with
 * its frames. In HEVC each frame's Dolby Vision metadata (its RPU) is a NAL
 * unit of its own after the frame's slices, of a type HEVC leaves
 * unspecified. Media3's MPEG-TS reader ends a sample at such a unit and
 * starts the next one at the next frame, so the RPU reaches no decoder: the
 * Dolby Vision decoder gets the base layer alone and the screen shows
 * HDR10. A file played as it is hands its samples over whole. For a stream
 * its playlist names Dolby Vision over HEVC, each sample takes in the Dolby
 * Vision units that follow it ([DolbyVisionSamples]); any other stream's
 * extractor is Media3's as it is.
 */
@OptIn(markerClass = [UnstableApi::class, ExperimentalApi::class])
class DolbyVisionSegments(private val base: HlsExtractorFactory = DefaultHlsExtractorFactory()) : HlsExtractorFactory {
    override fun createExtractor(
        uri: Uri,
        format: Format,
        muxedCaptionFormats: List<Format>?,
        timestampAdjuster: TimestampAdjuster,
        responseHeaders: Map<String, List<String>>,
        sniffingExtractorInput: ExtractorInput,
        playerId: PlayerId,
    ): HlsMediaChunkExtractor {
        val extractor = base.createExtractor(uri, format, muxedCaptionFormats, timestampAdjuster, responseHeaders, sniffingExtractorInput, playerId)
        return if (namesDolbyVision(format)) WholeSamples(extractor) else extractor
    }

    override fun setSubtitleParserFactory(subtitleParserFactory: SubtitleParser.Factory): HlsExtractorFactory = apply {
        base.setSubtitleParserFactory(subtitleParserFactory)
    }

    override fun experimentalParseSubtitlesDuringExtraction(parseSubtitlesDuringExtraction: Boolean): HlsExtractorFactory = apply {
        base.experimentalParseSubtitlesDuringExtraction(parseSubtitlesDuringExtraction)
    }

    override fun experimentalSetCodecsToParseWithinGopSampleDependencies(codecsToParseWithinGopSampleDependencies: Int): HlsExtractorFactory = apply {
        base.experimentalSetCodecsToParseWithinGopSampleDependencies(codecsToParseWithinGopSampleDependencies)
    }

    override fun getOutputTextFormat(sourceFormat: Format): Format = base.getOutputTextFormat(sourceFormat)

    /** [extractor], its video tracks' samples whole ([DolbyVisionSamples]). */
    private class WholeSamples(private val extractor: HlsMediaChunkExtractor) : HlsMediaChunkExtractor {
        /** By track id: a reader asks for its output once, and keeps it over the segments its extractor reads. */
        private val tracks = HashMap<Int, DolbyVisionSamples>()

        override fun init(extractorOutput: ExtractorOutput) = extractor.init(
            object : ExtractorOutput {
                override fun track(id: Int, type: Int): TrackOutput {
                    val track = extractorOutput.track(id, type)
                    if (type != C.TRACK_TYPE_VIDEO) return track
                    return tracks[id]?.takeIf { it.output === track } ?: DolbyVisionSamples(track).also { tracks[id] = it }
                }

                override fun endTracks() = extractorOutput.endTracks()

                override fun seekMap(seekMap: SeekMap) = extractorOutput.seekMap(seekMap)
            },
        )

        override fun read(extractorInput: ExtractorInput): Boolean = extractor.read(extractorInput)

        override fun isPackedAudioExtractor(): Boolean = extractor.isPackedAudioExtractor

        override fun isReusable(): Boolean = extractor.isReusable

        override fun recreate(): HlsMediaChunkExtractor = WholeSamples(extractor.recreate())

        override fun onTruncatedSegmentParsed() {
            extractor.onTruncatedSegmentParsed()
            tracks.values.forEach { it.startOver() }
        }
    }

    companion object {
        /** Whether a playlist names [format]'s video Dolby Vision over HEVC (`dvh1`, `dvhe`). */
        fun namesDolbyVision(format: Format): Boolean =
            DecoderRules.baseLayerOf(Util.getCodecsOfType(format.codecs, C.TRACK_TYPE_VIDEO)) == MimeTypes.VIDEO_H265
    }
}

/**
 * An HEVC track's [output] whose samples take in the Dolby Vision NAL units
 * that follow them: the RPU, and before it a profile 7 stream's
 * enhancement layer ([DolbyVisionSegments]). Media3's reader writes the
 * stream's bytes as they come, then says where in them each sample is. A
 * sample it ends where a Dolby Vision unit starts is passed on once the
 * unit after those has started, ending there; every other sample, and a
 * stream without such units, passes as it comes.
 *
 * The reader counts the zero before a start code as the unit's own when
 * the bytes it was handed last show one, whichever stream's that byte is:
 * a sample's end is taken for a unit's start when it is that or the byte
 * before, and a sample that starts in the one before it starts after it.
 */
@OptIn(markerClass = [UnstableApi::class])
internal class DolbyVisionSamples(val output: TrackOutput) : TrackOutput {
    /** The sample data's bytes so far. */
    private var written = 0L

    /** How many zero bytes the data so far ends with, three at the most: a start code is two and a 1. */
    private var zeros = 0

    /** Where the start code began whose NAL unit's header is the next byte, -1 otherwise; and whether a zero came before it. */
    private var headerOf = -1L
    private var headerAfterZero = false

    /**
     * The NAL units started last: where each one's start code began (its
     * two zeros and 1), where the unit before it ended (a byte earlier
     * when a zero came before the start code), and its type.
     */
    private val codes = LongArray(RECENT)
    private val ends = LongArray(RECENT)
    private val types = IntArray(RECENT)
    private var units = 0L

    /** A sample that ends at a Dolby Vision unit, until the unit after those starts. */
    private var held: Sample? = null

    /** Where the sample passed on last ended. */
    private var passedTo = 0L
    private var said = false

    private class Sample(val timeUs: Long, val flags: Int, val start: Long, val end: Long, val cryptoData: TrackOutput.CryptoData?)

    override fun format(format: Format) = output.format(format)

    override fun durationUs(durationUs: Long) = output.durationUs(durationUs)

    override fun sampleData(input: DataReader, length: Int, allowEndOfInput: Boolean, sampleDataPart: Int): Int {
        val read = output.sampleData(input, length, allowEndOfInput, sampleDataPart)
        if (read > 0) {
            written += read
            unseen()
        }
        return read
    }

    override fun sampleData(data: ParsableByteArray, length: Int, sampleDataPart: Int) {
        val bytes = data.data
        val from = data.position
        output.sampleData(data, length, sampleDataPart)
        written += length
        if (sampleDataPart == TrackOutput.SAMPLE_DATA_PART_MAIN) scan(bytes, from, length) else unseen()
    }

    override fun sampleMetadata(timeUs: Long, flags: Int, size: Int, offset: Int, cryptoData: TrackOutput.CryptoData?) {
        passHeld()
        val end = written - offset
        val sample = Sample(timeUs, flags, if (end > passedTo) max(end - size, passedTo) else end - size, end, cryptoData)
        val after = unitAt(end)
        if (after < 0 || !isDolbyVision(types[slot(after)])) {
            pass(sample, end)
            return
        }
        val next = (after + 1 until units).firstOrNull { !isDolbyVision(types[slot(it)]) }
        if (next != null) passWhole(sample, ends[slot(next)]) else held = sample
    }

    /** The segment's bytes start over: nothing read so far tells of what comes. */
    fun startOver() {
        passHeld()
        unseen()
        units = 0
    }

    /** Bytes went by unread: no start code is known to run into what follows. */
    private fun unseen() {
        zeros = 0
        headerOf = -1
    }

    /** Notes the NAL units that start in [bytes], which end the data so far. */
    private fun scan(bytes: ByteArray, from: Int, length: Int) {
        val end = from + length
        val base = written - end
        var i = from
        while (i < end) {
            // No start code ends within three bytes whose last is neither 0 nor 1.
            if (zeros == 0 && headerOf < 0 && i + 2 < end && (bytes[i + 2].toInt() and 0xFE) != 0) {
                i += 3
                continue
            }
            val byte = bytes[i].toInt() and 0xFF
            if (headerOf >= 0) {
                started(headerOf, headerAfterZero, (byte and 0x7E) shr 1)
                headerOf = -1
            }
            if (byte == 0) {
                zeros = min(zeros + 1, 3)
            } else {
                if (byte == 1 && zeros >= 2) {
                    headerOf = base + i - 2
                    headerAfterZero = zeros == 3
                }
                zeros = 0
            }
            i++
        }
    }

    private fun started(code: Long, afterZero: Boolean, type: Int) {
        val slot = slot(units)
        codes[slot] = code
        ends[slot] = if (afterZero) code - 1 else code
        types[slot] = type
        units++
        val sample = held ?: return
        if (isDolbyVision(type)) return
        held = null
        passWhole(sample, ends[slot])
    }

    private fun slot(unit: Long): Int = (unit % RECENT).toInt()

    /** Which of the units started last began at [position], or a byte after it; -1 when none did. */
    private fun unitAt(position: Long): Long {
        var unit = units - 1
        while (unit >= 0 && unit >= units - RECENT) {
            val code = codes[slot(unit)]
            if (code == position || code == position + 1) return unit
            unit--
        }
        return -1
    }

    /** Passes [sample] on, ending at [end]: past its Dolby Vision units. */
    private fun passWhole(sample: Sample, end: Long) {
        if (!said) {
            said = true
            Log.i(TAG, "Dolby Vision metadata stays with its frames")
        }
        pass(sample, end)
    }

    /** Passes on the sample held, as it came: nothing more is known of what follows it. */
    private fun passHeld() {
        val sample = held ?: return
        held = null
        pass(sample, sample.end)
    }

    private fun pass(sample: Sample, end: Long) {
        passedTo = end
        output.sampleMetadata(sample.timeUs, sample.flags, (end - sample.start).toInt(), (written - end).toInt(), sample.cryptoData)
    }

    private companion object {
        const val TAG = "pley.engine"

        /** How many NAL unit starts are remembered: a sample's end is among the last few. */
        const val RECENT = 16

        /** HEVC's unspecified NAL unit types, which Dolby Vision uses: its RPU, and the enhancement layer's units. */
        const val NAL_RPU = 62
        const val NAL_ENHANCEMENT_LAYER = 63

        fun isDolbyVision(type: Int): Boolean = type == NAL_RPU || type == NAL_ENHANCEMENT_LAYER
    }
}
