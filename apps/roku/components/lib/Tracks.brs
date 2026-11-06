' Audio and subtitle tracks, as the Samsung and LG app has them: the
' profile's defaults (GET /api/auth/me, PUT /api/me/track-defaults), which
' Settings shows and changes, and the tracks the person picks while a title
' plays, remembered for the whole show or the movie (PUT
' /api/items/{id}/streams, docs/API.md), so the next episode starts with
' matching ones. Indexes are the file's stream indexes (sidecar subtitle
' files from 1000).

' When subtitles come on by default (subtitle_mode), in order.
function tracksModeIds() as object
    return ["off", "forced", "foreign", "always"]
end function

' The modes worded as on the TVs: [{ id, label, hint }].
function tracksModes() as object
    modes = []
    for each id in tracksModeIds()
        key = "player.settings.modes." + id
        modes.Push({ id: id, label: t(key + ".label"), hint: t(key + ".hint") })
    end for
    return modes
end function

' A profile's defaults (GET /api/auth/me) as { audio, subtitle, mode }: ""
' for a language left to the file (audio) or to the audio (subtitles); the
' server's default mode, "forced", for one this app doesn't know.
function tracksDefaults(me as dynamic) as object
    defaults = { audio: "", subtitle: "", mode: "forced" }
    data = httpObject(me)
    if data = invalid then return defaults
    defaults.audio = httpString(data.audio_language)
    defaults.subtitle = httpString(data.subtitle_language)
    mode = httpString(data.subtitle_mode)
    for each known in tracksModeIds()
        if known = mode then defaults.mode = mode
    end for
    return defaults
end function

' defaults with one of them (kind: "audio", "mode" or "subtitle") changed.
function tracksWith(defaults as object, kind as string, value as string) as object
    changed = { audio: defaults.audio, subtitle: defaults.subtitle, mode: defaults.mode }
    changed[kind] = value
    return changed
end function

function tracksSame(a as object, b as object) as boolean
    return a.audio = b.audio and a.subtitle = b.subtitle and a.mode = b.mode
end function

' The body of PUT /api/me/track-defaults: all three at once, null for a
' language left to the file or the audio.
function tracksDefaultsBody(defaults as object) as object
    body = { audio_language: invalid, subtitle_language: invalid, subtitle_mode: defaults.mode }
    if defaults.audio <> "" then body.audio_language = defaults.audio
    if defaults.subtitle <> "" then body.subtitle_language = defaults.subtitle
    return body
end function

' Whether a refusal to save the defaults means this server or this sign-in
' doesn't let apps change them (they are changed on the web then).
function tracksRefused(status as integer) as boolean
    return status = 403 or status = 404 or status = 405
end function

function tracksModeLabel(mode as string) as string
    for each known in tracksModeIds()
        if known = mode then return t("player.settings.modes." + mode + ".label")
    end for
    return ""
end function

' The subtitle language is asked for only where it is used.
function tracksAsksSubtitleLanguage(mode as string) as boolean
    return mode = "foreign" or mode = "always"
end function

' Settings' rows for the defaults, in order: [{ kind, text }]. defaults is
' invalid until the profile came; readOnly once the server refused a change.
' A row says its value in fewer words than the choices do where a language
' needs them to fit (settings.rows, the modes' row).
function tracksRows(defaults as dynamic, readOnly as boolean) as object
    if defaults = invalid
        return [{ kind: "audio", text: t("player.settings.audioLanguage") }, { kind: "mode", text: t("player.settings.subtitles") }]
    end if
    mode = ""
    if tracksModeLabel(defaults.mode) <> "" then mode = t("player.settings.modes." + defaults.mode + ".row")
    rows = [
        { kind: "audio", text: t("player.settings.rows.audio", { language: fmtLanguageName(defaults.audio, t("player.settings.rows.fileDefault")) }) },
        { kind: "mode", text: t("player.settings.rows.mode", { mode: mode }) }
    ]
    if tracksAsksSubtitleLanguage(defaults.mode)
        rows.Push({ kind: "subtitle", text: t("player.settings.rows.subtitle", { language: fmtLanguageName(defaults.subtitle, t("player.settings.rows.sameAsAudio")) }) })
    end if
    if readOnly
        for each row in rows
            row.text = t("player.settings.rows.readOnly", { row: row.text })
        end for
    end if
    return rows
end function

' What one row offers: { title, labels, values, hints, current } (current:
' the index of the value now, -1 for a language not offered).
function tracksChoices(kind as string, defaults as object) as object
    labels = []
    values = []
    hints = []
    if kind = "mode"
        title = t("player.settings.subtitles")
        for each known in tracksModes()
            labels.Push(known.label)
            values.Push(known.id)
            hints.Push(known.hint)
        end for
    else
        title = t("player.settings.audioLanguage")
        none = t("player.settings.fileDefault")
        if kind = "subtitle"
            title = t("player.settings.subtitleLanguage")
            none = t("player.settings.sameAsAudio")
        end if
        labels.Push(none)
        values.Push("")
        for each language in fmtLanguages()
            labels.Push(language[1])
            values.Push(language[0])
        end for
    end if
    current = -1
    for i = 0 to values.Count() - 1
        if values[i] = defaults[kind] then current = i
    end for
    return { title: title, labels: labels, values: values, hints: hints, current: current }
end function

' --- While a title plays ---

' What the Roku shows as a stream starts is pley's choice, or its own: the
' tracks the person picks are those changed once the video has played this
' long.
function tracksSettleMs() as integer
    return 2000
end function

