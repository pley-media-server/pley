package app.pley.android.fixtures

import app.pley.android.model.CastKind
import app.pley.android.model.CastRequest
import app.pley.android.model.Control
import app.pley.android.model.DeviceKind
import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.JobState
import app.pley.android.model.LanguageChoice
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.Me
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingKind
import app.pley.android.model.PlayingSession
import app.pley.android.model.ProgressReport
import app.pley.android.model.ProgressState
import app.pley.android.model.QualityOption
import app.pley.android.model.RatingRequest
import app.pley.android.model.RemoteDevice
import app.pley.android.model.ServerEvent
import app.pley.android.model.SessionStatus
import app.pley.android.model.SessionTracks
import app.pley.android.model.StreamAction
import app.pley.android.model.StreamChoice
import app.pley.android.model.StreamKind
import app.pley.android.model.SubtitleMode
import app.pley.android.model.SubtitleTrack
import app.pley.android.model.TrackDefaults
import app.pley.android.model.TrackOption
import app.pley.android.model.TranscodeInfo
import app.pley.android.model.TrickplayInfo
import app.pley.android.model.caps.ClientCaps
import java.util.UUID
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import mockwebserver3.MockResponse

/**
 * Playing, and what playing writes (docs/API.md, "Playback", "Items"):
 * sessions answered direct or as HLS, the files with Range, the HLS stream,
 * subtitles as WebVTT (one track partial the first time) and pictures as
 * .mks, seek previews and chapter pictures; sessions stopped from elsewhere
 * (`session_stopped`, then 410), listed for the remote with their
 * tracks and sent its commands (`player_command`), the devices it casts
 * to (the event streams' receivers); progress, watched marks, ratings,
 * track choices and track defaults, which change what the profile reads
 * next (watch state changes tell its streams: `user_state_changed`).
 */
internal class PlayRoutes(private val s: State) {
    private val catalog get() = s.catalog

    fun routes(): List<Route> = listOf(
        route("POST", "/api/playback", PLAY) { call, c, _ -> start(call, c) },
        route("GET", "/api/playback/*", PLAY) { _, c, v ->
            val session = s.sessions[v[0]]?.takeIf { it.profileId == c.profile.id } ?: return@route Reply.notFound("no such session")
            Reply.json(SessionStatus.serializer(), status(session))
        },
        route("DELETE", "/api/playback/*", PLAY) { _, c, v ->
            s.sessions[v[0]]?.takeIf { it.profileId == c.profile.id }?.let {
                s.sessions.remove(it.id)
                s.events.emit(ServerEvent.SessionsChanged)
            }
            Reply.noContent()
        },
        route("GET", "/api/playback/*/*", PLAY) { _, c, v -> hls(v[0], v[1], c) },
        route("DELETE", "/api/sessions/*", PLAY) { call, c, v -> stopSession(call, c, v[0]) },
        route("GET", "/api/sessions", PLAY) { _, c, _ -> Reply.json(ListSerializer(PlayingSession.serializer()), playing(c)) },
        route("POST", "/api/sessions/*/command", PLAY) { call, c, v -> command(call, c, v[0]) },
        route("GET", "/api/remote/devices", PLAY) { _, c, _ -> Reply.json(ListSerializer(RemoteDevice.serializer()), s.events.devices(c.profile.id)) },
        route("POST", "/api/remote/devices/*/cast", PLAY) { call, c, v -> cast(call, c, v[0]) },
        route("GET", "/api/files/*/stream", PLAY, free = true) { call, _, v -> stream(call, v[0]) },
        route("GET", "/api/files/*/subtitles/*", PLAY) { _, _, v -> subtitles(v[0], v[1]) },
        route("GET", "/api/files/*/trickplay", PLAY, free = true) { _, _, v ->
            val file = v[0].toLongOrNull()?.let(catalog::file) ?: return@route Reply.notFound("no such file")
            Reply.json(TrickplayInfo.serializer(), trickplay(file.second))
        },
        route("GET", "/api/files/*/trickplay/*", PLAY, free = true) { _, _, v ->
            val file = v[0].toLongOrNull()?.let(catalog::file) ?: return@route Reply.notFound("no such file")
            val info = trickplay(file.second)
            val sheet = v[1].removeSuffix(".jpg").toIntOrNull()?.takeIf { it in 0 until info.sheets } ?: return@route Reply.notFound("no such sheet")
            Reply.picture(s.pictures.trickplaySheet(sheet, info.width, info.height, info.columns, info.rows))
        },
        route("GET", "/api/files/*/chapters/*", PLAY, free = true) { _, _, v ->
            val file = v[0].toLongOrNull()?.let(catalog::file) ?: return@route Reply.notFound("no such file")
            val n = v[1].removeSuffix(".jpg").toIntOrNull()?.takeIf { it in file.second.chapters.indices } ?: return@route Reply.notFound("no such chapter")
            Reply.picture(s.pictures.artwork(file.first.id * 10 + n, Pictures.Shape.Wide, 320))
        },
        route("POST", "/api/items/*/progress", PLAY) { call, c, v -> progress(call, c, v[0]) },
        route("POST", "/api/items/*/watched", PLAY) { _, c, v -> mark(c, v[0], watched = true) },
        route("POST", "/api/items/*/unwatched", PLAY) { _, c, v -> mark(c, v[0], watched = false) },
        route("PUT", "/api/items/*/streams", PLAY) { call, c, v -> streams(call, c, v[0]) },
        route("PUT", "/api/items/*/rating", PLAY) { call, c, v -> rating(call, c, v[0]) },
        route("DELETE", "/api/history/*", PLAY) { _, c, v ->
            val id = v[0].toLongOrNull()
            if (c.profile.history.removeAll { it.id == id }) Reply.noContent() else Reply.notFound("no such play")
        },
        route("PUT", "/api/me/track-defaults", PLAY) { call, c, _ -> trackDefaults(call, c) },
        route("PUT", "/api/me/language", PLAY) { call, c, _ -> language(call, c) },
    )

