' What this Roku plays, sent as caps with POST /api/playback (docs/API.md,
' "Version 2"), from the device's own answers: device is roDeviceInfo, or
' a stand-in with the same methods in tests. Asked again before each
' playback, since the TV, receiver or audio setting can change.
'
' The report is generous: whatever the Roku says it decodes is offered, so
' files play as they are wherever they can, and the player falls back to
' a conversion when one doesn't (PlayerScreen.brs).
'
' Codec names are the server's (ffprobe's); Roku's own ("mpeg4 avc") are
' only used in the questions.

' Containers the Video node plays as they are: video files, and audio ones.
' WebM too when a VP9 or AV1 decoder can read what it holds.
function capsContainers(webm as boolean) as object
    containers = ["mp4", "m4v", "mov", "mkv", "mp3", "m4a", "flac", "wav", "aac"]
    if webm then containers.Push("webm")
    return containers
end function

' What this Roku plays (POST /api/playback's caps). drawsPictures: whether
' the app draws PGS itself; once the server turned out not to make the
' pictures, it doesn't, and the server burns PGS in again.
function capsBuild(device as object, drawsPictures = true as boolean) as object
    video = []
    h264 = capsH264(device)
    video.Push(h264)
    hevc = capsHevc(device)
    video.Append(hevc)
    tenBit = false
    fourK = h264.max_height >= 2160
    for each entry in hevc
        if entry.max_bit_depth >= 10 then tenBit = true
        if entry.max_height >= 2160 then fourK = true
    end for
    vp9 = capsVp9(device)
    if vp9 <> invalid then video.Push(vp9)
    av1 = capsAv1(device, fourK)
    if av1 <> invalid then video.Push(av1)

    ' What the screen shows, with a decoder that feeds it.
    display = device.GetDisplayProperties()
    hdrTypes = []
    dolbyVision = false
    if tenBit
        if capsFlag(display, "Hdr10") then hdrTypes.Push("hdr10")
        if capsFlag(display, "Hdr10Plus") then hdrTypes.Push("hdr10plus")
        if capsFlag(display, "HLG") then hdrTypes.Push("hlg")
        dolbyVision = capsFlag(display, "DolbyVision")
        if dolbyVision then hdrTypes.Push("dolby_vision")
    end if

    channels = capsChannels(device)
    decode = capsAudioDecode(device)
    passthrough = []
    ' Sent as they are only when the audio setting is surround: in stereo
    ' the Roku mixes everything down itself.
    if channels > 2 then passthrough = capsPassthrough(device)
    audioCodecs = []
    audioCodecs.Append(decode)
    for each codec in passthrough
        if not capsListed(audioCodecs, codec) then audioCodecs.Push(codec)
    end for

    videoCodecs = []
    for each entry in video
        if not capsListed(videoCodecs, entry.codec) then videoCodecs.Push(entry.codec)
    end for

    hdr = capsListed(hdrTypes, "hdr10") or capsListed(hdrTypes, "hlg")
    report = {
        containers: capsContainers(vp9 <> invalid or av1 <> invalid),
        video_codecs: videoCodecs,
        audio_codecs: audioCodecs,
        hdr: hdr,
        dolby_vision: dolbyVision,
        caps_version: 2,
        video: video,
        hdr_types: hdrTypes,
        audio_decode: decode,
        audio_passthrough: passthrough,
        audio_channels: channels,
        ' The Video node moves between a transcode's sizes by itself (adaptive bitrate).
        hls: { master_playlist: true, segments: ["ts"], variants: true },
        device: { form: "tv", model: capsModel(device) }
    }
    ' PGS is drawn by the app over whatever plays (Pictures.brs), so the
    ' server doesn't burn it in; VobSub and DVB, not listed, are. To the
    ' server, a listed codec is the player's to show: unlisted, PGS asked
    ' for is burned in.
    if drawsPictures
        report.subtitle_codecs = ["hdmv_pgs_subtitle"]
        report.picture_subtitles_sideload = true
    end if
    ' Roku's Dolby Vision decoders take profiles 5 and 8; their levels go
    ' unsaid, so unchecked.
    if dolbyVision then report.dolby_vision_profiles = [{ profile: 5 }, { profile: 8 }]
    mode = capsVideoMode(device.GetVideoMode())
    if mode <> invalid then report.display = { width: mode.width, height: mode.height, refresh_rates: [mode.rate] }
    return report
end function

