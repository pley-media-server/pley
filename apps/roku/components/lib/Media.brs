' A title's files as its page shows them, in the other apps' words
' (packages/tv/src/format/format.ts, the Android app's format): the
' version, audio and subtitles to start with, which the person picks
' before playing, and their media info. Tracks are saved with Tracks.brs's
' rules.

' A track's language by name: "English", "Unknown" for und; a code not
' among the 34 in capitals ("KUR"), a longer tag as it is; "" for none.
function mediaLanguage(code as dynamic) as string
    text = httpString(code).Trim()
    if text = "" then return ""
    lower = LCase(text)
    if lower = "und" or lower = "mul" or lower = "zxx" then return t("player.media.languages." + lower)
    name = fmtLanguageName(lower, "")
    if name <> lower then return name
    if Len(text) <= 3 then return UCase(text)
    return text
end function

' A language's English name, as files' titles mostly have it ("English
' SDH" while the app is in French); "" for a code not among the 34. Files
' name some by their ISO 639-2/T code, as fmtLanguageName knows.
function mediaEnglishLanguage(code as dynamic) as string
    wanted = LCase(httpString(code).Trim())
    other = { fra: "fre", deu: "ger", zho: "chi", ces: "cze", nld: "dut", ell: "gre", fas: "per", ron: "rum", slk: "slo" }
    if other.DoesExist(wanted) then wanted = other[wanted]
    if wanted = "" or Instr(1, wanted, ".") > 0 then return ""
    name = i18nTemplate("en", "format.languages." + wanted, invalid)
    if name = invalid then return ""
    return name
end function