    // ---------------------------------------------------------------- sessions

    /**
     * POST /api/playback: direct when the caps play the file as it is
     * (container, codecs, dynamic range, the default audio or a player that
     * switches tracks itself, nothing burned in, bitrate), else HLS: remux
     * when the video plays, else transcode. Without `audio_index`, the
     * profile's audio ([audioFor]), which `audio_index` names.
     */
    private fun start(call: Call, caller: Caller): MockResponse {
        val request = call.body(PlaybackRequest.serializer()) ?: return Reply.badRequest("the body isn't a playback request")
        val item = catalog.item(request.itemId)?.takeIf { it.kind.playable } ?: return Reply.notFound("no such item")
        val file = request.fileId?.let { id -> item.files.firstOrNull { it.id == id } ?: return Reply.notFound("no such file") }
            ?: item.files.firstOrNull() ?: return Reply.notFound("the item has no file")
        val audio = file.streams.filter { it.kind == StreamKind.Audio }
        val defaultAudio = audio.firstOrNull { it.default } ?: audio.firstOrNull()
        if (request.audioIndex != null && audio.none { it.index == request.audioIndex }) return Reply.badRequest("no such audio track")
        val audioIndex = request.audioIndex ?: audioFor(item, file, caller.profile, request.caps)?.index
        val burn = request.subtitleIndex?.let { index -> file.streams.firstOrNull { it.index == index && it.kind == StreamKind.Subtitle && it.textBased == false } }

        val reasons = mutableListOf<String>()
        val caps = request.caps
        val switchesTracks = caps.capsVersion == ClientCaps.VERSION_2 && caps.audioTrackSwitching == true
        val video = file.streams.firstOrNull { it.kind == StreamKind.Video }
        val chosenAudio = audio.firstOrNull { it.index == audioIndex }
        val containerOk = file.container in caps.containers || (file.container == "mkv" && "matroska" in caps.containers)
        if (!containerOk) reasons += "The ${file.container?.uppercase()} container isn't supported by this player."
        val videoOk = video == null || (video.codec in caps.videoCodecs && rangePlays(video, caps.hdr, caps.dolbyVision))
        if (video != null && video.codec !in caps.videoCodecs) reasons += "${video.codec.uppercase()} video isn't supported by this player."
        if (video != null && video.codec in caps.videoCodecs && !rangePlays(video, caps.hdr, caps.dolbyVision)) {
            reasons += "${rangeName(video)} is converted to SDR for this screen."
        }
        val audioOk = chosenAudio == null || chosenAudio.codec in caps.audioCodecs
        if (!audioOk) reasons += "${chosenAudio.codec.uppercase()} audio isn't supported by this player."
        if (chosenAudio != null && chosenAudio != defaultAudio && !switchesTracks) reasons += "Another audio track than the file's default is played."
        if (burn != null) reasons += "Image subtitles are burned in."
        val bitrateOk = request.maxBitrate == null || (file.bitrate ?: 0L) <= request.maxBitrate!! * 1_000L
        if (!bitrateOk) reasons += "The file's bitrate is over the quality asked for."

        val method = when (request.mode) {
            PlaybackMode.Direct -> PlaybackMethod.Direct
            PlaybackMode.Remux -> PlaybackMethod.Remux
            PlaybackMode.Transcode -> PlaybackMethod.Transcode
            else -> when {
                reasons.isEmpty() -> PlaybackMethod.Direct
                videoOk && burn == null && bitrateOk -> PlaybackMethod.Remux
                else -> PlaybackMethod.Transcode
            }
        }
        if (method == PlaybackMethod.Direct) reasons.clear()
        // Starting with a client id ends that client's earlier sessions, unless it is
        // made ready beside the one playing (keep_previous): the player ends that one.
        request.clientId?.takeIf { request.keepPrevious != true }?.let { client ->
            s.sessions.values.removeAll { it.profileId == caller.profile.id && it.clientId == client }
        }
        val id = UUID.randomUUID().toString().replace("-", "")
        val session = PlaySession(
            id = id,
            profileId = caller.profile.id,
            itemId = item.id,
            fileId = file.id,
            method = method,
            clientId = request.clientId,
            controls = request.controls.orEmpty().filter { it != Control.Unknown },
            device = request.deviceName ?: call.header("User-Agent"),
            // Pley's Android app is one device per sign-in, and of a kind only when it says.
            deviceId = caller.grant?.let { "app-${it.id}" } ?: "session-$id",
            deviceKind = request.deviceKind?.takeIf { it != DeviceKind.Unknown } ?: DeviceKind.Other,
            positionMs = request.startMs ?: 0,
            reportedAtMs = s.nowMs(),
            // What it starts with: the audio picked or chosen, the subtitles burned in or those the profile's choices pick.
            audioIndex = audioIndex,
            subtitleIndex = burn?.index ?: s.views.withChoices(item, file, caller.profile).selectedSubtitle ?: PlayingSession.SUBTITLES_OFF,
        )
        s.sessions[session.id] = session
        s.events.emit(ServerEvent.SessionsChanged)
        val hls = method != PlaybackMethod.Direct
        val info = PlaybackInfo(
            sessionId = session.id,
            method = method,
            url = if (hls) "/api/playback/${session.id}/index.m3u8" else "/api/files/${file.id}/stream",
            hls = hls,
            fileId = file.id,
            durationMs = file.durationMs ?: 0,
            audioIndex = audioIndex,
            subtitleIndex = burn?.index,
            burnSubtitles = burn != null,
            audioConverted = hls && !audioOk,
            subtitles = file.streams.filter { it.kind == StreamKind.Subtitle }.map { stream ->
                SubtitleTrack(
                    index = stream.index,
                    codec = stream.codec,
                    language = stream.language,
                    title = stream.title,
                    default = stream.default,
                    forced = stream.forced,
                    external = stream.external,
                    textBased = stream.textBased == true,
                    url = if (stream.textBased == true) "/api/files/${file.id}/subtitles/${stream.index}.vtt" else null,
                )
            },
            reasons = reasons,
            reason = reasons.joinToString(" ").ifEmpty { null },
            autoQuality = if (method == PlaybackMethod.Transcode && request.maxBitrate == null) "8m" else null,
        )
        return Reply.json(PlaybackInfo.serializer(), info)
    }