' What Diagnostics says this Roku plays, from its report (capsBuild), a
' line each, as the other TVs list it.
function capsLines(report as object) as object
    lines = []
    display = httpObject(report.display)
    if display <> invalid
        rates = httpArray(display.refresh_rates)
        size = urlValue(display.width) + "×" + urlValue(display.height)
        if rates.Count() > 0
            lines.Push(t("player.caps.displayRate", { size: size, rate: urlValue(rates[0]) }))
        else
            lines.Push(t("player.caps.display", { size: size }))
        end if
    end if
    hdr = []
    for each kind in httpArray(report.hdr_types)
        hdr.Push(capsHdrName(kind))
    end for
    if hdr.Count() = 0 then hdr.Push(t("player.caps.none"))
    lines.Push(t("player.caps.hdr", { list: hdr.Join(", ") }))
    profiles = []
    for each entry in httpArray(report.dolby_vision_profiles)
        profiles.Push(urlValue(entry.profile))
    end for
    if profiles.Count() > 0 then lines.Push(t("player.caps.dolbyVision", { list: profiles.Join(", ") }))
    video = []
    tenBit = t("player.media.bits", { n: 10 })
    for each entry in httpArray(report.video)
        text = capsCodecName(entry.codec) + " " + urlValue(entry.max_width) + "×" + urlValue(entry.max_height)
        if httpNumber(entry.max_bit_depth, 8) >= 10 then text = text + " " + tenBit
        video.Push(text)
    end for
    lines.Push(t("player.caps.video", { list: video.Join(", ") }))
    lines.Push(t("player.caps.audioDecoded", { list: httpArray(report.audio_decode).Join(", ") }))
    passthrough = httpArray(report.audio_passthrough)
    sent = t("player.caps.nothing")
    if passthrough.Count() > 0 then sent = passthrough.Join(", ")
    lines.Push(t("player.caps.sent", { list: sent }))
    sound = t("player.media.stereo")
    if httpNumber(report.audio_channels, 2) > 2 then sound = t("player.caps.surround")
    lines.Push(t("player.caps.sound", { sound: sound }))
    if httpTrue(report.picture_subtitles_sideload) and capsListed(httpArray(report.subtitle_codecs), "hdmv_pgs_subtitle")
        lines.Push(t("player.caps.pictures"))
    end if
    lines.Push(t("player.caps.containers", { list: httpArray(report.containers).Join(", ") }))
    lines.Push(t("player.caps.streams"))
    return lines
end function

function capsCodecName(codec as dynamic) as string
    names = { h264: "H.264", hevc: "HEVC", vp9: "VP9", av1: "AV1" }
    name = httpString(codec)
    if names.DoesExist(name) then return names[name]
    return name
end function

function capsHdrName(kind as dynamic) as string
    names = { hdr10: "HDR10", hdr10plus: "HDR10+", hlg: "HLG", dolby_vision: "Dolby Vision" }
    name = httpString(kind)
    if names.DoesExist(name) then return names[name]
    return name
end function

' Whether CanDecodeVideo or CanDecodeAudio said yes.
function capsYes(answer as dynamic) as boolean
    if GetInterface(answer, "ifAssociativeArray") = invalid then return false
    result = answer.result
    if GetInterface(result, "ifBoolean") = invalid then return false
    return result
end function

function capsFlag(values as dynamic, key as string) as boolean
    if GetInterface(values, "ifAssociativeArray") = invalid then return false
    value = values[key]
    if GetInterface(value, "ifBoolean") = invalid then return false
    return value
end function

function capsListed(list as object, value as string) as boolean
    for each item in list
        if item = value then return true
    end for
    return false
end function

' The highest of levels (as Roku writes them, "5.1") that codec and profile
' decode at, or "".
function capsBestLevel(device as object, codec as string, profile as string, levels as object) as string
    best = ""
    for each level in levels
        format = { codec: codec, level: level }
        if profile <> "" then format.profile = profile
        if capsYes(device.CanDecodeVideo(format)) then best = level
    end for
    return best
end function

' A level as ffprobe numbers it: H.264 5.1 is 51, HEVC 5.1 is 153.
function capsLevelNumber(level as string, factor as integer) as integer
    return Int(Val(level) * factor + 0.5)
end function

' H.264, which every Roku plays up to 1080p (High, level 4.1) even when
' firmware won't say so; level 5.1 and up means 4K.
function capsH264(device as object) as object
    levels = ["4.1", "4.2", "5.0", "5.1", "5.2"]
    best = capsBestLevel(device, "mpeg4 avc", "high", levels)
    if best = "" then best = capsBestLevel(device, "h264", "high", levels)
    if best = "" then best = "4.1"
    width = 1920
    height = 1080
    if Val(best) >= 5.1
        width = 3840
        height = 2160
    end if
    return {
        codec: "h264",
        hardware: true,
        profiles: ["Constrained Baseline", "Baseline", "Main", "High"],
        max_width: width,
        max_height: height,
        max_bit_depth: 8,
        max_level: capsLevelNumber(best, 10)
    }
end function