' The name of a track's language (language, as shown; code, the file's)
' that its title has: the one shown, or the English one; "" for neither.
function mediaNameInTitle(title as string, language as string, code as dynamic) as string
    lower = LCase(title)
    for each name in [language, mediaEnglishLanguage(code)]
        if name <> "" and Instr(1, lower, LCase(name)) > 0 then return name
    end for
    return ""
end function

' Dolby Atmos, as ffprobe names it in the stream's profile or a title says.
function mediaAtmos(stream as object) as boolean
    if httpString(stream.kind) <> "audio" then return false
    re = CreateObject("roRegex", "atmos", "i")
    return re.IsMatch(httpString(stream.profile)) or re.IsMatch(httpString(stream.title))
end function

' 1 → "Mono", 2 → "Stereo", 6 → "5.1", 8 → "7.1", 5 → "5ch"; the
' layout's name without a count.
function mediaChannels(channels as dynamic, layout as dynamic) as string
    count = Int(httpNumber(channels, 0))
    if count <= 0 then return httpString(layout)
    if count = 1 then return t("player.media.mono")
    if count = 2 then return t("player.media.stereo")
    names = { "3": "2.1", "6": "5.1", "7": "6.1", "8": "7.1" }
    name = names[urlValue(count)]
    if name = invalid then return t("player.media.channelCount", { n: count })
    return name
end function

' "TrueHD Atmos 7.1", "EAC3 5.1", "AAC Stereo".
function mediaAudioFormat(stream as object) as string
    parts = []
    codec = detCodec(stream.codec)
    if codec <> "" then parts.Push(codec)
    if mediaAtmos(stream) then parts.Push("Atmos")
    channels = mediaChannels(stream.channels, stream.channel_layout)
    if channels <> "" then parts.Push(channels)
    return parts.Join(" ")
end function

' Subtitles the server counts as forced: flagged so, or a title saying so
' ("English (Forced)", "Signs & Songs"), as its streams::is_forced does.
function mediaForced(stream as object) as boolean
    if httpTrue(stream.forced) then return true
    re = CreateObject("roRegex", "(^|[^0-9a-z])(forced|signs)($|[^0-9a-z])", "i")
    return re.IsMatch(httpString(stream.title))
end function

' What a subtitle title says beyond its language and "forced" (shown as a
' flag): "SDH" of "English SDH", nothing of "English (Forced)", the whole
' title when it doesn't name the language ("Signs & Songs").
function mediaTitleRest(title as string, language as string) as string
    forced = CreateObject("roRegex", "\bforced\b", "i")
    at = Instr(1, LCase(title), LCase(language))
    if at = 0
        if mediaHasWord(forced.ReplaceAll(title, "")) then return title
        return ""
    end if
    rest = Left(title, at - 1) + " " + Mid(title, at + Len(language))
    rest = forced.ReplaceAll(rest, " ")
    ' The dashes by themselves: a byte of theirs alone could match others.
    rest = CreateObject("roRegex", "([()\[\]{}\-_.,:;|/]|–|—)+", "").ReplaceAll(rest, " ")
    rest = CreateObject("roRegex", "\s+", "").ReplaceAll(rest, " ")
    return rest.Trim()
end function

' Whether text has a letter or a digit (of any alphabet past ASCII too).
function mediaHasWord(text as string) as boolean
    for i = 1 to Len(text)
        c = Asc(Mid(text, i, 1))
        if (c >= 48 and c <= 57) or (c >= 65 and c <= 90) or (c >= 97 and c <= 122) or c >= 192 then return true
    end for
    return false
end function

' A track as the title's page lists it, as the other apps do. Audio:
' "English (EAC3 5.1)", "English – Commentary (AAC Stereo)". Subtitles:
' "French – SDH (SRT, Forced)". A title naming the language, as shown or
' in English ("English SDH" in French), doesn't name it twice.
function mediaTrackLabel(stream as object) as string
    language = mediaLanguage(stream.language)
    title = httpSentence(stream.title)
    named = ""
    if title <> "" and language <> "" then named = mediaNameInTitle(title, language, stream.language)
    if httpString(stream.kind) = "audio"
        base = language
        if base = "" then base = title
        if base = "" then base = t("player.tracks.unknown")
        extra = ""
        if title <> "" and language <> "" and named = "" then extra = " – " + title
        format = mediaAudioFormat(stream)
        if format <> "" then format = " (" + format + ")"
        return base + extra + format
    end if
    base = language
    if base = "" then base = title
    if base = "" then base = t("player.tracks.unknown")
    rest = ""
    if title <> "" and language <> ""
        if named = "" then named = language
        rest = mediaTitleRest(title, named)
    end if
    if rest <> "" then rest = " – " + rest
    flags = []
    codec = detCodec(stream.codec)
    if codec <> "" then flags.Push(codec)
    if mediaForced(stream) then flags.Push(t("player.tracks.forced"))
    if httpTrue(stream.external) then flags.Push(t("player.tracks.external"))
    text = base + rest
    if flags.Count() > 0 then text = text + " (" + flags.Join(", ") + ")"
    return text
end function

' Whether the profile picked the file's audio track (audio_picked); an
' older server doesn't say, and its choice reads as picked.
function mediaPicked(file as object) as boolean
    if GetInterface(file.audio_picked, "ifBoolean") = invalid then return true
    return file.audio_picked
end function

' The audio a title's page offers while no track is picked, as the other
' apps word it: "Automatic (English)", the language of the details' choice
' (the profile's defaults pick it), else of the file's default track.
function mediaAutomaticLabel(file as object) as string
    audio = tracksAudioStreams(file)
    track = invalid
    selected = httpNumber(file.selected_audio, -1)
    for each stream in audio
        if track = invalid and httpNumber(stream.index, -2) = selected then track = stream
    end for
    for each stream in audio
        if track = invalid and httpTrue(stream.default) then track = stream
    end for
    if track = invalid and audio.Count() > 0 then track = audio[0]
    code = ""
    if track <> invalid then code = LCase(httpString(track.language))
    if code = "" or code = "und" or code = "zxx" or code = "mul" then return t("player.tracks.automatic")
    return t("player.tracks.automaticLanguage", { language: mediaLanguage(code) })
end function