    /**
     * The audio of a request that names none, as pley picks it (docs/API.md,
     * "The audio track"), in short: the profile's track for the item (its
     * choice, else its audio language's, else the file's default); when the
     * player can't play that one as it is, another of its language it can,
     * not a commentary.
     */
    private fun audioFor(item: ItemDetail, file: MediaFile, profile: Profile, caps: ClientCaps): MediaStream? {
        val audio = file.streams.filter { it.kind == StreamKind.Audio }
        val selected = s.views.withChoices(item, file, profile).selectedAudio
        val chosen = audio.firstOrNull { it.index == selected } ?: audio.firstOrNull { it.default } ?: audio.firstOrNull() ?: return null
        if (chosen.codec in caps.audioCodecs) return chosen
        return audio.firstOrNull { other ->
            other.language == chosen.language && other.codec in caps.audioCodecs && other.title?.contains("commentary", ignoreCase = true) != true
        } ?: chosen
    }

    private fun rangePlays(video: MediaStream, hdr: Boolean, dolbyVision: Boolean): Boolean = when (video.dynamicRange) {
        DynamicRange.Hdr10, DynamicRange.Hdr10Plus, DynamicRange.Hlg -> hdr
        DynamicRange.DolbyVision -> if (video.dvProfile == 5) dolbyVision else hdr || dolbyVision
        else -> true
    }

    private fun rangeName(video: MediaStream): String = when (video.dynamicRange) {
        DynamicRange.DolbyVision -> "Dolby Vision P${video.dvProfile}" + if (video.dvProfile == 8) ".${video.dvBlCompat}" else ""
        DynamicRange.Hdr10Plus -> "HDR10+"
        DynamicRange.Hlg -> "HLG"
        else -> "HDR10"
    }

    private fun status(session: PlaySession): SessionStatus {
        if (session.method == PlaybackMethod.Direct) return SessionStatus(session.id, session.method)
        val file = catalog.file(session.fileId)?.second
        val video = file?.streams?.firstOrNull { it.kind == StreamKind.Video }
        val copy = session.method == PlaybackMethod.Remux
        val segments = HLS_SEGMENTS
        return SessionStatus(
            sessionId = session.id,
            method = session.method,
            transcode = TranscodeInfo(
                jobId = "job-${session.id.take(8)}",
                state = JobState.Finished,
                video = if (copy) StreamAction.Copy else StreamAction.Transcode,
                audio = StreamAction.Transcode,
                videoIn = video?.let { "${it.codec.uppercase()} ${it.width}×${it.height}" },
                videoOut = if (copy) video?.let { "${it.codec.uppercase()} ${it.width}×${it.height}" } else "H.264 1920×1080",
                audioIn = file?.audioCodec?.uppercase(),
                audioOut = "AAC 2.0",
                encoder = if (copy) null else "libx264",
                preset = if (copy) null else "veryfast",
                decoder = "cpu",
                pipeline = if (copy) "copy" else "cpu",
                speed = 4.0,
                fps = 96.0,
                transcodedMs = Clip.Sdr1080.durationMs,
                segmentsReady = segments,
                segmentsTotal = segments,
                bitrateOut = 2_000_000,
            ),
        )
    }

    /**
     * DELETE /api/sessions/{id} with an optional `{ message }`: Alex (the
     * administrator) stops any session, others their own. The player hears
     * `session_stopped`; the session's stream answers 410 from then on.
     */
    private fun stopSession(call: Call, caller: Caller, id: String): MockResponse {
        val admin = caller.profile.id == FixtureIds.ALEX
        val session = s.sessions[id]
        if (session == null) {
            val stopped = s.stoppedSessions[id] ?: return Reply.notFound("no such session")
            return if (stopped.profileId == caller.profile.id || admin) Reply.noContent() else Reply.error(403, "forbidden", "not your session")
        }
        if (session.profileId != caller.profile.id && !admin) return Reply.error(403, "forbidden", "not your session")
        val message = call.body(StopBody.serializer())?.message?.trim()?.takeIf { it.isNotEmpty() }
        if (message != null && message.length > 200) return Reply.badRequest("the message is longer than 200 characters")
        s.sessions.remove(id)
        s.stoppedSessions[id] = StoppedSession(session.profileId, message)
        s.events.emitTo(session.profileId, ServerEvent.SessionStopped(id, message, byAdmin = session.profileId != caller.profile.id))
        s.events.emit(ServerEvent.SessionsChanged)
        return Reply.noContent()
    }

