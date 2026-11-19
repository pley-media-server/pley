package app.pley.android.player.session

import app.pley.android.caps.CapsSnapshot
import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemDetail
import app.pley.android.model.StreamKind
import app.pley.android.model.caps.ClientCaps
import app.pley.android.model.caps.DolbyVisionCaps
import app.pley.android.model.caps.VideoDecoderCaps
import app.pley.android.network.PleyApi
import kotlinx.coroutines.runBlocking

/** Capability snapshots written by hand, for what the probe's test devices don't cover. */
object TestCaps {
    private val hevc10 = VideoDecoderCaps("hevc", hardware = true, profiles = listOf("Main", "Main 10"), maxWidth = 3840, maxHeight = 2160, maxFps = 60.0, maxBitDepth = 10)
    private val avc = VideoDecoderCaps("h264", hardware = true, profiles = listOf("High"), maxWidth = 3840, maxHeight = 2160, maxFps = 60.0, maxBitDepth = 8)

    /**
     * A TV on an HDR10 screen and a receiver that takes AC-3 and E-AC-3:
     * the HDR10 movies play as they are, with any of their audio tracks (the
     * player switches them itself, as the app's reports say). [dv] Dolby
     * Vision profiles decoded (with a Dolby Vision screen when any).
     */
    fun hdrTv(dv: List<Int> = emptyList(), passthrough: List<String> = listOf("ac3", "eac3")): CapsSnapshot {
        val types = listOf(DynamicRange.Hdr10, DynamicRange.Hlg) + if (dv.isNotEmpty()) listOf(DynamicRange.DolbyVision) else emptyList()
        return CapsSnapshot(
            report = ClientCaps(
                containers = listOf("mp4", "m4v", "mov", "mkv", "matroska", "webm", "ts"),
                videoCodecs = listOf("h264", "hevc"),
                audioCodecs = listOf("aac", "mp3", "opus", "flac") + passthrough,
                hdr = true,
                dolbyVision = dv.isNotEmpty(),
                capsVersion = ClientCaps.VERSION_2,
                video = listOf(avc, hevc10),
                hdrTypes = types,
                dolbyVisionProfiles = dv.map { DolbyVisionCaps(it, 9) },
                audioDecode = listOf("aac", "mp3", "opus", "flac"),
                audioPassthrough = passthrough,
                audioChannels = 8,
                audioTrackSwitching = true,
            ),
            lines = emptyList(),
        )
    }
}

/** Fixture catalog lookups that the ids of FixtureIds don't name. */
object Catalogue {
    /** The first movie whose file is an MKV with two audio tracks and subtitles shown as pictures. */
    fun movieWithTracks(api: PleyApi): ItemDetail = runBlocking {
        for (id in 1L..200L) {
            val item = api.item(id)
            val file = item.files.firstOrNull() ?: continue
            if (file.container == "mkv" &&
                file.streams.count { it.kind == StreamKind.Audio } == 2 &&
                file.streams.any { it.kind == StreamKind.Subtitle && it.textBased == false }
            ) {
                return@runBlocking item
            }
        }
        error("no such movie in the fixtures")
    }
}