' What a title's page offers before its file plays: { audio, subtitles },
' each { labels, values, current } (values: the file's stream indexes, -1
' for Automatic and Off). Audio is offered for two tracks or more,
' Automatic first: no track picked, each device plays its best of the
' language. Subtitles for any, Off first.
function mediaTracks(file as dynamic) as object
    out = { audio: { labels: [], values: [], current: 0 }, subtitles: { labels: [], values: [], current: 0 } }
    data = httpObject(file)
    if data = invalid then return out
    audio = tracksAudioStreams(data)
    if audio.Count() > 1
        picked = -1
        if mediaPicked(data) then picked = httpNumber(data.selected_audio, -1)
        out.audio.labels.Push(mediaAutomaticLabel(data))
        out.audio.values.Push(-1)
        for each stream in audio
            index = Int(httpNumber(stream.index, -1))
            if index = picked then out.audio.current = out.audio.values.Count()
            out.audio.labels.Push(mediaTrackLabel(stream))
            out.audio.values.Push(index)
        end for
    end if
    selected = httpNumber(data.selected_subtitle, -1)
    for each stream in httpArray(data.streams)
        subtitle = httpObject(stream)
        if subtitle <> invalid and httpString(subtitle.kind) = "subtitle" and httpNumber(subtitle.index, -1) >= 0
            if out.subtitles.values.Count() = 0
                out.subtitles.labels.Push(t("player.tracks.off"))
                out.subtitles.values.Push(-1)
            end if
            index = Int(httpNumber(subtitle.index, -1))
            if index = selected then out.subtitles.current = out.subtitles.values.Count()
            out.subtitles.labels.Push(mediaTrackLabel(subtitle))
            out.subtitles.values.Push(index)
        end if
    end for
    return out
end function

' item (ItemDetail) as the server answers once audioIndex (-1 Automatic,
' -2 left as it was) and subtitleIndex (-1 off) were picked for its file
' fileId: the page, and Play pressed at once, start with them.
sub mediaPick(item as object, fileId as dynamic, audioIndex as integer, subtitleIndex as integer)
    for each file in httpArray(item.files)
        if httpObject(file) <> invalid and httpNumber(file.id, -1) = httpNumber(fileId, -2)
            if audioIndex <> -2
                file.selected_audio = invalid
                if audioIndex >= 0 then file.selected_audio = audioIndex
                file.audio_picked = audioIndex >= 0
            end if
            file.selected_subtitle = invalid
            if subtitleIndex >= 0 then file.selected_subtitle = subtitleIndex
        end if
    end for
end sub

' --- Versions ---

' A file's first video stream, invalid for none.
function mediaVideo(file as object) as dynamic
    for each stream in httpArray(file.streams)
        if httpObject(stream) <> invalid and httpString(stream.kind) = "video" then return stream
    end for
    return invalid
end function

' The audio a file is described by: the profile's pick, else the default,
' else the first. Without a pick each device plays its best track of the
' language, so the line doesn't name one as if it would play.
function mediaStartingAudio(file as object) as dynamic
    audio = tracksAudioStreams(file)
    if mediaPicked(file)
        for each stream in audio
            if httpNumber(stream.index, -2) = httpNumber(file.selected_audio, -1) then return stream
        end for
    end if
    for each stream in audio
        if httpTrue(stream.default) then return stream
    end for
    if audio.Count() > 0 then return audio[0]
    return invalid
end function

' A version as a title's page lists it, as the other TVs do: "Final Cut ·
' 4K · HEVC · DV P8.1 · TrueHD Atmos 7.1 · 62.0 GB" ("Version 2" when
' nothing is known). Dolby Vision is shortened as badges shorten it, so
' the line fits the panel.
function mediaVersionLine(file as object, number as integer) as string
    video = mediaVideo(file)
    width = file.width
    height = file.height
    codec = file.video_codec
    range = ""
    if video <> invalid
        if httpNumber(width, 0) <= 0 then width = video.width
        if httpNumber(height, 0) <= 0 then height = video.height
        if httpString(codec) = "" then codec = video.codec
        range = detRange(video).Replace("Dolby Vision", "DV")
    end if
    audio = mediaStartingAudio(file)
    parts = []
    for each part in [httpSentence(file.edition), detResolution(width, height), detCodec(codec), range]
        if part <> "" then parts.Push(part)
    end for
    if audio <> invalid
        format = mediaAudioFormat(audio)
        if format <> "" then parts.Push(format)
    end if
    if httpNumber(file.size, 0) > 0 then parts.Push(fmtBytes(file.size))
    if parts.Count() = 0 then return t("player.tracks.version", { number: number })
    return parts.Join(" · ")
end function

