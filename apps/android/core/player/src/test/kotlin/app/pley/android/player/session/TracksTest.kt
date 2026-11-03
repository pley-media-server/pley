package app.pley.android.player.session

import app.pley.android.caps.CapsSnapshot
import app.pley.android.fixtures.FixtureIds
import app.pley.android.model.PleyJson
import app.pley.android.model.StreamChoice
import app.pley.android.model.StreamKind
import app.pley.android.player.PlayRequest
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Audio and subtitles: switched by the engine in direct play, by the server in streams, remembered for the item. */
@RunWith(RobolectricTestRunner::class)
class TracksTest {
    private var h = SessionHarness(caps = TestCaps.hdrTv())

    @After
    fun close() = h.close()

    private fun use(caps: CapsSnapshot) {
        h.close()
        h = SessionHarness(caps = caps)
    }

    private fun streamChoices(itemId: Long): List<StreamChoice> =
        h.journal.writes("PUT", "/api/items/$itemId/streams").map { PleyJson.decodeFromString(StreamChoice.serializer(), it.body) }

    @Test
    fun `direct play switches audio in the engine and remembers the choice`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val file = movie.files.first()
        val audio = file.streams.filter { it.kind == StreamKind.Audio }
        val c = h.play(PlayRequest(movie.id))
        assertFalse(h.engine.source!!.hls)
        assertEquals("video/x-matroska", h.engine.source!!.mimeType)
        // No track named: the server picks; the one its answer names is chosen in the engine, by its order in the file.
        assertNull(h.playbackRequests().single().audioIndex)
        assertEquals(0 to audio[0].language, h.engine.audioChoices.single())
        assertEquals(listOf(true, false), c.state.value.audio.map { it.selected })

