package app.pley.android.player.engine

import android.content.Context
import android.os.Handler
import androidx.annotation.OptIn
import androidx.media3.common.Format
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.Renderer
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.mediacodec.MediaCodecAdapter
import androidx.media3.exoplayer.mediacodec.MediaCodecInfo
import androidx.media3.exoplayer.mediacodec.MediaCodecSelector
import androidx.media3.exoplayer.video.MediaCodecVideoRenderer
import androidx.media3.exoplayer.video.VideoRendererEventListener

/**
 * Media3's renderers with pley's decoder rules: the video renderer weighs
 * each format against [rules] (size, rate, bit depth, HDR) before choosing a
 * decoder; a decoder that fails to start falls back to the next one listed
 * (a Dolby Vision decoder to the base layer's HEVC decoder); Fire TV queues
 * buffers asynchronously, as Amazon asks, unless its model's kill switch
 * says no; the sound can be moved against the picture ([audioDelay],
 * [DelayedAudioSink]). Music gets no video renderer.
 */
@OptIn(markerClass = [UnstableApi::class])
class PleyRenderersFactory(
    context: Context,
    private val rules: DecoderRules,
    asyncQueueing: Boolean,
    private val music: Boolean,
    private val audioDelay: AudioDelay? = null,
    /** Each video decoder started, with the MIME type it decodes; before Media3's listeners hear of it. */
    private val onVideoCodec: (name: String, mimeType: String) -> Unit = { _, _ -> },
) : DefaultRenderersFactory(context) {
    init {
        setEnableDecoderFallback(true)
        setMediaCodecSelector(rules.selector())
        setExtensionRendererMode(EXTENSION_RENDERER_MODE_OFF)
        if (asyncQueueing) forceEnableMediaCodecAsynchronousQueueing()
    }

    override fun buildVideoRenderers(
        context: Context,
        extensionRendererMode: Int,
        mediaCodecSelector: MediaCodecSelector,
        enableDecoderFallback: Boolean,
        eventHandler: Handler,
        eventListener: VideoRendererEventListener,
        allowedVideoJoiningTimeMs: Long,
        out: ArrayList<Renderer>,
    ) {
        if (music) return
        val builder = MediaCodecVideoRenderer.Builder(context)
            .setCodecAdapterFactory(getCodecAdapterFactory())
            .setMediaCodecSelector(mediaCodecSelector)
            .setAllowedJoiningTimeMs(allowedVideoJoiningTimeMs)
            .setEnableDecoderFallback(enableDecoderFallback)
            .setEventHandler(eventHandler)
            .setEventListener(eventListener)
            .setMaxDroppedFramesToNotify(MAX_DROPPED_VIDEO_FRAME_COUNT_TO_NOTIFY)
        out.add(PolicyVideoRenderer(builder, context, rules, onVideoCodec))
    }

    override fun buildAudioSink(context: Context, enableFloatOutput: Boolean, enableAudioTrackPlaybackParams: Boolean): AudioSink? {
        val sink = super.buildAudioSink(context, enableFloatOutput, enableAudioTrackPlaybackParams) ?: return null
        return audioDelay?.let { DelayedAudioSink(sink, it) } ?: sink
    }

    override fun buildCameraMotionRenderers(context: Context, extensionRendererMode: Int, out: ArrayList<Renderer>) = Unit
}

/**
 * Media3's video renderer, asking the decoder lists through a selector that
 * knows the format, with Dolby Vision decoders first for Dolby Vision that
 * comes out as Dolby Vision ([DecoderRules.dolbyVisionDecoders]), and
 * saying which MIME type each decoder it starts was chosen for (a Dolby
 * Vision stream's base layer goes to an HEVC decoder): the same renderer
 * otherwise.
 */
@OptIn(markerClass = [UnstableApi::class])
internal class PolicyVideoRenderer(
    builder: MediaCodecVideoRenderer.Builder,
    private val context: Context,
    private val rules: DecoderRules,
    private val onCodec: (name: String, mimeType: String) -> Unit = { _, _ -> },
) : MediaCodecVideoRenderer(builder) {
    override fun onCodecInitialized(
        name: String,
        configuration: MediaCodecAdapter.Configuration,
        initializedTimestampMs: Long,
        initializationDurationMs: Long,
    ) {
        onCodec(name, configuration.codecInfo.mimeType)
        super.onCodecInitialized(name, configuration, initializedTimestampMs, initializationDurationMs)
    }

    override fun supportsFormat(mediaCodecSelector: MediaCodecSelector, format: Format): Int =
        super.supportsFormat(rules.selectorFor(mediaCodecSelector, format), format)

    override fun getDecoderInfos(
        mediaCodecSelector: MediaCodecSelector,
        format: Format,
        requiresSecureDecoder: Boolean,
    ): List<MediaCodecInfo> =
        rules.dolbyVisionDecoders(context, mediaCodecSelector, format, requiresSecureDecoder, configuration.tunneling)
            ?: super.getDecoderInfos(rules.selectorFor(mediaCodecSelector, format), format, requiresSecureDecoder)
}