' A title's versions, one per file: { labels, values (file ids), current
' (the one fileId names, else the first) }.
function mediaVersions(item as object, fileId as dynamic) as object
    out = { labels: [], values: [], current: 0 }
    wanted = httpNumber(fileId, -1)
    for each file in httpArray(item.files)
        if httpObject(file) <> invalid
            id = httpNumber(file.id, 0)
            if id = wanted then out.current = out.values.Count()
            out.labels.Push(mediaVersionLine(file, out.values.Count() + 1))
            out.values.Push(id)
        end if
    end for
    return out
end function

' --- Media info ---
' Each file and every stream in full, as the other apps' media info
' (packages/tv/src/format/media-info.ts, the Android app's MediaFacts)
' words it.

' A codec level as people write it: HEVC 153 → "5.1", H.264 51 → "5.1",
' AV1 13 → "5.1"; "" without one. A level is a name, written with a point
' in every language, as Dolby Vision's profiles are.
function mediaLevel(codec as dynamic, level as dynamic) as string
    value = Int(httpNumber(level, 0))
    if value <= 0 then return ""
    name = LCase(httpString(codec))
    if name = "av1" then return urlValue(2 + Int(value / 4)) + "." + urlValue(value mod 4)
    tenths = value
    if name = "hevc" or name = "h265" then tenths = Int(value / 3 + 0.5)
    if tenths mod 10 = 0 then return urlValue(tenths \ 10)
    return urlValue(tenths \ 10) + "." + urlValue(tenths mod 10)
end function

' "Main 10 · Level 5.1".
function mediaProfile(stream as object) as string
    parts = []
    profile = httpSentence(stream.profile)
    if profile <> "" then parts.Push(profile)
    level = mediaLevel(stream.codec, stream.level)
    if level <> "" then parts.Push(t("player.media.level", { level: level }))
    return parts.Join(" · ")
end function

' "23.976 fps" ("23,976 i/s" in French): up to three decimals, trailing
' zeros left out.
function mediaFrameRate(fps as dynamic, interlaced as boolean) as string
    value = httpNumber(fps, 0)
    if value <= 0 then return ""
    thousandths = Int(value * 1000 + 0.5)
    decimals = 0
    if thousandths mod 1000 > 0
        digits = Right("00" + urlValue(thousandths mod 1000), 3)
        while Right(digits, 1) = "0"
            digits = Left(digits, Len(digits) - 1)
        end while
        decimals = Len(digits)
    end if
    text = i18nNumber(thousandths / 1000, decimals)
    if interlaced then return t("player.media.fpsInterlaced", { fps: text })
    return t("player.media.fps", { fps: text })
end function

' "10-bit · 4:2:0".
function mediaBitDepth(depth as dynamic, pixFmt as dynamic) as string
    parts = []
    bits = Int(httpNumber(depth, 0))
    if bits > 0 then parts.Push(t("player.media.bits", { n: bits }))
    format = LCase(httpString(pixFmt))
    chroma = ""
    if CreateObject("roRegex", "^(yuv420|nv12|p010)", "").IsMatch(format)
        chroma = "4:2:0"
    else if Left(format, 6) = "yuv422"
        chroma = "4:2:2"
    else if Left(format, 6) = "yuv444"
        chroma = "4:4:4"
    else if Left(format, 4) = "gray"
        chroma = "4:0:0"
    end if
    if chroma <> "" then parts.Push(chroma)
    return parts.Join(" · ")
end function

' A colour property by name ("BT.2020", "PQ (SMPTE ST 2084)"); one not
' known as ffprobe names it; "" for none.
function mediaColour(table as object, name as dynamic) as string
    value = httpString(name)
    if value = "" or value = "unknown" or value = "reserved" then return ""
    if table.DoesExist(value) then return table[value]
    return value
end function

function mediaPrimaries() as object
    return { bt709: "BT.709", bt2020: "BT.2020", smpte170m: "BT.601 NTSC", bt470bg: "BT.601 PAL", bt470m: "BT.470 M", smpte240m: "SMPTE 240M", smpte431: "DCI-P3", smpte432: "Display P3", film: "Film" }
end function

function mediaTransfers() as object
    return { smpte2084: "PQ (SMPTE ST 2084)", "arib-std-b67": "HLG (ARIB STD-B67)", bt709: "BT.709", smpte170m: "BT.601", bt470bg: "BT.470 BG", "bt2020-10": "BT.2020 (10-bit)", "bt2020-12": "BT.2020 (12-bit)", "iec61966-2-1": "sRGB", linear: "Linear" }