        c.selectAudio(audio[1].index.toLong())
        h.settle()
        assertEquals(1 to audio[1].language, h.engine.audioChoices.last())
        assertEquals(1, h.playbackRequests().size)
        assertEquals(listOf(StreamChoice(file.id, audio[1].index, null)), streamChoices(movie.id))
        assertEquals(listOf(false, true), c.state.value.audio.map { it.selected })
        // Picked, the audio goes with a subtitle change too.
        val text = file.streams.first { it.kind == StreamKind.Subtitle && it.textBased == true }
        c.selectSubtitle(text.index.toLong())
        h.settle()
        assertEquals(StreamChoice(file.id, audio[1].index, text.index), streamChoices(movie.id).last())
    }

    @Test
    fun `the server's audio plays in direct play, and subtitles changed alone don't save it`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val file = movie.files.first()
        val audio = file.streams.filter { it.kind == StreamKind.Audio }
        // The French E-AC-3 picked on another device: remembered, not the file's default.
        val french = audio.first { it.codec == "eac3" }
        runBlocking { h.api.setStreams(movie.id, StreamChoice(file.id, french.index, null)) }
        val c = h.play(PlayRequest(movie.id))
        assertNull(h.playbackRequests().single().audioIndex)
        assertFalse(h.engine.source!!.hls)
        // The server's answer names it: the engine switches to it, the menu marks it.
        assertEquals(audio.indexOf(french) to french.language, h.engine.audioChoices.last())
        assertTrue(c.state.value.audio.first { it.index == french.index.toLong() }.selected)

        val text = file.streams.first { it.kind == StreamKind.Subtitle && it.textBased == true }
        c.selectSubtitle(text.index.toLong())
        h.settle()
        val body = h.journal.writes("PUT", "/api/items/${movie.id}/streams").last().body
        assertFalse(body, body.contains("audio_index"))
        assertEquals(StreamChoice(file.id, null, text.index), streamChoices(movie.id).last())
    }

    @Test
    fun `a stream's audio switch starts a session with audio_index`() {
        use(capsOf(TestDevices.sdrPhone))
        val movie = Catalogue.movieWithTracks(h.api)
        val audio = movie.files.first().streams.filter { it.kind == StreamKind.Audio }
        val c = h.play(PlayRequest(movie.id))
        assertTrue(h.engine.source!!.hls)
        // A stream: the server picks the audio, and says which.
        assertNull(h.playbackRequests().single().audioIndex)
        assertTrue(c.state.value.audio.first { it.index == audio[0].index.toLong() }.selected)
        val from = h.playbackRequests().single().startMs!!
        h.engine.ready()
        h.advance(2_000)
        c.selectAudio(audio[1].index.toLong())
        h.settle()
        val again = h.playbackRequests().last()
        assertEquals(audio[1].index, again.audioIndex)
        assertEquals(from + 2_000L, again.startMs)
    }

    @Test
    fun `audio this device can't play is sent to the server even for a direct play`() {
        use(TestCaps.hdrTv(passthrough = listOf("ac3")))
        val movie = Catalogue.movieWithTracks(h.api)
        val eac3 = movie.files.first().streams.first { it.kind == StreamKind.Audio && it.codec == "eac3" }
        h.play(PlayRequest(movie.id, audioIndex = eac3.index.toLong()))
        assertEquals(eac3.index, h.playbackRequests().single().audioIndex)
    }

    @Test
    fun `subtitles off is off, embedded ones are the engine's, pictures in a stream are burned in`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val subs = movie.files.first().streams.filter { it.kind == StreamKind.Subtitle }
        val text = subs.first { it.textBased == true }
        val picture = subs.first { it.textBased == false }
        val c = h.play(PlayRequest(movie.id))
        assertEquals(null to null, h.engine.textChoices.last())
        assertEquals("Off", c.state.value.subtitles.first().label)
        assertTrue(c.state.value.subtitles.first().selected)

        c.selectSubtitle(text.index.toLong())
        h.settle()
        assertEquals(subs.indexOf(text) to text.language, h.engine.textChoices.last())
        // Pictures play in the engine too, in direct play: nothing burned in.
        c.selectSubtitle(picture.index.toLong())
        h.settle()
        assertEquals(subs.indexOf(picture) to picture.language, h.engine.textChoices.last())
        assertEquals(1, h.playbackRequests().size)
        assertFalse(c.state.value.subtitles.first { it.index == picture.index.toLong() }.burnIn)

        c.selectSubtitle(null)
        h.settle()
        assertEquals(null to null, h.engine.textChoices.last())
        assertEquals(
            listOf(text.index, picture.index, null),
            streamChoices(movie.id).map { it.subtitleIndex },
        )
        // The app's copy of the item (its page, playing it again) is told each time.
        assertEquals(streamChoices(movie.id).map { movie.id to it }, h.tracksChosen)
        assertEquals(0, h.api.count("subtitles"))
    }

    @Test
    fun `the subtitle style is for text: not for pictures, nor with subtitles off`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val subs = movie.files.first().streams.filter { it.kind == StreamKind.Subtitle }
        val c = h.play(PlayRequest(movie.id))
        assertFalse(c.state.value.subtitleStyled)
        c.selectSubtitle(subs.first { it.textBased == true }.index.toLong())
        h.settle()
        assertTrue(c.state.value.subtitleStyled)
        c.selectSubtitle(subs.first { it.textBased == false }.index.toLong())
        h.settle()
        assertFalse(c.state.value.subtitleStyled)
    }

    @Test
    fun `pictures in a stream start a burn-in session, and back`() {
        use(capsOf(TestDevices.sdrPhone))
        val movie = Catalogue.movieWithTracks(h.api)
        val picture = movie.files.first().streams.first { it.kind == StreamKind.Subtitle && it.textBased == false }
        val c = h.play(PlayRequest(movie.id))
        assertNull(h.playbackRequests().single().subtitleIndex)
        assertTrue(c.state.value.subtitles.first { it.index == picture.index.toLong() }.burnIn)
        c.selectSubtitle(picture.index.toLong())
        h.settle()
        assertEquals(picture.index, h.playbackRequests().last().subtitleIndex)
        c.selectSubtitle(null)
        h.settle()
        assertEquals(3, h.playbackRequests().size)
        assertNull(h.playbackRequests().last().subtitleIndex)
    }

    @Test
    fun `a stream's text subtitles come from the server, asked again while partial, and merged`() {
        use(capsOf(TestDevices.sdrPhone))
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_HDR10))
        assertTrue(h.engine.source!!.hls)
        h.engine.durationMs = 20_000
        h.engine.ready()
        c.selectSubtitle(FixtureIds.SAMPLE_HDR10_SUBTITLE.toLong())
        h.advance(1_500)
        assertEquals(listOf("The fixture server plays this."), c.cues.value.map { it.text.toString() })
        assertEquals(1, h.api.count("subtitles"))
        // The first answer was partial: asked again 3 s later, whole.
        h.advance(3_000)
        assertEquals(2, h.api.count("subtitles"))
        c.seekTo(17_000)
        h.advance(250)
        assertEquals(listOf("The end credits."), c.cues.value.map { it.text.toString() })
        h.advance(6_000)
        assertEquals(2, h.api.count("subtitles"))
        // The stream itself never shows subtitles.
        assertEquals(null to null, h.engine.textChoices.last())
    }

    @Test
    fun `a subtitle file beside the media comes from the server in direct play`() {
        val c = h.play(PlayRequest(FixtureIds.SAMPLE_HDR10))
        assertFalse(h.engine.source!!.hls)
        h.engine.ready()
        c.selectSubtitle(FixtureIds.SAMPLE_HDR10_EXTERNAL_SUBTITLE.toLong())
        h.advance(1_500)
        assertEquals(listOf("Le serveur de test joue ceci."), c.cues.value.map { it.text.toString() })
        assertEquals(null to null, h.engine.textChoices.last())
    }

    @Test
    fun `the tracks start as asked, else as the profile chose`() {
        val movie = Catalogue.movieWithTracks(h.api)
        val file = movie.files.first()
        val audio = file.streams.filter { it.kind == StreamKind.Audio }
        val text = file.streams.first { it.kind == StreamKind.Subtitle && it.textBased == true }
        h.play(PlayRequest(movie.id)).apply {
            selectAudio(audio[1].index.toLong())
            selectSubtitle(text.index.toLong())
            h.settle()
            close()
        }
        h.settle()
        val again = h.play(PlayRequest(movie.id))
        assertTrue(again.state.value.audio.first { it.index == audio[1].index.toLong() }.selected)
        assertTrue(again.state.value.subtitles.first { it.index == text.index.toLong() }.selected)
        again.close()
        h.settle()
        val asked = h.play(PlayRequest(movie.id, audioIndex = audio[0].index.toLong()))
        assertTrue(asked.state.value.audio.first { it.index == audio[0].index.toLong() }.selected)
    }
}