    /**
     * GET /api/sessions: the caller's own sessions (an administrator's
     * too), oldest first, as the server lists them. The fixtures keep no
     * live viewers here, and drop no player that went quiet.
     */
    private fun playing(caller: Caller): List<PlayingSession> {
        val now = s.nowMs()
        return s.sessions.values.filter { it.profileId == caller.profile.id }.map { session ->
            val item = catalog.item(session.itemId)
            PlayingSession(
                sessionId = session.id,
                kind = PlayingKind.Item,
                device = session.device,
                deviceId = session.deviceId,
                deviceKind = session.deviceKind,
                item = item?.toSummary(),
                state = session.state,
                positionMs = session.positionMs,
                durationMs = catalog.file(session.fileId)?.second?.durationMs ?: item?.durationMs,
                updatedAt = session.reportedAtMs / 1_000,
                ageMs = (now - session.reportedAtMs).coerceAtLeast(0),
                controls = session.controls,
                audioIndex = session.audioIndex,
                subtitleIndex = session.subtitleIndex,
                tracks = tracksOf(session.fileId),
                quality = session.quality,
                qualities = qualitiesOf(session.fileId),
            )
        }
    }

    /**
     * What a player's Quality menu offers for a file, as the server works
     * it out (apps/server/src/playback.rs, offered_qualities): Original and
     * Automatic, then the transcodes with fewer lines or at most 80 % of its
     * bitrate, once for each bitrate and lines they give it, by lines then
     * bitrate. Every one when neither is known; none for a file without video.
     */
    private fun qualitiesOf(fileId: Long): List<QualityOption> {
        val file = catalog.file(fileId)?.second ?: return emptyList()
        if (file.streams.none { it.kind == StreamKind.Video }) return emptyList()
        val w = file.width ?: 0
        val h = file.height ?: 0
        val lines = if (w <= 0 && h <= 0) null else when {
            w >= 3800 || h >= 2100 -> 2160
            w >= 1800 || h >= 1000 -> 1080
            w >= 1200 || h >= 700 -> 720
            else -> 480
        }
        val kbps = file.bitrate?.takeIf { it > 0 }?.let { it / 1000.0 }
        val height = file.height?.takeIf { it > 0 }
        val fixed = listOf(QualityOption("original", "Original"), QualityOption("auto", "Automatic"))
        if (kbps == null && height == null) return fixed + QUALITY_MENU.map { QualityOption(it.id, it.label) }
        val copies = mutableListOf<Triple<QualityOption, Int, Int>>()
        for (q in QUALITY_MENU) {
            val fewerLines = height != null && q.lines < height
            val lighter = kbps != null && q.kbps <= kbps * 0.8
            if (!fewerLines && !lighter) continue
            val shown = lines?.let { minOf(q.lines, it) } ?: q.lines
            if (copies.any { it.second == q.kbps && it.third == shown }) continue
            val label = if (lines != null && q.lines > lines) "${q.kbps / 1000} Mbps ${if (lines >= 2160) "4K" else "${lines}p"}" else q.label
            copies += Triple(QualityOption(q.id, label), q.kbps, shown)
        }
        return fixed + copies.sortedWith(compareByDescending<Triple<QualityOption, Int, Int>> { it.third }.thenByDescending { it.second }).map { it.first }
    }

    /** A file's audio and subtitles (added ones too), as sessions list them for the remote to pick from. */
    private fun tracksOf(fileId: Long): SessionTracks {
        val streams = catalog.file(fileId)?.second?.streams.orEmpty()
        fun option(stream: MediaStream) = TrackOption(
            index = stream.index,
            language = stream.language,
            title = stream.title,
            codec = stream.codec,
            channels = stream.channels,
            default = stream.default,
            forced = stream.forced,
        )
        val added = s.addedSubtitles[fileId].orEmpty().map { TrackOption(it.index, it.language, it.title, it.codec, forced = it.forced) }
        return SessionTracks(
            audio = streams.filter { it.kind == StreamKind.Audio }.map(::option),
            subtitles = streams.filter { it.kind == StreamKind.Subtitle }.map(::option) + added,
        )
    }

    /**
     * POST /api/sessions/{id}/command: the caller's own session only (an
     * administrator stops others', no more), a command its player listed,
     * a seek within its length, a skip of up to 10 minutes, audio or
     * subtitles of the session's file (`-1`: subtitles off), and one of the
     * qualities its menu offers. The player's streams hear `player_command`.
     */
    private fun command(call: Call, caller: Caller, id: String): MockResponse {
        val body = call.body(PlayerCommandRequest.serializer()) ?: return Reply.badRequest("the body isn't a command")
        val session = s.sessions[id] ?: return Reply.notFound("no such session")
        if (session.profileId != caller.profile.id) return Reply.error(403, "forbidden", "not your session")
        val length = catalog.file(session.fileId)?.second?.durationMs ?: DAY_MS
        val tracks = tracksOf(session.fileId)
        when (body.command) {
            Control.Unknown -> return Reply.badRequest("unknown command")
            Control.Seek -> if (body.positionMs == null || body.positionMs !in 0..length) return Reply.badRequest("position_ms is out of range")
            Control.Skip -> if (body.offsetMs == null || body.offsetMs !in -SKIP_MS..SKIP_MS) return Reply.badRequest("offset_ms is out of range")
            Control.Audio -> if (tracks.audio.none { it.index == body.streamIndex }) return Reply.badRequest("stream_index must be one of the file's audio tracks")
            Control.Subtitles -> if (body.streamIndex != PlayingSession.SUBTITLES_OFF && tracks.subtitles.none { it.index == body.streamIndex }) {
                return Reply.badRequest("stream_index must be one of the file's subtitle tracks, or -1 for none")
            }
            Control.Quality -> if (qualitiesOf(session.fileId).none { it.id == body.quality }) {
                return Reply.badRequest("quality must be one of the session's qualities")
            }
            else -> Unit
        }
        if (body.command !in session.controls) {
            return Reply.error(409, "not_controllable", "this player doesn't take ${body.command.wire} from a remote")
        }
        val event = ServerEvent.PlayerCommand(
            id,
            body.command,
            body.positionMs.takeIf { body.command == Control.Seek },
            body.offsetMs.takeIf { body.command == Control.Skip },
            body.streamIndex.takeIf { body.command == Control.Audio || body.command == Control.Subtitles },
            body.quality.takeIf { body.command == Control.Quality },
        )
        s.events.emitTo(session.profileId, event)
        return MockResponse.Builder().code(202).build()
    }