end function

function mediaMatrices() as object
    return { bt709: "BT.709", bt2020nc: "BT.2020 non-constant", bt2020c: "BT.2020 constant", smpte170m: "BT.601", bt470bg: "BT.601", ictcp: "ICtCp", gbr: "RGB", ycgco: "YCgCo", fcc: "FCC" }
end function

' "Profile 8.1 · Level 6 · HDR10-compatible base layer"; "" without a
' Dolby Vision profile.
function mediaDolbyVision(stream as object) as string
    profile = Int(httpNumber(stream.dv_profile, -1))
    if profile < 0 then return ""
    compat = Int(httpNumber(stream.dv_bl_compat, 0))
    name = urlValue(profile)
    if profile >= 8 and (compat = 1 or compat = 2 or compat = 4) then name = name + "." + urlValue(compat)
    parts = [t("player.media.dvProfile", { profile: name })]
    level = httpNumber(stream.dv_level, -1)
    if level >= 0 then parts.Push(t("player.media.level", { level: urlValue(level) }))
    bases = { "1": "HDR10", "2": "SDR", "4": "HLG", "6": "Blu-ray HDR10" }
    base = bases[urlValue(compat)]
    if compat = 0
        parts.Push(t("player.media.noBaseLayer"))
    else if base <> invalid
        parts.Push(t("player.media.baseLayer", { range: base }))
    end if
    layer = httpString(stream.dv_el)
    if profile = 7 and layer = "fel" then parts.Push(t("player.media.fullLayer"))
    if profile = 7 and layer = "mel" then parts.Push(t("player.media.minimalLayer"))
    return parts.Join(" · ")
end function

' "3840×2160 (4K)".
function mediaDimensions(width as dynamic, height as dynamic) as string
    w = httpNumber(width, 0)
    h = httpNumber(height, 0)
    if w <= 0 or h <= 0 then return ""
    text = urlValue(w) + "×" + urlValue(h)
    size = detResolution(w, h)
    if size <> "" then text = text + " (" + size + ")"
    return text
end function

' "Default, Forced, Beside the media".
function mediaFlags(stream as object) as string
    flags = []
    if httpTrue(stream.default) then flags.Push(t("player.media.default"))
    if httpTrue(stream.forced) then flags.Push(t("player.tracks.forced"))
    if httpTrue(stream.external) then flags.Push(t("player.media.beside"))
    return flags.Join(", ")
end function

