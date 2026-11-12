package app.pley.android.player.engine

import android.net.Uri
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.ParsableByteArray
import androidx.media3.common.util.TimestampAdjuster
import androidx.media3.exoplayer.analytics.PlayerId
import androidx.media3.exoplayer.hls.HlsExtractorFactory
import androidx.media3.exoplayer.hls.HlsMediaChunkExtractor
import androidx.media3.extractor.ExtractorInput
import androidx.media3.extractor.ExtractorOutput
import androidx.media3.extractor.SeekMap
import androidx.media3.extractor.TrackOutput
import androidx.media3.extractor.ts.H265Reader
import androidx.media3.extractor.ts.SeiReader
import androidx.media3.extractor.ts.TsPayloadReader
import androidx.media3.test.utils.FakeExtractorInput
import androidx.media3.test.utils.FakeExtractorOutput
import androidx.media3.test.utils.FakeTrackOutput
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A Dolby Vision stream in MPEG-TS, read by Media3's own HEVC reader: its samples with and without their RPUs. */
@RunWith(RobolectricTestRunner::class)
class DolbyVisionSegmentsTest {
    private fun unit(hex: String): ByteArray = hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    /** A NAL unit of [type] with [size] bytes after its header, none of them part of a start code. */
    private fun unit(type: Int, size: Int, seed: Int): ByteArray =
        byteArrayOf((type shl 1).toByte(), 0x01) + ByteArray(size) { (0x10 + (it * 7 + seed) % 0xD0).toByte() }

    // A 64×64 Main 10 stream's own parameter sets and slices, as x265 writes them.
    private val delimiter = unit("460110")
    private val parameters = listOf(
        unit("40010c01ffff02200000030090000003000003001e959809"),
        unit("42010102200000030090000003000003001ea0208104d96566924caf016808000003000800000300c040"),
        unit("4401c172b42240"),
    )
    private val keyFrame = unit("2801af1d800850d45d1dd8aa9b")
    private val frame = unit("0201d0097e10c63bfa84")

    private fun rpu(seed: Int, size: Int = 60) = unit(RPU, size, seed)

    /**
     * Three frames of a stream, as NAL units: each one's picture, and what
     * Dolby Vision adds after its slices. Each unit starts with a start
     * code of three bytes, or of four as FFmpeg writes them.
     */
    private class Frames(val pictures: List<List<ByteArray>>, val dolbyVision: List<List<ByteArray>>, val fourBytes: Boolean) {
        fun bytes(units: List<ByteArray>): ByteArray =
            units.fold(byteArrayOf()) { all, unit -> all + (if (fourBytes) byteArrayOf(0, 0, 0, 1) else START) + unit }

        val alone: List<ByteArray> get() = pictures.map(::bytes)
        val whole: List<ByteArray> get() = pictures.zip(dolbyVision) { picture, more -> bytes(picture + more) }
    }

    private fun stream(dolbyVision: List<List<ByteArray>>, fourBytes: Boolean = false): Frames =
        Frames(listOf(listOf(delimiter) + parameters + listOf(keyFrame), listOf(delimiter, frame), listOf(delimiter, frame)), dolbyVision, fourBytes)

    private fun withRpus(fourBytes: Boolean = false): Frames =
        // The second RPU runs over several packets.
        stream(listOf(listOf(rpu(1)), listOf(rpu(2, size = 500)), listOf(rpu(3))), fourBytes)

    /**
     * The samples Media3's MPEG-TS HEVC reader makes of [frames], fed
     * [chunk] bytes at a time as a transport stream's packets feed it, and
     * of the start of a frame after them; through [DolbyVisionSamples]
     * when [whole]. With [zeroBefore], each chunk comes after a zero that
     * isn't the stream's, as a packet's payload comes after its header.
     */
    private fun samples(frames: Frames, whole: Boolean, chunk: Int = 184, zeroBefore: Boolean = false): FakeTrackOutput {
        val output = FakeExtractorOutput()
        val tracks = object : ExtractorOutput {
            override fun track(id: Int, type: Int): TrackOutput = output.track(id, type).let { if (whole) DolbyVisionSamples(it) else it }

            override fun endTracks() = output.endTracks()

            override fun seekMap(seekMap: SeekMap) = output.seekMap(seekMap)
        }
        val reader = H265Reader(SeiReader(emptyList(), MimeTypes.VIDEO_MP2T), MimeTypes.VIDEO_MP2T)
        reader.createTracks(tracks, TsPayloadReader.TrackIdGenerator(1, 1))
        (frames.whole + listOf(frames.bytes(listOf(delimiter, frame)))).forEachIndexed { index, bytes ->
            reader.packetStarted(index * 41_708L, TsPayloadReader.FLAG_DATA_ALIGNMENT_INDICATOR)
            bytes.asList().chunked(chunk).forEach {
                val data = if (zeroBefore) ParsableByteArray(byteArrayOf(0) + it.toByteArray()).apply { position = 1 } else ParsableByteArray(it.toByteArray())
                reader.consume(data)
            }
        }
        return output.trackOutputs.valueAt(0)
    }