    /** The body of DELETE /api/sessions/{id}. */
    @Serializable
    private data class StopBody(val message: String? = null)

    /**
     * POST /api/remote/devices/{id}/cast, refused in the server's order:
     * a receiver that isn't the caller's own open one (404), not exactly one
     * of a title, a playlist and a channel (400), a title there isn't (404)
     * or of a kind that isn't cast (400), a playlist not the caller's (404),
     * a channel (404: there is no Live TV here), what the receiver doesn't
     * play (409 `not_castable`), a `start_ms` past a day or tracks that
     * aren't one of a movie's or an episode's files' (400), and more than 10
     * casts a second (429). The caller's streams hear `cast`, from the
     * caller's User-Agent.
     */
    private fun cast(call: Call, caller: Caller, receiverId: String): MockResponse {
        val body = call.body(CastRequest.serializer()) ?: return Reply.badRequest("the body isn't a cast")
        val takes = s.events.casts(caller.profile.id, receiverId) ?: return Reply.notFound("no such device")
        val itemId = body.itemId
        val playlistId = body.playlistId
        val picked = body.audioIndex != null || body.subtitleIndex != null
        var streams: List<MediaStream>? = null
        val kind = when {
            listOfNotNull(itemId, playlistId, body.channelId).size != 1 -> return Reply.badRequest("name one of item_id, playlist_id and channel_id")
            itemId != null -> {
                val item = catalog.item(itemId) ?: return Reply.notFound("no such item")
                if (item.kind !in CASTABLE) return Reply.badRequest("only movies, episodes, shows, seasons, tracks, albums and artists can be cast")
                if (picked && item.kind !in WITH_TRACKS) return Reply.badRequest("audio_index and subtitle_index go with a movie or an episode")
                streams = item.files.flatMap { f -> catalog.file(f.id)?.second?.streams.orEmpty() }
                CastKind.Item
            }
            picked -> return Reply.badRequest("audio_index and subtitle_index go with item_id")
            playlistId != null -> {
                s.household.playlists[playlistId]?.takeIf { it.owner == caller.profile.id } ?: return Reply.notFound("playlist $playlistId not found")
                CastKind.Item
            }
            else -> return Reply.notFound("no such channel")
        }
        if (kind !in takes) return Reply.error(409, "not_castable", "this device doesn't play titles from a remote")
        if (body.startMs?.let { it !in 0..DAY_MS } == true) return Reply.badRequest("start_ms must be between 0 and $DAY_MS")
        val tracks = streams.orEmpty()
        if (body.audioIndex?.let { index -> tracks.none { it.kind == StreamKind.Audio && it.index == index } } == true) {
            return Reply.badRequest("audio_index must be one of the file's audio tracks")
        }
        val subtitle = body.subtitleIndex
        if (subtitle != null && subtitle != PlayingSession.SUBTITLES_OFF && tracks.none { it.kind == StreamKind.Subtitle && it.index == subtitle }) {
            return Reply.badRequest("subtitle_index must be one of the file's subtitle tracks, or -1 for none")
        }
        if (!s.events.takeCast(caller.profile.id, receiverId, s.nowMs())) return Reply.error(429, "too_many_requests", "10 casts a second at most: wait a moment")
        val from = call.header("User-Agent")?.trim()?.take(DEVICE_NAME_MAX)?.ifEmpty { null }
        s.events.emitTo(
            caller.profile.id,
            ServerEvent.Cast(
                receiverId,
                itemId,
                playlistId,
                body.channelId,
                body.startMs,
                shuffle = body.shuffle == true,
                audioIndex = body.audioIndex,
                subtitleIndex = body.subtitleIndex,
                from = from,
            ),
        )
        return MockResponse.Builder().code(202).build()
    }

    /** GET /api/playback/{session}/{file}: the committed HLS stream of the 1080p clip, whatever the item. */
    private fun hls(sessionId: String, name: String, caller: Caller): MockResponse {
        s.stoppedSessions[sessionId]?.takeIf { it.profileId == caller.profile.id }?.let { stopped ->
            val body = JsonObject(mapOf("error" to JsonPrimitive("session_stopped"), "message" to (stopped.message?.let(::JsonPrimitive) ?: JsonNull)))
            return Reply.json(body.toString(), code = 410)
        }
        s.sessions[sessionId]?.takeIf { it.profileId == caller.profile.id && it.method != PlaybackMethod.Direct }
            ?: return Reply.notFound("playback session expired")
        val type = when {
            name == "index.m3u8" -> "application/vnd.apple.mpegurl"
            name.endsWith(".ts") && name.removeSuffix(".ts").toIntOrNull() != null -> "video/mp2t"
            else -> return Reply.notFound("no such file")
        }
        val bytes = Media.bytes("hls/$name") ?: return Reply.notFound("no such segment")
        return Reply.bytes(bytes, type, headers = mapOf("Cache-Control" to "no-cache"))
    }