' The file name only, after the last / or \.
function mediaFileName(path as dynamic) as string
    text = httpSentence(path)
    for each mark in ["/", "\"]
        at = Instr(1, text, mark)
        while at > 0
            text = Mid(text, at + 1)
            at = Instr(1, text, mark)
        end while
    end for
    return text
end function

sub mediaFact(facts as object, label as string, value as string)
    if value.Trim() <> "" then facts.Push({ label: label, value: value })
end sub

' A file's facts in blocks ([{ title, facts: [{ label, value }] }]): the
' file, then each stream, video first.
function mediaFacts(file as object) as object
    facts = []
    mediaFact(facts, t("player.media.name"), mediaFileName(file.path))
    mediaFact(facts, t("player.media.container"), UCase(httpString(file.container)))
    if httpNumber(file.size, 0) > 0 then mediaFact(facts, t("player.media.size"), fmtBytes(file.size))
    mediaFact(facts, t("player.media.duration"), fmtDuration(file.duration_ms))
    mediaFact(facts, t("player.media.overallBitrate"), fmtBitrate(file.bitrate))
    blocks = [{ title: t("player.media.file"), facts: facts }]
    names = { video: t("player.media.video"), audio: t("player.media.audio"), subtitle: t("player.media.subtitles") }
    for each kind in ["video", "audio", "subtitle"]
        streams = []
        for each stream in httpArray(file.streams)
            if httpObject(stream) <> invalid and httpString(stream.kind) = kind then streams.Push(stream)
        end for
        streams.SortBy("index")
        for i = 0 to streams.Count() - 1
            title = names[kind]
            if streams.Count() > 1 then title = t("player.media.numbered", { kind: title, n: i + 1 })
            blocks.Push({ title: title, facts: mediaStreamFacts(streams[i]) })
        end for
    end for
    return blocks
end function

' One stream's facts.
function mediaStreamFacts(stream as object) as object
    facts = []
    kind = httpString(stream.kind)
    language = mediaLanguage(stream.language)
    if kind = "video"
        mediaFact(facts, t("player.media.codec"), detCodec(stream.codec))
        mediaFact(facts, t("player.media.profile"), mediaProfile(stream))
        mediaFact(facts, t("player.media.resolution"), mediaDimensions(stream.width, stream.height))
        order = httpString(stream.field_order)
        mediaFact(facts, t("player.media.frameRate"), mediaFrameRate(stream.frame_rate, httpTrue(stream.interlaced) or (order <> "" and order <> "progressive")))
        mediaFact(facts, t("player.media.bitDepth"), mediaBitDepth(stream.bit_depth, stream.pix_fmt))
        range = detRange(stream)
        if range = "" then range = "SDR"
        mediaFact(facts, "HDR", range)
        mediaFact(facts, "Dolby Vision", mediaDolbyVision(stream))
        mediaFact(facts, t("player.media.primaries"), mediaColour(mediaPrimaries(), stream.color_primaries))
        mediaFact(facts, t("player.media.transfer"), mediaColour(mediaTransfers(), stream.color_transfer))
        mediaFact(facts, t("player.media.matrix"), mediaColour(mediaMatrices(), stream.color_space))
        mediaFact(facts, t("player.media.bitrate"), fmtBitrate(stream.bitrate))
        mediaFact(facts, t("player.media.title"), httpSentence(stream.title))
    else if kind = "audio"
        codec = detCodec(stream.codec)
        if mediaAtmos(stream) then codec = codec + " Atmos"
        mediaFact(facts, t("player.media.codec"), codec)
        mediaFact(facts, t("player.media.profile"), httpSentence(stream.profile))
        channels = mediaChannels(stream.channels, stream.channel_layout)
        layout = httpString(stream.channel_layout)
        if layout <> "" and channels <> "" and layout <> channels then channels = channels + " (" + layout + ")"
        mediaFact(facts, t("player.media.channels"), channels)
        hertz = httpNumber(stream.sample_rate, 0)
        if hertz > 0 then mediaFact(facts, t("player.media.sampleRate"), fmtOneDecimal(hertz / 1000) + " kHz")
        mediaFact(facts, t("player.media.language"), language)
        mediaFact(facts, t("player.media.title"), httpSentence(stream.title))
        mediaFact(facts, t("player.media.bitrate"), fmtBitrate(stream.bitrate))
        mediaFact(facts, t("player.media.flags"), mediaFlags(stream))
    else
        format = t("player.media.formatText", { codec: detCodec(stream.codec) })
        if GetInterface(stream.text_based, "ifBoolean") <> invalid and not stream.text_based then format = t("player.media.formatPicture", { codec: detCodec(stream.codec) })
        mediaFact(facts, t("player.media.format"), format)
        mediaFact(facts, t("player.media.language"), language)
        mediaFact(facts, t("player.media.title"), httpSentence(stream.title))
        mediaFact(facts, t("player.media.flags"), mediaFlags(stream))
    end if
    return facts
end function

' A title's media info as text, for a panel that scrolls: each file (in
' capitals, by its edition, or "Version 2" when there are several) and its
' blocks, "Label: value" under each block's title, a line between blocks.
function mediaInfoText(item as object) as string
    files = []
    for each file in httpArray(item.files)
        if httpObject(file) <> invalid then files.Push(file)
    end for
    lines = []
    for i = 0 to files.Count() - 1
        title = httpSentence(files[i].edition)
        if title = "" and files.Count() > 1 then title = t("player.tracks.version", { number: i + 1 })
        if title <> ""
            if lines.Count() > 0 then lines.Push("")
            lines.Push(UCase(title))
        end if
        under = title <> ""
        for each block in mediaFacts(files[i])
            if lines.Count() > 0 and not under then lines.Push("")
            under = false
            lines.Push(block.title)
            for each fact in block.facts
                lines.Push(t("player.media.fact", { label: fact.label, value: fact.value }))
            end for
        end for
    end for
    return lines.Join(Chr(10))
end function