    private fun assertSamples(expected: List<ByteArray>, track: FakeTrackOutput) {
        assertEquals(expected.size, track.sampleCount)
        expected.forEachIndexed { index, bytes -> assertArrayEquals("sample $index", bytes, track.getSampleData(index)) }
    }

    /** As [assertSamples], but for the zeros between two samples, which are either's. */
    private fun assertSamplesButForZeros(expected: List<ByteArray>, track: FakeTrackOutput) {
        fun ByteArray.bare() = dropWhile { it == 0.toByte() }.dropLastWhile { it == 0.toByte() }
        assertEquals(expected.size, track.sampleCount)
        expected.forEachIndexed { index, bytes -> assertEquals("sample $index", bytes.bare(), track.getSampleData(index).bare()) }
    }

    @Test
    fun eachSampleTakesInTheRpuThatFollowsItsFrame() {
        val frames = withRpus()
        // Media3's reader leaves the RPUs out: the samples are the pictures alone.
        val plain = samples(frames, whole = false)
        assertSamples(frames.alone, plain)
        for (chunk in CHUNKS) {
            val track = samples(frames, whole = true, chunk)
            assertSamples(frames.whole, track)
            assertEquals(plain.sampleTimesUs, track.sampleTimesUs)
            assertEquals((0..2).map(plain::getSampleFlags), (0..2).map(track::getSampleFlags))
            assertEquals(plain.lastFormat, track.lastFormat)
        }
        assertEquals(C.BUFFER_FLAG_KEY_FRAME, plain.getSampleFlags(0) and C.BUFFER_FLAG_KEY_FRAME)
    }

    @Test
    fun startCodesOfFourBytesAreTakenAsThoseOfThree() {
        val frames = withRpus(fourBytes = true)
        assertSamplesButForZeros(frames.alone, samples(frames, whole = false))
        for (chunk in CHUNKS) {
            assertSamplesButForZeros(frames.whole, samples(frames, whole = true, chunk))
            assertSamplesButForZeros(frames.whole, samples(frames, whole = true, chunk, zeroBefore = true))
        }
    }

    @Test
    fun aZeroThatIsNotTheStreamsMovesNoSample() {
        // Media3's reader takes the byte before a packet's payload for the stream's, and starts a unit there.
        val frames = withRpus()
        for (chunk in CHUNKS) assertSamples(frames.whole, samples(frames, whole = true, chunk, zeroBefore = true))
    }

    @Test
    fun aProfileSevenFramesEnhancementLayerComesWithItToo() {
        val layered = (1..3).map { listOf(unit(ENHANCEMENT_LAYER, 40, it), unit(ENHANCEMENT_LAYER, 300, it + 3), rpu(it)) }
        val frames = stream(layered)
        assertSamples(frames.alone, samples(frames, whole = false))
        for (chunk in CHUNKS) assertSamples(frames.whole, samples(frames, whole = true, chunk))
    }

    @Test
    fun aStreamWithoutDolbyVisionUnitsPassesAsMedia3ReadsIt() {
        for (fourBytes in listOf(false, true)) {
            val frames = stream(listOf(emptyList(), emptyList(), emptyList()), fourBytes)
            for (chunk in CHUNKS) {
                val plain = samples(frames, whole = false, chunk)
                val track = samples(frames, whole = true, chunk)
                assertSamplesButForZeros(frames.alone, plain)
                assertSamples((0..2).map(plain::getSampleData), track)
                assertEquals(plain.sampleTimesUs, track.sampleTimesUs)
            }
        }
    }