    // ---------------------------------------------------------------- files

    /** GET /api/files/{id}/stream: the clip the file streams, with Range. */
    private fun stream(call: Call, id: String): MockResponse {
        val entry = id.toLongOrNull()?.let(catalog::file)?.first ?: return Reply.notFound("no such file")
        val bytes = Media.bytes(entry.clip.resource) ?: return Reply.notFound("no such file")
        val size = bytes.size.toLong()
        val headers = mapOf("Accept-Ranges" to "bytes")
        val range = call.header("Range") ?: return Reply.bytes(bytes, entry.clip.contentType, headers = headers)
        val span = Ranges.parse(range, size)
            ?: return MockResponse.Builder().code(416).setHeader("Content-Range", "bytes */$size").build()
        return Reply.bytes(
            bytes,
            span.first.toInt(),
            (span.last - span.first + 1).toInt(),
            entry.clip.contentType,
            206,
            headers + ("Content-Range" to "bytes ${span.first}-${span.last}/$size"),
        )
    }

    /**
     * GET /api/files/{id}/subtitles/{index}.vtt. The HDR10 sample's embedded
     * track comes partial (`X-Pley-Complete: 0`) the first time it is asked
     * for, as a track still being read out of its file, and whole after.
     */
    private fun subtitles(id: String, name: String): MockResponse {
        val (entry, file) = id.toLongOrNull()?.let(catalog::file) ?: return Reply.notFound("no such file")
        if (name.endsWith(".mks")) return pictures(file, name)
        val index = name.removeSuffix(".vtt").toIntOrNull() ?: return Reply.notFound("no such track")
        s.addedSubtitles[file.id]?.firstOrNull { it.index == index }?.let { added ->
            return Reply.bytes(ExtraRoutes.cues(added).toByteArray(), "text/vtt; charset=utf-8")
        }
        val stream = file.streams.firstOrNull { it.index == index && it.kind == StreamKind.Subtitle }
            ?: return Reply.notFound("no such track")
        if (stream.textBased != true) return Reply.badRequest("subtitles shown as pictures are burned in")
        val cues = cues(file, stream)
        val key = "${entry.id}:$index"
        val asked = s.subtitleAsks.merge(key, 1, Int::plus)!!
        val partial = entry.id == FixtureIds.SAMPLE_HDR10_FILE && index == FixtureIds.SAMPLE_HDR10_SUBTITLE && asked == 1
        val text = vtt(if (partial) cues.take(cues.size / 2) else cues)
        return if (partial) {
            Reply.bytes(text.toByteArray(), "text/vtt; charset=utf-8", headers = mapOf("X-Pley-Complete" to "0", "Cache-Control" to "no-store"))
        } else {
            Reply.bytes(text.toByteArray(), "text/vtt; charset=utf-8", headers = mapOf("Cache-Control" to "private, max-age=3600"))
        }
    }

    /**
     * GET /api/files/{id}/subtitles/{index}.mks: a track shown as pictures,
     * alone in a Matroska file (PGS made here): a cue every 6 s, "PGS 1",
     * "PGS 2"…, at the file's own times. 404 for text.
     */
    private fun pictures(file: MediaFile, name: String): MockResponse {
        val index = name.removeSuffix(".mks").toIntOrNull() ?: return Reply.notFound("no such track")
        val stream = file.streams.firstOrNull { it.index == index && it.kind == StreamKind.Subtitle && it.textBased == false }
            ?: return Reply.notFound("no such picture subtitle track")
        val duration = file.durationMs ?: 0
        val cues = (0 until (duration / PICTURE_EVERY_MS).toInt().coerceIn(1, 400)).map { n ->
            Triple(n * PICTURE_EVERY_MS + 500, n * PICTURE_EVERY_MS + 4_500, "PGS ${n + 1}")
        }
        val bytes = PgsMks.build(cues, duration, stream.language)
        return Reply.bytes(bytes, "video/x-matroska", headers = mapOf("Cache-Control" to "private, max-age=3600"))
    }

    /** A track's cues: the sample's own, else one every 30 s. */
    private fun cues(file: MediaFile, stream: MediaStream): List<Triple<Long, Long, String>> {
        if (file.id == FixtureIds.SAMPLE_HDR10_FILE) {
            val french = stream.language == "fre"
            return listOf(
                Triple(1_000L, 4_000L, if (french) "Le serveur de test joue ceci." else "The fixture server plays this."),
                Triple(6_000L, 9_000L, if (french) "Rien ici n'est un film :\njuste une mire qui gigote." else "Nothing here is a film:\njust a glossy, jumpy pattern."),
                Triple(
                    11_000L,
                    14_000L,
                    if (french) "Mire et tonalité seulement,\nquelques gags typographiques,\npuis le générique." else "Test pattern and tone only,\nplaying quietly, gently, joyfully,\nlong enough to judge the edges.",
                ),
                Triple(16_000L, 19_000L, if (french) "Le générique de fin." else "The end credits."),
            )
        }
        val duration = file.durationMs ?: 0
        // One, two or three lines in turn, with descenders: what a film's subtitles look like at the bottom.
        val more = listOf("", "\nquietly playing a gentle, jumpy pattern", "\nquietly playing a gentle, jumpy pattern\nlong enough to judge the edges")
        return (0 until (duration / 30_000).toInt().coerceIn(1, 400)).map { n ->
            Triple(n * 30_000L + 1_000, n * 30_000L + 4_000, "Line ${n + 1} (${stream.language ?: "und"})" + more[n % 3])
        }
    }

