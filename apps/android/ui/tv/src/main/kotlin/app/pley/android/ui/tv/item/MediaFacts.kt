package app.pley.android.ui.tv.item

import android.content.res.Resources
import androidx.compose.runtime.Immutable
import app.pley.android.common.formatWords
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.bitDepthLabel
import app.pley.android.model.format.channelsLabel
import app.pley.android.model.format.codecLabel
import app.pley.android.model.format.colorMatrixLabel
import app.pley.android.model.format.colorPrimariesLabel
import app.pley.android.model.format.colorTransferLabel
import app.pley.android.model.format.dimensionsLabel
import app.pley.android.model.format.dolbyVisionDetails
import app.pley.android.model.format.formatBitrate
import app.pley.android.model.format.formatBytes
import app.pley.android.model.format.formatDuration
import app.pley.android.model.format.frameRateLabel
import app.pley.android.model.format.hdrFormatLabel
import app.pley.android.model.format.languageName
import app.pley.android.model.format.profileLevelLabel
import app.pley.android.model.format.sampleRateLabel
import app.pley.android.screens.item.fileName
import app.pley.android.ui.tv.R

/** One line of a readout: a muted label and its value. */
@Immutable
internal data class MediaFact(val label: String, val value: String)

/** Facts under a title: "Video", "Audio 2", "This TV". */
@Immutable
internal data class MediaFacts(val title: String, val facts: List<MediaFact>)

/**
 * Files and streams in full, as the item page's Media info and the
 * player's Info panel list them: everything the server's media info says,
 * in the words of [app.pley.android.model.format]. Facts that aren't known
 * are left out.
 */
internal object MediaFactText {
    /** Name, container, size, duration and overall bitrate. */
    fun file(r: Resources, f: MediaFile): List<MediaFact> = facts {
        val words = formatWords(r)
        add(r.getString(R.string.tv_readout_name), fileName(f.path))
        add(r.getString(R.string.tv_readout_container), f.container?.uppercase())
        add(r.getString(R.string.tv_readout_size), f.size.takeIf { it > 0 }?.let { formatBytes(it, words) })
        add(r.getString(R.string.tv_readout_duration), f.durationMs?.takeIf { it > 0 }?.let { formatDuration(it, words) })
        add(r.getString(R.string.tv_readout_overall_bitrate), formatBitrate(f.bitrate, words))
    }

    /** Codec, profile and level, size, frame rate, bit depth, HDR and Dolby Vision, colour, bitrate. */
    fun video(r: Resources, s: MediaStream): List<MediaFact> = facts {
        val words = formatWords(r)
        add(r.getString(R.string.tv_readout_codec), codecLabel(s.codec))
        add(r.getString(R.string.tv_readout_profile), profileLevelLabel(s, words))
        add(r.getString(R.string.tv_readout_resolution), dimensionsLabel(s.width, s.height))
        add(
            r.getString(R.string.tv_readout_frame_rate),
            frameRateLabel(s.frameRate, words)?.let { if (s.interlaced == true) r.getString(R.string.tv_readout_interlaced, it) else it },
        )
        add(r.getString(R.string.tv_readout_bit_depth), bitDepthLabel(s, words))
        add(r.getString(R.string.tv_readout_hdr), hdrFormatLabel(s))
        add(r.getString(R.string.tv_readout_dolby_vision), dolbyVisionDetails(s, words))
        add(r.getString(R.string.tv_readout_primaries), colorPrimariesLabel(s.colorPrimaries))
        add(r.getString(R.string.tv_readout_transfer), colorTransferLabel(s.colorTransfer))
        add(r.getString(R.string.tv_readout_matrix), colorMatrixLabel(s.colorSpace))
        add(r.getString(R.string.tv_readout_bitrate), formatBitrate(s.bitrate, words))
        add(r.getString(R.string.tv_readout_title), s.title)
    }