    @Test
    fun aSampleHeldForItsRpuPassesAsItCameWhenTheNextIsToldFirst() {
        val track = FakeExtractorOutput().track(1, C.TRACK_TYPE_VIDEO)
        val samples = DolbyVisionSamples(track)
        samples.format(Format.Builder().setSampleMimeType(MimeTypes.VIDEO_H265).build())
        val picture = START + delimiter + START + frame
        val metadata = START + rpu(1)
        // The RPU's end isn't known: no unit starts after it.
        val first = picture + metadata
        samples.sampleData(ParsableByteArray(first), first.size)
        samples.sampleMetadata(0, C.BUFFER_FLAG_KEY_FRAME, picture.size, metadata.size, null)
        assertEquals(0, track.sampleCount)
        // Bytes the reader never showed, then a sample of them.
        samples.sampleData(FakeExtractorInput.Builder().setData(picture).build(), picture.size, false)
        samples.sampleMetadata(41_708, 0, picture.size, 0, null)
        assertSamples(listOf(picture, picture), track)
    }

    /** An extractor that asks for a video and an audio track, and remembers what it got. */
    private class Asking : HlsMediaChunkExtractor {
        var video: TrackOutput? = null
        var audio: TrackOutput? = null

        override fun init(extractorOutput: ExtractorOutput) {
            video = extractorOutput.track(1, C.TRACK_TYPE_VIDEO)
            audio = extractorOutput.track(2, C.TRACK_TYPE_AUDIO)
        }

        override fun read(extractorInput: ExtractorInput): Boolean = false

        override fun isPackedAudioExtractor(): Boolean = false

        override fun isReusable(): Boolean = true

        override fun recreate(): HlsMediaChunkExtractor = Asking()

        override fun onTruncatedSegmentParsed() = Unit
    }

    /** The extractor [DolbyVisionSegments] makes of [asking] for a stream its playlist names [codecs]. */
    private fun extractorFor(codecs: String?, asking: Asking): HlsMediaChunkExtractor {
        val factory = DolbyVisionSegments(HlsExtractorFactory { _, _, _, _, _, _, _ -> asking })
        val format = Format.Builder().setCodecs(codecs).build()
        return factory.createExtractor(Uri.EMPTY, format, null, TimestampAdjuster(0), emptyMap(), FakeExtractorInput.Builder().build(), PlayerId.UNSET)
    }

    @Test
    fun onlyAStreamItsPlaylistNamesDolbyVisionOverHevcIsReadThisWay() {
        val named = Asking()
        val output = FakeExtractorOutput()
        extractorFor("dvh1.08.06,ec-3", named).init(output)
        assertSame(output.trackOutputs[1], (named.video as DolbyVisionSamples).output)
        assertSame(output.trackOutputs[2], named.audio)

        val plain = Asking()
        assertSame(plain, extractorFor("hvc1.2.4.L153.B0,ec-3", plain))

        assertTrue(DolbyVisionSegments.namesDolbyVision(Format.Builder().setCodecs("dvhe.05.06").build()))
        assertTrue(DolbyVisionSegments.namesDolbyVision(Format.Builder().setCodecs("mp4a.40.2,dvh1.07.06").build()))
        for (codecs in listOf(null, "ec-3", "avc1.640028,mp4a.40.2", "dav1.10.09", "dvav.09.05")) {
            assertFalse("$codecs", DolbyVisionSegments.namesDolbyVision(Format.Builder().setCodecs(codecs).build()))
        }
    }

    @Test
    fun aTracksOutputStaysTheSameOverTheSegmentsOneExtractorReads() {
        val asking = Asking()
        val extractor = extractorFor("dvh1.08.06", asking)
        val output = FakeExtractorOutput()
        // Each segment an extractor reads starts with the output it writes to.
        extractor.init(output)
        val first = asking.video
        extractor.init(output)
        assertSame(first, asking.video)
        assertTrue(extractor.isReusable)
        assertTrue(extractor.recreate() !== extractor)
    }

    private companion object {
        val START = byteArrayOf(0, 0, 1)
        val CHUNKS = listOf(1, 2, 3, 4, 5, 7, 184, 4096)
        const val RPU = 62
        const val ENHANCEMENT_LAYER = 63
    }
}
