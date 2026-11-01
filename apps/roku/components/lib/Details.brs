' What the player's menu says of the playing file, as the other apps' do
' (packages/tv/src/player/details.ts, the Android app's PlaybackDetails):
' how it plays and why, its video and audio as they are and as the server
' converts them; and its chapters.

' ffprobe's codec name as people know it: "h264" is "H.264"; others in
' capitals.
function detCodec(codec as dynamic) as string
    names = {
        h264: "H.264", hevc: "HEVC", h265: "HEVC", av1: "AV1", vp9: "VP9", vp8: "VP8", mpeg4: "MPEG-4", mpeg2video: "MPEG-2",
        mpeg1video: "MPEG-1", vc1: "VC-1", wmv3: "WMV", msmpeg4v3: "DivX", theora: "Theora", prores: "ProRes", aac: "AAC",
        ac3: "AC3", eac3: "EAC3", dts: "DTS", truehd: "TrueHD", mlp: "MLP", flac: "FLAC", alac: "ALAC", mp3: "MP3", mp2: "MP2",
        opus: "Opus", vorbis: "Vorbis", wmav2: "WMA", pcm_s16le: "PCM", pcm_s24le: "PCM", pcm_s32le: "PCM", pcm_f32le: "PCM",
        subrip: "SRT", srt: "SRT", ass: "ASS", ssa: "SSA", webvtt: "WebVTT", mov_text: "MOV Text", hdmv_pgs_subtitle: "PGS",
        pgssub: "PGS", dvd_subtitle: "VobSub", dvdsub: "VobSub", dvb_subtitle: "DVB", dvb_teletext: "Teletext", eia_608: "CEA-608"
    }
    name = LCase(httpString(codec))
    if name = "" then return ""
    if names.DoesExist(name) then return names[name]
    return UCase(name)
end function

' "4K", "1440p", "1080p", "720p", "576p", "480p", "SD"; "" without a size.
function detResolution(width as dynamic, height as dynamic) as string
    w = httpNumber(width, 0)
    h = httpNumber(height, 0)
    if w <= 0 and h <= 0 then return ""
    if w >= 3800 or h >= 2100 then return "4K"
    if w >= 2500 or h >= 1400 then return "1440p"
    if w >= 1800 or h >= 1000 then return "1080p"
    if w >= 1200 or h >= 700 then return "720p"
    if h >= 540 then return "576p"
    if h >= 470 then return "480p"
    return "SD"
end function

' A video stream's dynamic range as the server names it: "Dolby Vision
' P8.1", "Dolby Vision P7 FEL", "HDR10+", "HDR10", "HLG"; "" for SDR.
function detRange(stream as dynamic) as string
    if stream = invalid then return ""
    range = httpString(stream.dynamic_range)
    if range = "dolby_vision"
        label = "Dolby Vision"
        profile = httpNumber(stream.dv_profile, -1)
        if profile >= 0
            label = label + " P" + urlValue(profile)
            compat = httpNumber(stream.dv_bl_compat, -1)
            if profile >= 8 and (compat = 1 or compat = 2 or compat = 4) then label = label + "." + urlValue(compat)
            layer = httpString(stream.dv_el)
            if profile = 7 and layer <> "" then label = label + " " + UCase(layer)
        end if
        if httpTrue(stream.hdr10plus) then label = label + " + HDR10+"
        return label
    end if
    if range = "hdr10plus" then return "HDR10+"
    if range = "hdr10" then return "HDR10"
    if range = "hlg" then return "HLG"
    if range = "" and httpTrue(stream.hdr) then return "HDR"
    return ""
end function

' "HEVC 4K Dolby Vision P8.1".
function detVideo(stream as dynamic) as string
    if stream = invalid then return ""
    parts = []
    for each part in [detCodec(stream.codec), detResolution(stream.width, stream.height), detRange(stream)]
        if part <> "" then parts.Push(part)
    end for
    return parts.Join(" ")
end function

' "TrueHD 7.1".
function detAudio(stream as dynamic) as string
    if stream = invalid then return ""
    layouts = { "1": "1.0", "2": "2.0", "3": "2.1", "6": "5.1", "7": "6.1", "8": "7.1" }
    label = detCodec(stream.codec)
    channels = httpNumber(stream.channels, 0)
    if channels > 0
        layout = layouts[urlValue(channels)]
        if layout = invalid then layout = urlValue(channels) + "ch"
        label = label + " " + layout
    end if
    return label.Trim()
end function

' "8.2 Mbps", "24 Mbps", "640 kbps"; "" without one.
function detBitrate(bps as dynamic) as string
    value = httpNumber(bps, 0)
    if value <= 0 then return ""
    if value >= 10000000 then return urlValue(Int(value / 1000000 + 0.5)) + " Mbps"
    if value >= 1000000 then return fmtOneDecimal(value / 1000000) + " Mbps"
    return urlValue(Int(value / 1000 + 0.5)) + " kbps"
end function

' How the stream plays: "Direct play", "Direct stream", "Direct stream ·
' Audio transcoding", "Transcoding".
function detMethod(info as object) as string
    method = httpString(info.method)
    if method = "remux"
        if httpTrue(info.audio_converted) then return "Direct stream · Audio transcoding"
        return "Direct stream"
    end if
    if method = "transcode" then return "Transcoding"
    return "Direct play"