    /** Codec and profile, channels, sample rate, language, title, bitrate, default and forced. */
    fun audio(r: Resources, s: MediaStream): List<MediaFact> = facts {
        val words = formatWords(r)
        add(r.getString(R.string.tv_readout_codec), codecLabel(s.codec))
        add(r.getString(R.string.tv_readout_profile), s.profile)
        add(r.getString(R.string.tv_readout_channels), channels(s, words))
        add(r.getString(R.string.tv_readout_sample_rate), sampleRateLabel(s.sampleRate, words))
        add(r.getString(R.string.tv_readout_language), language(s, words))
        add(r.getString(R.string.tv_readout_title), s.title)
        add(r.getString(R.string.tv_readout_bitrate), formatBitrate(s.bitrate, words))
        add(r.getString(R.string.tv_readout_flags), flags(r, s))
    }

    /** Format (text or picture), language, title, default, forced and beside the media. */
    fun subtitle(r: Resources, s: MediaStream): List<MediaFact> = facts {
        add(r.getString(R.string.tv_readout_format), format(r, s))
        add(r.getString(R.string.tv_readout_language), language(s, formatWords(r)))
        add(r.getString(R.string.tv_readout_title), s.title)
        add(r.getString(R.string.tv_readout_flags), flags(r, s))
    }

    /** Every stream of [f], each under its kind ("Video", "Audio 2"), in kind then index order. */
    fun streams(r: Resources, f: MediaFile): List<MediaFacts> {
        val known = f.streams.filter { it.kind != StreamKind.Unknown }.sortedWith(compareBy({ it.kind.ordinal }, { it.index }))
        return known.groupBy { it.kind }.flatMap { (kind, streams) ->
            val name = r.getString(
                when (kind) {
                    StreamKind.Video -> R.string.tv_readout_video
                    StreamKind.Audio -> R.string.tv_readout_audio
                    else -> R.string.tv_readout_subtitles
                },
            )
            streams.mapIndexed { i, s ->
                val title = if (streams.size > 1) r.getString(R.string.tv_readout_stream, name, i + 1) else name
                MediaFacts(title, of(r, s))
            }
        }
    }

    /** [video], [audio] or [subtitle], by the stream's kind. */
    fun of(r: Resources, s: MediaStream): List<MediaFact> = when (s.kind) {
        StreamKind.Video -> video(r, s)
        StreamKind.Audio -> audio(r, s)
        else -> subtitle(r, s)
    }

    /** "5.1 (5.1(side))", "Stereo", in [words]. */
    fun channels(s: MediaStream, words: FormatWords): String? {
        val count = channelsLabel(s.channels, words = words).takeIf { it.isNotEmpty() }
        val layout = s.channelLayout?.takeIf { it.isNotBlank() }
        return when {
            count == null -> layout
            layout == null || layout.equals(count, ignoreCase = true) -> count
            else -> "$count ($layout)"
        }
    }

    /** "English" ("Anglais" in French words), else the code as the file has it. */
    fun language(s: MediaStream, words: FormatWords): String? = languageName(s.language, words) ?: s.language?.takeIf { it.isNotBlank() }

    /** "text, SRT" or "picture, PGS". */
    fun format(r: Resources, s: MediaStream): String {
        val kind = when (s.textBased) {
            true -> r.getString(R.string.tv_readout_text)
            false -> r.getString(R.string.tv_readout_image)
            null -> null
        }
        return listOfNotNull(codecLabel(s.codec).takeIf { it.isNotEmpty() }, kind).joinToString(", ")
    }

    private fun flags(r: Resources, s: MediaStream): String = listOfNotNull(
        r.getString(R.string.tv_readout_default).takeIf { s.default },
        r.getString(R.string.tv_readout_forced).takeIf { s.forced },
        r.getString(R.string.tv_readout_external).takeIf { s.external },
    ).joinToString(", ")

    private inline fun facts(block: FactList.() -> Unit): List<MediaFact> = FactList().apply(block).list

    class FactList {
        val list = mutableListOf<MediaFact>()

        /** Adds the line unless [value] is unknown or empty. */
        fun add(label: String, value: String?) {
            if (!value.isNullOrBlank()) list += MediaFact(label, value)
        }
    }
}
