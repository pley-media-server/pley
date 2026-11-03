package app.pley.android.player.session

import app.pley.android.data.LocalFile
import app.pley.android.data.LocalMedia
import app.pley.android.data.drive.DriveFolder
import app.pley.android.data.drive.DriveResume
import app.pley.android.data.drive.Drives
import app.pley.android.data.drive.Sidecar
import app.pley.android.model.Chapter
import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.ProgressReport
import app.pley.android.model.StreamKind
import app.pley.android.model.UserState
import app.pley.android.network.SubtitleText
import app.pley.android.player.PlayRequest
import app.pley.android.player.subtitles.SubtitleFetcher
import java.io.File
import java.io.IOException
import java.net.URI
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * A video on a drive plugged into the TV, as the player's downloads path
 * takes it ([LocalMedia]), as the Samsung app's LocalPlayerSource has it:
 * a movie titled by its file name, one file at its path with the .srt and
 * .vtt files beside it, where it stopped kept on the device
 * ([DriveResume]). Its own tracks and chapters come from Media3 as it
 * loads ([DriveStreams]). Read off the main thread, once: a sleeping drive
 * takes a moment to answer.
 */
internal class DriveVideo(
    private val path: String,
    private val drives: Drives,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : LocalMedia {
    /** The file's id in the player, made of its path. */
    val fileId: Long = DriveResume.fileIdOf(path)

    @Volatile
    private var described: ItemDetail? = null

    @Volatile
    private var sidecars: List<Sidecar> = emptyList()

    override fun file(fileId: Long): LocalFile? {
        if (fileId != this.fileId) return null
        val subtitles = sidecars.withIndex().associate { (i, s) -> DriveStreams.SIDECAR_FROM + i to uriOf(s.path) }
        return LocalFile(uri = uriOf(path), subtitles = subtitles)
    }

    override suspend fun detail(itemId: Long): ItemDetail = described ?: withContext(io) { describe() }.also { described = it }

    /** Where it got to, kept on the device: past 95 % it was watched, and starts over next time. */
    override suspend fun report(itemId: Long, report: ProgressReport) {
        drives.resume.keep(path, report.positionMs, report.durationMs)
    }

    private fun describe(): ItemDetail {
        val video = File(path)
        if (!video.isFile) throw IOException("$path isn't on the drive")
        val names = try {
            drives.files.read(path.substringBeforeLast('/')).map { it.name }
        } catch (_: IOException) {
            // Its folder can't be listed: it plays without the files beside it.
            emptyList()
        }
        val found = DriveFolder.sidecars(path, names).take(MAX_SIDECARS)
        sidecars = found
        val streams = found.mapIndexed { i, s ->
            MediaStream(
                index = DriveStreams.SIDECAR_FROM + i,
                kind = StreamKind.Subtitle,
                codec = s.codec,
                language = s.language,
                title = s.title,
                forced = s.forced,
                textBased = true,
                external = true,
            )
        }
        return ItemDetail(
            id = PlayRequest.DRIVE_ITEM,
            libraryId = 0,
            kind = ItemKind.Movie,
            title = DriveFolder.title(path),
            userState = UserState(positionMs = drives.resume.kept(path).positionMs),
            files = listOf(
                MediaFile(
                    id = fileId,
                    path = video.name,
                    size = video.length(),
                    container = path.substringAfterLast('.', "").lowercase().ifEmpty { null },
                    streams = streams,
                ),
            ),
        )
    }

    companion object {
        /** The most subtitle files beside a video that it shows. */
        const val MAX_SIDECARS = 32

        /** [path] as the address Media3 and the subtitles read: `file:/storage/1A2B-3C4D/Ad%20Astra.mkv`. */
        fun uriOf(path: String): String = File(path).toURI().toString()
    }
}

/**
 * The subtitle files beside a video on a drive (their `file:` addresses),
 * whole, as the app's WebVTT: SubRip made WebVTT, UTF-16 with a byte-order
 * mark, UTF-8, else Latin-1 ([DriveFolder.decode]). A file larger than
 * [MAX_BYTES] isn't read: no subtitle file comes near it, and its text is
 * copied a few times on the way to WebVTT, beside the video's buffers on a
 * 1 GB TV.
 */
internal class DriveSubtitleFetcher(private val io: CoroutineDispatcher = Dispatchers.IO) : SubtitleFetcher {
    override suspend fun fetch(fileId: Long, index: Int, url: String): SubtitleText = withContext(io) {
        if (!url.startsWith("file:")) throw IOException("Not a file on the drive")
        val file = File(URI(url))
        if (file.length() > MAX_BYTES) throw IOException("A subtitle file too large to read")
        SubtitleText(DriveFolder.toWebVtt(DriveFolder.decode(file.readBytes())), complete = true)
    }

    companion object {
        /** A film's SubRip file is some 100 KB; one with every cue twice over is still far from this. */
        const val MAX_BYTES = 2L * 1024 * 1024
    }
}

/**
 * A file on a drive's streams and chapters, as Media3 found them: the
 * server's names for their codecs, numbered by kind in the file's order
 * (videos, then audio, then text), and the subtitle files beside it after
 * them, from [SIDECAR_FROM], as the server numbers its own.
 */
internal object DriveStreams {
    const val SIDECAR_FROM = 1000

    /** The most tracks of a kind taken from a file: more is a file that isn't right. */
    private const val MAX_PER_KIND = 64

    /** A track's or a chapter's name is cut to this many characters: the file says what it likes. */
    private const val MAX_NAME = 200

    /** A track's language is cut to this, the longest tag (RFC 5646) a player needs to read. */
    private const val MAX_LANGUAGE = 35

    fun streams(tracks: EngineTracks, sidecars: List<MediaStream>): List<MediaStream> {
        var index = 0
        val videos = tracks.video.take(MAX_PER_KIND).map { t ->
            MediaStream(
                index = index++,
                kind = StreamKind.Video,
                codec = codecOf(t.mimeType, t.codecs),
                language = t.language?.take(MAX_LANGUAGE),
                title = t.label?.take(MAX_NAME),
                default = t.default,
                width = t.width.takeIf { it > 0 },
                height = t.height.takeIf { it > 0 },
                frameRate = t.frameRate.takeIf { it > 0 }?.toDouble(),
                bitDepth = t.bitDepth,
                hdr = t.range?.let { it != DynamicRange.Sdr },
                dynamicRange = t.range,
                dvProfile = if (t.range == DynamicRange.DolbyVision) dolbyVisionProfileOf(t.codecs) else null,
            )
        }
        val audio = tracks.audio.take(MAX_PER_KIND).map { t ->
            MediaStream(
                index = index++,
                kind = StreamKind.Audio,
                codec = codecOf(t.mimeType, t.codecs),
                language = t.language?.take(MAX_LANGUAGE),
                title = t.label?.take(MAX_NAME),
                default = t.default,
                channels = t.channels,
            )
        }
        val text = tracks.text.take(MAX_PER_KIND).map { t ->
            val codec = codecOf(t.mimeType, t.codecs)
            MediaStream(
                index = index++,
                kind = StreamKind.Subtitle,
                codec = codec,
                language = t.language?.take(MAX_LANGUAGE),
                title = t.label?.take(MAX_NAME),
                default = t.default,
                forced = t.forced,
                textBased = codec !in PICTURES,
            )
        }
        return videos + audio + text + sidecars
    }

    /** [chapters] each ending where the next starts when the file gives no end, the last at [durationMs]. */
    fun chapters(chapters: List<Chapter>, durationMs: Long): List<Chapter> = chapters.mapIndexed { i, c ->
        val next = chapters.getOrNull(i + 1)?.startMs ?: durationMs
        val end = if (c.endMs > c.startMs) c.endMs else next.coerceAtLeast(c.startMs)
        c.copy(endMs = end, title = c.title?.take(MAX_NAME))
    }

    /** The server's name of a codec from Media3's MIME type: "hevc", "eac3", "subrip". */
    fun codecOf(mimeType: String?, codecs: String?): String {
        if (mimeType == DOLBY_VISION) {
            // Named by its base layer's codec, as the server names Dolby Vision streams.
            return when (codecs?.substringBefore('.')?.lowercase()) {
                "dvav", "dva1" -> "h264"
                "dav1" -> "av1"
                else -> "hevc"
            }
        }
        return mimeType?.let { CODECS[it] ?: it.substringAfter('/') }.orEmpty()
    }

    /** Dolby Vision's profile from its codecs: 7 for "dvhe.07.06"; `null` when they don't say. */
    fun dolbyVisionProfileOf(codecs: String?): Int? = codecs?.split('.')?.getOrNull(1)?.toIntOrNull()

    private const val DOLBY_VISION = "video/dolby-vision"

    /** Subtitles drawn as pictures. */
    private val PICTURES = setOf("hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle")

    private val CODECS = mapOf(
        "video/avc" to "h264",
        "video/hevc" to "hevc",
        "video/av01" to "av1",
        "video/x-vnd.on2.vp9" to "vp9",
        "video/x-vnd.on2.vp8" to "vp8",
        "video/mpeg2" to "mpeg2video",
        "video/mp4v-es" to "mpeg4",
        "audio/mp4a-latm" to "aac",
        "audio/mpeg" to "mp3",
        "audio/mpeg-L2" to "mp2",
        "audio/ac3" to "ac3",
        "audio/eac3" to "eac3",
        "audio/eac3-joc" to "eac3",
        "audio/true-hd" to "truehd",
        "audio/vnd.dts" to "dts",
        "audio/vnd.dts.hd" to "dts",
        "audio/vnd.dts.hd;profile=lbr" to "dts",
        "audio/vnd.dts.uhd;profile=p2" to "dts",
        "audio/flac" to "flac",
        "audio/opus" to "opus",
        "audio/vorbis" to "vorbis",
        "audio/alac" to "alac",
        "audio/raw" to "pcm_s16le",
        "application/x-subrip" to "subrip",
        "text/x-ssa" to "ass",
        "text/vtt" to "webvtt",
        "application/x-quicktime-tx3g" to "mov_text",
        "application/pgs" to "hdmv_pgs_subtitle",
        "application/vobsub" to "dvd_subtitle",
        "application/dvbsubs" to "dvb_subtitle",
    )
}