end function

' The transcoder's "H.264 1920×1080" as "H.264 1080p".
function detSimplified(text as dynamic) as string
    out = httpSentence(text)
    re = CreateObject("roRegex", "(\d+)×(\d+)", "")
    match = re.Match(out)
    if match.Count() = 3
        size = detResolution(Val(match[1]), Val(match[2]))
        if size <> "" then out = out.Replace(match[0], size)
    end if
    return out
end function

' The playback's details: { method, why, rows: [{ label, value }] }. info
' is the session's PlaybackInfo, file the file it plays, audioIndex the
' audio playing (-1 for the server's), live the session's TranscodeInfo
' (GET /api/playback/{session}) or invalid, positionMs where it is.
function detPlayback(info as object, file as dynamic, audioIndex as integer, live as dynamic, positionMs as integer) as object
    method = httpString(info.method)
    transcode = invalid
    if method <> "direct" then transcode = httpObject(live)
    video = invalid
    audios = []
    if file <> invalid
        for each stream in httpArray(file.streams)
            if GetInterface(stream, "ifAssociativeArray") <> invalid
                kind = httpString(stream.kind)
                if kind = "video" and video = invalid then video = stream
                if kind = "audio" then audios.Push(stream)
            end if
        end for
    end if
    wanted = audioIndex
    if wanted < 0 then wanted = Int(httpNumber(info.audio_index, -1))
    audio = invalid
    for each stream in audios
        if audio = invalid and httpNumber(stream.index, -2) = wanted then audio = stream
    end for
    for each stream in audios
        if audio = invalid and httpTrue(stream.default) then audio = stream
    end for
    if audio = invalid and audios.Count() > 0 then audio = audios[0]
    why = ""
    if method <> "direct"
        reasons = []
        for each reason in httpArray(info.reasons)
            text = httpSentence(reason)
            if text <> "" then reasons.Push(text)
        end for
        why = reasons.Join(" ")
        ' Nothing more specific known: the method says it.
        if why = "" then why = httpSentence(info.reason)
    end if
    rows = []
    converting = method <> "direct"
    if video <> invalid
        value = detVideo(video)
        if transcode <> invalid and httpString(transcode.video_in) <> "" then value = detSimplified(transcode.video_in)
        ' A remux copies the video.
        if method = "transcode" and (transcode = invalid or httpString(transcode.video) <> "copy")
            out = "…"
            if transcode <> invalid and httpString(transcode.video_out) <> ""
                out = detSimplified(transcode.video_out)
                if httpString(transcode.tonemap) <> "" then out = out + " SDR"
            end if
            value = value + " → " + out
        end if
        rows.Push({ label: "Video", value: value })
    end if
    if audio <> invalid
        value = detAudio(audio)
        if transcode <> invalid and httpString(transcode.audio_in) <> "" then value = httpSentence(transcode.audio_in)
        if converting and (transcode = invalid or httpString(transcode.audio) <> "copy") and (method = "transcode" or httpTrue(info.audio_converted))
            out = "…"
            if transcode <> invalid and httpString(transcode.audio_out) <> "" then out = httpSentence(transcode.audio_out)
            value = value + " → " + out
        end if
        rows.Push({ label: "Audio", value: value })
    end if
    if file <> invalid
        bitrate = detBitrate(file.bitrate)
        if bitrate <> "" then rows.Push({ label: "Bitrate", value: bitrate })
    end if
    shown = httpSentence(info.video_range)
    if transcode <> invalid and httpString(transcode.tonemap) <> "" then shown = "SDR (tone-mapped with " + httpSentence(transcode.tonemap) + ")"
    if video <> invalid and shown <> "" then rows.Push({ label: "Shown as", value: shown })
    if httpTrue(info.burn_subtitles) then rows.Push({ label: "Subtitles", value: "Burned in" })
    if transcode <> invalid
        speed = httpNumber(transcode.speed, 0)
        if speed > 0
            value = fmtOneDecimal(speed) + "× real time"
            ahead = Int((httpNumber(transcode.transcoded_ms, 0) - positionMs) / 1000)
            if ahead > 0 then value = value + " · " + urlValue(ahead) + " s ready"
            rows.Push({ label: "Speed", value: value })
        end if
    end if
    return { method: detMethod(info), why: why, rows: rows }
end function

' A file's chapters, by start: [{ label ("1. Opening · 0:00"), startMs }].
function detChapters(file as dynamic) as object
    out = []
    if file = invalid then return out
    for each chapter in httpArray(file.chapters)
        if GetInterface(chapter, "ifAssociativeArray") <> invalid
            startMs = Int(httpNumber(chapter.start_ms, -1))
            if startMs >= 0
                number = out.Count() + 1
                title = httpSentence(chapter.title)
                if title = "" then title = "Chapter " + urlValue(number)
                out.Push({ label: urlValue(number) + ". " + title + " · " + fmtClock(startMs), startMs: startMs })
            end if
        end if
    end for
    return out
end function

' The chapter positionMs is in (its index), 0 before the first.
function detChapterAt(chapters as object, positionMs as integer) as integer
    at = 0
    for i = 0 to chapters.Count() - 1
        if chapters[i].startMs <= positionMs then at = i
    end for
    return at
end function