    private fun vtt(cues: List<Triple<Long, Long, String>>): String = buildString {
        append("WEBVTT\n\n")
        for ((start, end, text) in cues) {
            append(time(start)).append(" --> ").append(time(end)).append('\n').append(text).append("\n\n")
        }
    }

    private fun time(ms: Long) = "%02d:%02d:%02d.%03d".format(ms / 3_600_000, ms / 60_000 % 60, ms / 1_000 % 60, ms % 1_000)

    /** A preview every second for the 20 s clips, every 10 s for the rest. */
    private fun trickplay(file: MediaFile): TrickplayInfo {
        val duration = file.durationMs ?: 0
        val interval = if (duration <= 60_000) 1_000L else 10_000L
        val count = (duration / interval).toInt().coerceAtLeast(1)
        val perSheet = 10 * 10
        return TrickplayInfo(Catalog.trickplayId(file.id), interval, 320, 180, 10, 10, count, (count + perSheet - 1) / perSheet)
    }

    // ---------------------------------------------------------------- writes

    private fun playable(id: String): ItemDetail? = id.toLongOrNull()?.let(catalog::item)

    /** POST /api/items/{id}/progress: at 90 % the item is watched, its position cleared and the play recorded. */
    private fun progress(call: Call, caller: Caller, id: String): MockResponse {
        val item = playable(id)?.takeIf { it.kind.playable } ?: return Reply.notFound("no such item")
        val report = call.body(ProgressReport.serializer()) ?: return Reply.badRequest("the body isn't a progress report")
        val duration = report.durationMs ?: item.durationMs ?: 0
        val now = s.now()
        report.sessionId?.let { s.sessions[it] }?.takeIf { it.profileId == caller.profile.id }?.let { session ->
            session.positionMs = report.positionMs
            if (report.state != ProgressState.Stopped) session.state = report.state
            session.reportedAtMs = s.nowMs()
            // The tracks it says it plays; one left out, or not of its file, keeps what was known.
            val tracks = tracksOf(session.fileId)
            report.audioIndex?.takeIf { index -> tracks.audio.any { it.index == index } }?.let { session.audioIndex = it }
            report.subtitleIndex?.takeIf { index -> index == PlayingSession.SUBTITLES_OFF || tracks.subtitles.any { it.index == index } }
                ?.let { session.subtitleIndex = it }
            report.quality?.takeIf { id -> qualitiesOf(session.fileId).any { it.id == id } }?.let { session.quality = it }
        }
        val done = duration > 0 && report.positionMs >= duration * 9 / 10
        val before = caller.profile.state(item.id)
        // A play counts once: reports after the 90 % mark find it watched with no position.
        val counts = done && (before.positionMs > 0 || !before.watched)
        caller.profile.update(item.id) { state ->
            if (done) {
                state.copy(positionMs = 0, watched = true, playCount = state.playCount + if (counts) 1 else 0, lastPlayedAt = now)
            } else {
                state.copy(positionMs = report.positionMs.coerceAtLeast(0), lastPlayedAt = now)
            }
        }
        // The profile's devices hear of what changes what they show
        // (`user_state_changed`): watched, a resume point started or
        // cleared, or moved as the player pauses or stops; not every report.
        val after = caller.profile.state(item.id)
        val settled = report.state == ProgressState.Paused || report.state == ProgressState.Stopped
        if (after.watched != before.watched || (after.positionMs > 0) != (before.positionMs > 0) || (settled && after.positionMs != before.positionMs)) {
            s.events.emitTo(caller.profile.id, ServerEvent.UserStateChanged(lineage(item)))
        }
        if (counts) {
            s.household.addHistory(caller.profile, item.id, now, call.header("User-Agent"))
            s.events.emit(ServerEvent.LibraryChanged(item.libraryId))
        }
        return Reply.noContent()
    }

    /** Watched or unwatched by hand (containers: all their episodes or tracks). Not a play. */
    private fun mark(caller: Caller, id: String, watched: Boolean): MockResponse {
        val item = playable(id) ?: return Reply.notFound("no such item")
        val leaves = catalog.leaves(item.id)
        val changes = leaves.any { leaf -> caller.profile.state(leaf).let { it.watched != watched || it.positionMs > 0 } }
        for (leaf in leaves) {
            caller.profile.update(leaf) { it.copy(watched = watched, positionMs = 0) }
        }
        if (changes) {
            val children = item.kind in setOf(ItemKind.Show, ItemKind.Season, ItemKind.Album, ItemKind.Artist)
            s.events.emitTo(caller.profile.id, ServerEvent.UserStateChanged(lineage(item), children))
        }
        s.events.emit(ServerEvent.LibraryChanged(item.libraryId))
        return Reply.noContent()
    }

    /** [item], then its parent and grandparent: what `user_state_changed` names. */
    private fun lineage(item: ItemDetail): List<Long> = listOfNotNull(item.id, item.parentId, item.grandparentId)