' The audio stream a playback plays: the session's audio_index, else the
' file's selected_audio, else its default audio stream (the first when none
' is marked); -1 for a file without audio.
function tracksAudioPlaying(info as dynamic, file as dynamic) as integer
    session = httpObject(info)
    if session <> invalid
        index = httpNumber(session.audio_index, -1)
        if index >= 0 then return Int(index)
    end if
    data = httpObject(file)
    if data = invalid then return -1
    selected = httpNumber(data.selected_audio, -1)
    if selected >= 0 then return Int(selected)
    first = -1
    for each stream in tracksAudioStreams(data)
        if httpTrue(stream.default) then return Int(httpNumber(stream.index, -1))
        if first < 0 then first = Int(httpNumber(stream.index, -1))
    end for
    return first
end function

' The file's audio streams, in the file's order.
function tracksAudioStreams(file as object) as object
    found = []
    for each stream in httpArray(file.streams)
        data = httpObject(stream)
        if data <> invalid and httpString(data.kind) = "audio" and httpNumber(data.index, -1) >= 0 then found.Push(data)
    end for
    return found
end function

' The subtitles a Roku shows, as the file's stream index: captionMode is the
' Video node's globalCaptionMode, track its currentSubtitleTrack (moved by
' subtitle sync or not), subtitles the PlaybackInfo's (index, url) and
' server the address their paths are under. -1: none shown (captions off, or
' no track); -2: can't be told (captions only on replay or on mute, or a
' track that isn't one of pley's, such as captions in the video itself).
function tracksSubtitleShown(captionMode as string, track as string, subtitles as dynamic, server as string) as integer
    mode = LCase(captionMode)
    if mode = "off" then return -1
    if mode <> "on" then return -2
    shown = syncBaseUrl(track)
    if shown = "" then return -1
    for each entry in httpArray(subtitles)
        data = httpObject(entry)
        if data <> invalid
            path = httpString(data.url)
            index = httpNumber(data.index, -1)
            if path <> "" and index >= 0
                url = path
                if Left(path, 1) = "/" then url = server + path
                if shown = url or (Left(path, 1) = "/" and Len(shown) > Len(path) and Right(shown, Len(path)) = path) then return Int(index)
            end if
        end if
    end for
    return -2
end function

' The audio a Roku plays from a file played as it is, as the file's stream
' index: available is the Video node's availableAudioTracks ([{ Language,
' Name, Track }]), current its currentAudioTrack. The tracks Roku lists are
' taken to be the file's audio streams in the file's order, and only when
' Roku lists as many as the file has and their languages agree wherever both
' have one; -2 when that can't be told.
function tracksAudioShown(available as dynamic, current as string, file as dynamic) as integer
    data = httpObject(file)
    if data = invalid or current = "" then return -2
    tracks = httpArray(available)
    streams = tracksAudioStreams(data)
    if tracks.Count() = 0 or tracks.Count() <> streams.Count() then return -2
    at = -1
    for i = 0 to tracks.Count() - 1
        track = httpObject(tracks[i])
        if track = invalid then return -2
        if not tracksSameLanguage(httpString(track.Language), httpString(streams[i].language)) then return -2
        if tracksTrackId(track.Track) = current then at = i
    end for
    if at < 0 then return -2
    return Int(httpNumber(streams[at].index, -2))
end function

' A Roku track's Track as text (a string, or a number on some models).
function tracksTrackId(value as dynamic) as string
    if GetInterface(value, "ifString") <> invalid then return value
    return urlValue(value)
end function

' Whether two language tags can be the same: equal, or one of them unknown.
function tracksSameLanguage(a as string, b as string) as boolean
    x = LCase(a)
    y = LCase(b)
    if x = "" or y = "" or x = "und" or y = "und" then return true
    return x = y
end function

' The body of PUT /api/items/{id}/streams: the file and the tracks it
' plays, null for its default audio (-1) and for subtitles off (-1). The
' audio left out (-2) keeps the one picked before: subtitles changed alone
' don't pin the track the server chose for this Roku on other devices.
function tracksStreamsBody(fileId as dynamic, audioIndex as integer, subtitleIndex as integer) as object
    body = { subtitle_index: invalid }
    if audioIndex <> -2 then body.audio_index = invalid
    file = httpNumber(fileId, 0)
    if file > 0 then body.file_id = file
    if audioIndex >= 0 then body.audio_index = audioIndex
    if subtitleIndex >= 0 then body.subtitle_index = subtitleIndex
    return body
end function

' The stream indexes of a file's audio tracks and of its subtitles, in the
' file's order, as Pley's menu lists them: { audio, subtitles }, what the
' user's remote picks from.
function tracksIndexes(file as dynamic) as object
    out = { audio: [], subtitles: [] }
    data = httpObject(file)
    if data = invalid then return out
    for each stream in httpArray(data.streams)
        s = httpObject(stream)
        if s <> invalid and httpNumber(s.index, -1) >= 0
            kind = httpString(s.kind)
            if kind = "audio" then out.audio.Push(Int(s.index))
            if kind = "subtitle" then out.subtitles.Push(Int(s.index))
        end if
    end for
    return out
end function

' The subtitles a playback shows, as its reports say them: those the
' server burns in (burned; -1 for none) while no others show, else those
' shown (shown: pictures drawn here or a text track; -1 none, -2 not
' known).
function tracksSubtitlePlaying(shown as integer, burned as integer) as integer
    if burned >= 0 and shown < 0 then return burned
    return shown
end function

' Whether the person's pick of subtitleIndex needs another session: the
' server draws picture subtitles onto the video (burnedIndex, -1 for none),
' and they are other subtitles now.
function tracksNeedNewSession(burnedIndex as integer, subtitleIndex as integer) as boolean
    return burnedIndex >= 0 and subtitleIndex <> burnedIndex
end function