' HEVC, as one entry per profile level: Main 10 (which takes Main too), and
' Main alone when it goes higher.
function capsHevc(device as object) as object
    levels = ["4.1", "5.0", "5.1", "5.2"]
    main = capsBestLevel(device, "hevc", "main", levels)
    main10 = capsBestLevel(device, "hevc", "main 10", levels)
    entries = []
    if main10 <> "" then entries.Push(capsHevcEntry(["Main", "Main 10"], 10, main10))
    if main <> "" and (main10 = "" or Val(main) > Val(main10)) then entries.Push(capsHevcEntry(["Main"], 8, main))
    return entries
end function

function capsHevcEntry(profiles as object, depth as integer, level as string) as object
    width = 1920
    height = 1080
    if Val(level) >= 5.0
        width = 3840
        height = 2160
    end if
    return {
        codec: "hevc",
        hardware: true,
        profiles: profiles,
        max_width: width,
        max_height: height,
        max_bit_depth: depth,
        max_level: capsLevelNumber(level, 30)
    }
end function

' AV1, when the Roku says it decodes it, for files played as they are (it
' never travels in pley's HLS segments). Asked about alone: Roku says no
' to AV1 asked with a container even where it plays it. Its limits go
' unsaid; a Roku with 4K decoders plays 4K AV1.
function capsAv1(device as object, fourK as boolean) as dynamic
    if not capsYes(device.CanDecodeVideo({ codec: "av1" })) then return invalid
    width = 1920
    height = 1080
    if fourK
        width = 3840
        height = 2160
    end if
    return { codec: "av1", hardware: true, profiles: ["Main"], max_width: width, max_height: height, max_bit_depth: 10 }
end function

' VP9, when it plays from Matroska files (it never travels in pley's HLS
' segments).
function capsVp9(device as object) as dynamic
    if not capsYes(device.CanDecodeVideo({ codec: "vp9", container: "mkv" })) then return invalid
    levels = ["4.1", "5.0", "5.1"]
    profile0 = capsBestLevel(device, "vp9", "profile 0", levels)
    profile2 = capsBestLevel(device, "vp9", "profile 2", levels)
    if profile0 = "" then return invalid
    profiles = ["Profile 0"]
    depth = 8
    if profile2 <> ""
        profiles.Push("Profile 2")
        depth = 10
    end if
    width = 1920
    height = 1080
    if Val(profile0) >= 5.0
        width = 3840
        height = 2160
    end if
    return { codec: "vp9", hardware: true, profiles: profiles, max_width: width, max_height: height, max_bit_depth: depth }
end function

' Audio decoded on the Roku: [Roku's name, ffprobe's names].
function capsAudioNames() as object
    return [
        ["aac", ["aac"]],
        ["mp3", ["mp3"]],
        ["ac3", ["ac3"]],
        ["eac3", ["eac3"]],
        ["flac", ["flac"]],
        ["alac", ["alac"]],
        ["opus", ["opus"]],
        ["vorbis", ["vorbis"]],
        ["mp2", ["mp2"]],
        ["dts", ["dts"]],
        ["pcm", ["pcm_s16le", "pcm_s24le", "pcm_s16be", "pcm_s24be"]]
    ]
end function

function capsAudioDecode(device as object) as object
    ' Every Roku decodes AAC, what the server converts audio to.
    decode = ["aac"]
    for each pair in capsAudioNames()
        if capsYes(device.CanDecodeAudio({ codec: pair[0] }))
            for each name in pair[1]
                if not capsListed(decode, name) then decode.Push(name)
            end for
        end if
    end for
    return decode
end function

' Formats the TV or receiver takes as they are over HDMI. DTS is
' passthrough only on Roku.
function capsPassthrough(device as object) as object
    out = []
    for each codec in ["ac3", "eac3", "dts"]
        if capsYes(device.CanDecodeAudio({ codec: codec, passthru: 1 })) then out.Push(codec)
    end for
    return out
end function

' The most channels the audio output takes: 6 with the surround setting.
function capsChannels(device as object) as integer
    output = device.GetAudioOutputChannel()
    if GetInterface(output, "ifString") <> invalid and Instr(1, LCase(output), "5.1") > 0 then return 6
    return 2
end function

function capsModel(device as object) as string
    name = httpSentence(device.GetModelDisplayName())
    if name = "" then name = httpSentence(device.GetModel())
    return name
end function

' The video mode roDeviceInfo names, such as "1080p", "2160p60b10" or
' "720p50": { width, height, rate }, or invalid.
function capsVideoMode(mode as dynamic) as dynamic
    re = CreateObject("roRegex", "^([0-9]{3,4})[ip]([0-9]{2,3})?", "")
    parts = re.Match(httpString(mode))
    if parts.Count() < 2 then return invalid
    height = parts[1].ToInt()
    widths = { "480": 720, "576": 720, "720": 1280, "1080": 1920, "2160": 3840, "4320": 7680 }
    if not widths.DoesExist(parts[1]) then return invalid
    rate = 60
    if parts.Count() > 2 and parts[2] <> "" then rate = parts[2].ToInt()
    return { width: widths[parts[1]], height: height, rate: rate }
end function