    /**
     * PUT /api/items/{id}/streams: remembered for the movie, or for the
     * whole show. Without `audio_index` the audio chosen before stays:
     * subtitles alone were picked.
     */
    private fun streams(call: Call, caller: Caller, id: String): MockResponse {
        val item = playable(id) ?: return Reply.notFound("no such item")
        val choice = call.body(StreamChoice.serializer()) ?: return Reply.badRequest("the body isn't a track choice")
        val owner = if (item.kind == ItemKind.Episode) item.grandparentId ?: item.id else item.id
        val audioNamed = call.body(JsonObject.serializer())?.containsKey("audio_index") == true
        val before = caller.profile.streamChoices[owner]
        caller.profile.streamChoices[owner] = if (audioNamed) choice else choice.copy(audioIndex = before?.audioIndex)
        caller.profile.version++
        return Reply.noContent()
    }

    private fun rating(call: Call, caller: Caller, id: String): MockResponse {
        val item = playable(id) ?: return Reply.notFound("no such item")
        val request = call.body(RatingRequest.serializer()) ?: return Reply.badRequest("the body isn't a rating")
        val rating = request.rating?.let { if (it < 0 || it > 10) return Reply.badRequest("a rating is 0 to 10") else Math.round(it * 2) / 2.0 }
        caller.profile.update(item.id) { it.copy(rating = rating) }
        return Reply.noContent()
    }

    /** PUT /api/me/track-defaults (S3-APP-ROUTES): languages of 2 or 3 lower-case letters; answers the profile. */
    private fun trackDefaults(call: Call, caller: Caller): MockResponse {
        val defaults = call.body(TrackDefaults.serializer()) ?: return Reply.badRequest("the body isn't track defaults")
        val language = Regex("[a-z]{2,3}")
        if (listOfNotNull(defaults.audioLanguage, defaults.subtitleLanguage).any { !language.matches(it) }) {
            return Reply.badRequest("a language is 2 or 3 lower-case letters")
        }
        if (defaults.subtitleMode == SubtitleMode.Unknown) return Reply.badRequest("subtitle_mode is off, forced, foreign or always")
        caller.profile.trackDefaults = defaults
        caller.profile.version++
        return Reply.json(Me.serializer(), s.views.me(caller.profile, caller.scope))
    }

    /** PUT /api/me/language: "en", "fr", "es" or `null` (the server's); answers the profile. */
    private fun language(call: Call, caller: Caller): MockResponse {
        val choice = call.body(LanguageChoice.serializer()) ?: return Reply.badRequest("the body isn't a language")
        if (choice.language != null && choice.language !in LANGUAGES) return Reply.badRequest("language is one of en, fr, es, or null")
        caller.profile.language = choice.language
        caller.profile.version++
        return Reply.json(Me.serializer(), s.views.me(caller.profile, caller.scope))
    }

    companion object {
        /** The languages Pley is in. */
        private val LANGUAGES = setOf("en", "fr", "es")

        /** Segments of the committed HLS stream (20 s in 4 s segments). */
        const val HLS_SEGMENTS = 5

        /** How often the .mks of picture subtitles has a cue. */
        const val PICTURE_EVERY_MS = 6_000L

        /** A seek's limit when a title's length isn't known. */
        const val DAY_MS = 24 * 60 * 60 * 1_000L

        /** How far a skip goes at most, either way. */
        const val SKIP_MS = 10 * 60 * 1_000L

        /** What the server keeps of a device's name. */
        const val DEVICE_NAME_MAX = 64

        /** A Quality menu's transcodes, in its order: id, label, kbps and lines (apps/server/src/playback.rs, MENU). */
        private class MenuEntry(val id: String, val label: String, val kbps: Int, val lines: Int)

        private val QUALITY_MENU = listOf(
            MenuEntry("30m", "30 Mbps 4K", 30_000, 2160),
            MenuEntry("15m", "15 Mbps 4K", 15_000, 2160),
            MenuEntry("20m", "20 Mbps 1080p", 20_000, 1080),
            MenuEntry("12m", "12 Mbps 1080p", 12_000, 1080),
            MenuEntry("8m", "8 Mbps 1080p", 8_000, 1080),
            MenuEntry("4m", "4 Mbps 720p", 4_000, 720),
            MenuEntry("2m", "2 Mbps 480p", 2_000, 480),
        )

        /** What a remote casts with tracks: a title of one file's worth. */
        private val WITH_TRACKS = setOf(ItemKind.Movie, ItemKind.Episode)

        /** What a remote casts from the library: what plays itself, a show or a season, an album or an artist. */
        private val CASTABLE = setOf(ItemKind.Movie, ItemKind.Episode, ItemKind.Show, ItemKind.Season, ItemKind.Track, ItemKind.Album, ItemKind.Artist)
    }
}

/** `Range: bytes=…` (one range; RFC 9110 §14.1.2). */
internal object Ranges {
    /** The first and last byte asked for in [header], within [size]; `null` when it can't be served. */
    fun parse(header: String, size: Long): LongRange? {
        if (!header.startsWith("bytes=") || ',' in header || size == 0L) return null
        val spec = header.removePrefix("bytes=").trim()
        val start = spec.substringBefore('-')
        val end = spec.substringAfter('-', "")
        return when {
            start.isEmpty() -> {
                val suffix = end.toLongOrNull()?.takeIf { it > 0 } ?: return null
                (size - suffix).coerceAtLeast(0)..(size - 1)
            }
            else -> {
                val first = start.toLongOrNull() ?: return null
                val last = if (end.isEmpty()) size - 1 else (end.toLongOrNull() ?: return null).coerceAtMost(size - 1)
                if (first >= size || last < first) null else first..last
            }
        }
    }
}
