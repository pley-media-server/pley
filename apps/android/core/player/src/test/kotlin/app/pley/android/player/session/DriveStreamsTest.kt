package app.pley.android.player.session

import app.pley.android.model.Chapter
import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** A file on a drive, as Media3 found it: the streams and chapters the player shows. */
class DriveStreamsTest {
    private val tracks = EngineTracks(
        video = listOf(EngineTrack(null, "video/dolby-vision", null, supported = true, codecs = "dvhe.07.06", width = 3840, height = 2160, frameRate = 23.976f, range = DynamicRange.DolbyVision, bitDepth = 10)),
        audio = listOf(
            EngineTrack("en", "audio/true-hd", 8, supported = true, label = "Atmos", default = true),
            EngineTrack("en", "audio/ac3", 6, supported = true, label = "Compatibility"),
            EngineTrack("ru", "audio/mp4a-latm", 2, supported = true),
        ),
        text = listOf(
            EngineTrack("en", "application/x-subrip", null, supported = true, forced = true),
            EngineTrack("fr", "application/pgs", null, supported = true),
        ),
    )

    @Test
    fun streamsAreNumberedByKindInTheFilesOrderWithTheFilesBesideItAfter() {
        val beside = MediaStream(index = DriveStreams.SIDECAR_FROM, kind = StreamKind.Subtitle, codec = "subrip", language = "en", external = true, textBased = true)
        val streams = DriveStreams.streams(tracks, listOf(beside))
        assertEquals((0..5).toList() + DriveStreams.SIDECAR_FROM, streams.map { it.index })
        assertEquals(listOf("hevc", "truehd", "ac3", "aac", "subrip", "hdmv_pgs_subtitle", "subrip"), streams.map { it.codec })
        val video = streams[0]
        assertEquals(DynamicRange.DolbyVision, video.dynamicRange)
        assertEquals(7, video.dvProfile)
        assertEquals(3840 to 2160, video.width to video.height)
        assertEquals("Atmos", streams[1].title)
        assertEquals(listOf(true, false, false), streams.filter { it.kind == StreamKind.Audio }.map { it.default })
        assertEquals(8, streams[1].channels)
        // Pictures are pictures; the forced flag stays.
        assertEquals(listOf(true, false, true), streams.filter { it.kind == StreamKind.Subtitle }.map { it.textBased })
        assertEquals(true, streams[4].forced)
    }

    @Test
    fun chaptersWithoutAnEndEndWhereTheNextStartsTheLastAtTheEnd() {
        val chapters = DriveStreams.chapters(listOf(Chapter(0, 0, "Opening"), Chapter(600_000, 0, "The Road"), Chapter(900_000, 1_000_000, "Fury")), 7_380_000)
        assertEquals(listOf(0L to 600_000L, 600_000L to 900_000L, 900_000L to 1_000_000L), chapters.map { it.startMs to it.endMs })
        assertEquals(listOf("Opening", "The Road", "Fury"), chapters.map { it.title })
        // A name as long as a file likes is cut short.
        assertEquals(200, DriveStreams.chapters(listOf(Chapter(0, 0, "x".repeat(10_000))), 1_000).single().title?.length)
    }

    @Test
    fun codecsGoByTheServersNames() {
        assertEquals("hevc", DriveStreams.codecOf("video/dolby-vision", "dvh1.08.06"))
        assertEquals("av1", DriveStreams.codecOf("video/dolby-vision", "dav1.10.06"))
        assertEquals("h264", DriveStreams.codecOf("video/dolby-vision", "dvav.09.05"))
        assertEquals("eac3", DriveStreams.codecOf("audio/eac3-joc", null))
        assertEquals("dts", DriveStreams.codecOf("audio/vnd.dts.hd", null))
        assertEquals("ass", DriveStreams.codecOf("text/x-ssa", null))
        assertEquals("x-unknown", DriveStreams.codecOf("audio/x-unknown", null))
        assertEquals(8, DriveStreams.dolbyVisionProfileOf("dvh1.08.06"))
        assertNull(DriveStreams.dolbyVisionProfileOf(null))
    }
}
