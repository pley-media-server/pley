' A title's files and their streams, as docs/API.md's MediaFile describes
' them.

function mediaTestFile() as object
    return ParseJson("{""id"":7,""path"":""Arrival (2016).mkv"",""size"":66571993088,""container"":""mkv"",""duration_ms"":6960000,""bitrate"":61000000,""width"":3840,""height"":2160,""video_codec"":""hevc"",""audio_codec"":""truehd"",""edition"":null,""selected_audio"":1,""selected_subtitle"":null,""audio_picked"":false,""chapters"":[],""streams"":[{""index"":0,""kind"":""video"",""codec"":""hevc"",""language"":null,""title"":null,""default"":true,""forced"":false,""width"":3840,""height"":2160,""dynamic_range"":""dolby_vision"",""dv_profile"":8,""dv_bl_compat"":1},{""index"":1,""kind"":""audio"",""codec"":""truehd"",""language"":""eng"",""title"":""TrueHD Atmos 7.1"",""default"":true,""forced"":false,""channels"":8,""profile"":""Dolby TrueHD + Dolby Atmos""},{""index"":2,""kind"":""audio"",""codec"":""ac3"",""language"":""fre"",""title"":null,""default"":false,""forced"":false,""channels"":6},{""index"":3,""kind"":""audio"",""codec"":""aac"",""language"":""eng"",""title"":""Commentary"",""default"":false,""forced"":false,""channels"":2},{""index"":4,""kind"":""subtitle"",""codec"":""subrip"",""language"":""eng"",""title"":""English SDH"",""default"":false,""forced"":false,""text_based"":true},{""index"":5,""kind"":""subtitle"",""codec"":""hdmv_pgs_subtitle"",""language"":""fre"",""title"":""Forced"",""default"":false,""forced"":true,""text_based"":false},{""index"":1000,""kind"":""subtitle"",""codec"":""subrip"",""language"":null,""title"":""Signs & Songs"",""default"":false,""forced"":false,""external"":true,""text_based"":true}]}")
end function

sub test_media_languages_by_name()
    tkEqual(mediaLanguage("eng"), "English")
    tkEqual(mediaLanguage("fra"), "French", "the ISO 639-2/T code")
    tkEqual(mediaLanguage("ENG"), "English")
    tkEqual(mediaLanguage("pt-BR"), "pt-BR", "a longer tag as it is")
    tkEqual(mediaLanguage("und"), "Unknown")
    tkEqual(mediaLanguage("zxx"), "No linguistic content")
    tkEqual(mediaLanguage("kur"), "KUR", "a code not among the 34, in capitals")
    tkEqual(mediaLanguage(invalid), "")
    tkEqual(mediaLanguage(""), "")
end sub

sub test_media_audio_formats()
    file = mediaTestFile()
    tkEqual(mediaAudioFormat(file.streams[1]), "TrueHD Atmos 7.1", "Atmos from the profile")
    tkEqual(mediaAudioFormat(file.streams[2]), "AC3 5.1")
    tkEqual(mediaAudioFormat(file.streams[3]), "AAC Stereo")
    tkEqual(mediaAudioFormat({ kind: "audio", codec: "dts", channels: 5 }), "DTS 5ch")
    tkEqual(mediaAudioFormat({ kind: "audio", codec: "opus", channel_layout: "quad" }), "Opus quad", "the layout without a count")
    tkEqual(mediaChannels(1, invalid), "Mono")
    tkEqual(mediaChannels(0, invalid), "")
end sub

sub test_media_tracks_named_as_the_other_apps()
    file = mediaTestFile()
    tkEqual(mediaTrackLabel(file.streams[1]), "English – TrueHD Atmos 7.1 (TrueHD Atmos 7.1)", "a title that doesn't name the language")
    tkEqual(mediaTrackLabel(file.streams[2]), "French (AC3 5.1)")
    tkEqual(mediaTrackLabel(file.streams[3]), "English – Commentary (AAC Stereo)")
    tkEqual(mediaTrackLabel(file.streams[4]), "English – SDH (SRT)", "the title's language said once")
    tkEqual(mediaTrackLabel(file.streams[5]), "French (PGS, Forced)", "a title that only says forced")
    tkEqual(mediaTrackLabel(file.streams[6]), "Signs & Songs (SRT, Forced, External)", "signs count as forced")
    tkEqual(mediaTrackLabel({ kind: "audio", codec: "aac", title: "Director" }), "Director (AAC)", "a title without a language")
    tkEqual(mediaTrackLabel({ kind: "subtitle", codec: "ass", language: "eng", title: "English (Forced)", forced: true }), "English (ASS, Forced)")
    tkEqual(mediaTrackLabel({ kind: "subtitle", codec: "subrip" }), "Unknown (SRT)")
end sub

sub test_media_automatic_names_the_language()
    file = mediaTestFile()
    tkEqual(mediaAutomaticLabel(file), "Automatic (English)", "the language of the details' choice")
    file.selected_audio = 2
    tkEqual(mediaAutomaticLabel(file), "Automatic (French)")
    file.selected_audio = invalid
    file.streams[1].default = false
    file.streams[2].default = true
    tkEqual(mediaAutomaticLabel(file), "Automatic (French)", "else the file's default")
    file.streams[2].language = "und"
    tkEqual(mediaAutomaticLabel(file), "Automatic", "no language to name")
end sub

sub test_media_tracks_before_playing()
    file = mediaTestFile()
    tracks = mediaTracks(file)
    tkEqual(tracks.audio.labels, ["Automatic (English)", "English – TrueHD Atmos 7.1 (TrueHD Atmos 7.1)", "French (AC3 5.1)", "English – Commentary (AAC Stereo)"])
    tkEqual(tracks.audio.values, [-1, 1, 2, 3])
    tkEqual(tracks.audio.current, 0, "no track picked: Automatic")
    tkEqual(tracks.subtitles.labels, ["Off", "English – SDH (SRT)", "French (PGS, Forced)", "Signs & Songs (SRT, Forced, External)"])
    tkEqual(tracks.subtitles.values, [-1, 4, 5, 1000])
    tkEqual(tracks.subtitles.current, 0, "off")

    file.audio_picked = true
    file.selected_audio = 3
    file.selected_subtitle = 1000
    tracks = mediaTracks(file)
    tkEqual(tracks.audio.current, 3, "the track picked")
    tkEqual(tracks.subtitles.current, 3)
    file.Delete("audio_picked")
    tkEqual(mediaTracks(file).audio.current, 3, "an older server doesn't say: its choice reads as picked")

    file.streams = [file.streams[0], file.streams[1]]
    tracks = mediaTracks(file)
    tkEqual(tracks.audio.labels, [], "one audio track: nothing to choose")
    tkEqual(tracks.subtitles.labels, [], "no subtitles")
    tkEqual(mediaTracks(invalid).audio.labels, [])
end sub

sub test_media_picks_shown_before_the_server_says()
    item = { id: 12, files: [mediaTestFile()] }
    mediaPick(item, 7, 2, 4)
    tkEqual(item.files[0].selected_audio, 2)
    tkTrue(item.files[0].audio_picked)
    tkEqual(item.files[0].selected_subtitle, 4)
    mediaPick(item, 7, -2, -1)
    tkEqual(item.files[0].selected_audio, 2, "subtitles alone leave the audio")
    tkEqual(item.files[0].selected_subtitle, invalid, "off")
    mediaPick(item, 7, -1, -1)
    tkEqual(item.files[0].selected_audio, invalid, "Automatic")
    tkFalse(item.files[0].audio_picked)
    mediaPick(item, 8, 3, 5)
    tkEqual(item.files[0].selected_subtitle, invalid, "another file's pick")
end sub

sub test_media_sizes_as_the_other_apps()
    tkEqual(mediaBytes(512), "512 B")
    tkEqual(mediaBytes(1536), "1.5 KB")
    tkEqual(mediaBytes(157286400), "150 MB")
    tkEqual(mediaBytes(66571993088&), "62.0 GB", "past 32 bits")
    tkEqual(mediaBytes(1000000), "977 KB", "three digits")
    tkEqual(mediaBytes(1048576), "1.0 MB")
    tkEqual(mediaBytes(invalid), "")
end sub

sub test_media_versions_one_line_each()
    file = mediaTestFile()
    tkEqual(mediaVersionLine(file, 1), "4K · HEVC · DV P8.1 · TrueHD Atmos 7.1 · 62.0 GB", "the default audio: none picked")
    file.edition = "Final Cut"
    file.audio_picked = true
    file.selected_audio = 2
    tkEqual(mediaVersionLine(file, 1), "Final Cut · 4K · HEVC · DV P8.1 · AC3 5.1 · 62.0 GB", "the audio picked")
    tkEqual(mediaVersionLine({ id: 9, size: 0, streams: [] }, 2), "Version 2", "nothing known")
    sd = { id: 8, size: 1536, width: invalid, height: invalid, video_codec: invalid, streams: [{ index: 0, kind: "video", codec: "mpeg2video", width: 720, height: 576 }] }
    tkEqual(mediaVersionLine(sd, 3), "576p · MPEG-2 · 1.5 KB", "the video stream's size and codec when the file doesn't say")

    item = { files: [mediaTestFile(), { id: 8, size: 1073741824, width: 1920, height: 1080, video_codec: "h264", streams: [] }] }
    versions = mediaVersions(item, 8)
    tkEqual(versions.labels, ["4K · HEVC · DV P8.1 · TrueHD Atmos 7.1 · 62.0 GB", "1080p · H.264 · 1.0 GB"])
    tkEqual(versions.values, [7, 8])
    tkEqual(versions.current, 1)
    tkEqual(mediaVersions(item, invalid).current, 0, "the first plays unless another was chosen")
end sub

sub test_media_info_words_as_the_other_apps()
    tkEqual(mediaLevel("h264", 51), "5.1")
    tkEqual(mediaLevel("hevc", 153), "5.1")
    tkEqual(mediaLevel("hevc", 150), "5")
    tkEqual(mediaLevel("av1", 13), "5.1")
    tkEqual(mediaLevel("h264", invalid), "")
    tkEqual(mediaFrameRate(23.976023, false), "23.976 fps")
    tkEqual(mediaFrameRate(24, false), "24 fps")
    tkEqual(mediaFrameRate(29.97, true), "29.97 fps, interlaced")
    tkEqual(mediaFrameRate(0, false), "")
    tkEqual(mediaBitDepth(10, "yuv420p10le"), "10-bit · 4:2:0")
    tkEqual(mediaBitDepth(invalid, "yuv444p"), "4:4:4")
    tkEqual(mediaDimensions(1920, 1080), "1920×1080 (1080p)")
    tkEqual(mediaColour(mediaTransfers(), "arib-std-b67"), "HLG (ARIB STD-B67)")
    tkEqual(mediaColour(mediaPrimaries(), "unknown"), "")
    tkEqual(mediaColour(mediaMatrices(), "odd"), "odd", "as ffprobe names it")
    tkEqual(mediaDolbyVision({ dv_profile: 7, dv_bl_compat: 6, dv_el: "fel" }), "Profile 7 · Blu-ray HDR10-compatible base layer · full enhancement layer")
    tkEqual(mediaDolbyVision({ dv_profile: 5 }), "Profile 5 · no compatible base layer")
    tkEqual(mediaDolbyVision({}), "")
    tkEqual(mediaFileName("/media/movies/Arrival (2016).mkv"), "Arrival (2016).mkv")
    tkEqual(mediaFileName("D:\Movies\Arrival.mkv"), "Arrival.mkv")
end sub

sub test_media_info_of_every_stream()
    file = mediaTestFile()
    video = file.streams[0]
    video.bit_depth = 10
    video.pix_fmt = "yuv420p10le"
    video.frame_rate = 23.976023
    video.profile = "Main 10"
    video.level = 153
    video.dv_level = 6
    video.color_primaries = "bt2020"
    video.color_transfer = "smpte2084"
    video.color_space = "bt2020nc"
    file.streams[1].sample_rate = 48000
    file.streams[1].channel_layout = "7.1"
    file.streams[2].channel_layout = "5.1(side)"
    blocks = mediaFacts(file)
    titles = []
    for each block in blocks
        titles.Push(block.title)
    end for
    tkEqual(titles, ["File", "Video", "Audio 1", "Audio 2", "Audio 3", "Subtitles 1", "Subtitles 2", "Subtitles 3"])
    tkEqual(blocks[0].facts, [
        { label: "Name", value: "Arrival (2016).mkv" },
        { label: "Container", value: "MKV" },
        { label: "Size", value: "62.0 GB" },
        { label: "Duration", value: "1 h 56 min" },
        { label: "Overall bitrate", value: "61 Mbps" }
    ])
    tkEqual(blocks[1].facts, [
        { label: "Codec", value: "HEVC" },
        { label: "Profile", value: "Main 10 · Level 5.1" },
        { label: "Resolution", value: "3840×2160 (4K)" },
        { label: "Frame rate", value: "23.976 fps" },
        { label: "Bit depth", value: "10-bit · 4:2:0" },
        { label: "HDR", value: "Dolby Vision P8.1" },
        { label: "Dolby Vision", value: "Profile 8.1 · Level 6 · HDR10-compatible base layer" },
        { label: "Primaries", value: "BT.2020" },
        { label: "Transfer", value: "PQ (SMPTE ST 2084)" },
        { label: "Matrix", value: "BT.2020 non-constant" }
    ])
    tkEqual(blocks[2].facts, [
        { label: "Codec", value: "TrueHD Atmos" },
        { label: "Profile", value: "Dolby TrueHD + Dolby Atmos" },
        { label: "Channels", value: "7.1" },
        { label: "Sample rate", value: "48 kHz" },
        { label: "Language", value: "English" },
        { label: "Title", value: "TrueHD Atmos 7.1" },
        { label: "Flags", value: "Default" }
    ])
    tkEqual(blocks[3].facts[1], { label: "Channels", value: "5.1 (5.1(side))" }, "a layout of another name")
    tkEqual(blocks[6].facts, [{ label: "Format", value: "PGS, picture" }, { label: "Language", value: "French" }, { label: "Title", value: "Forced" }, { label: "Flags", value: "Forced" }])
    tkEqual(blocks[7].facts, [{ label: "Format", value: "SRT, text" }, { label: "Title", value: "Signs & Songs" }, { label: "Flags", value: "Beside the media" }])
    sdr = mediaStreamFacts({ index: 0, kind: "video", codec: "h264", width: 1920, height: 1080 })
    tkEqual(sdr[2], { label: "HDR", value: "SDR" })
end sub

sub test_media_info_as_text()
    one = { files: [{ id: 1, path: "a.mkv", size: 1536, container: "mkv", streams: [{ index: 0, kind: "audio", codec: "aac", channels: 2 }] }] }
    tkEqual(mediaInfoText(one), "File" + Chr(10) + "Name: a.mkv" + Chr(10) + "Container: MKV" + Chr(10) + "Size: 1.5 KB" + Chr(10) + Chr(10) + "Audio" + Chr(10) + "Codec: AAC" + Chr(10) + "Channels: Stereo")
    two = { files: [one.files[0], { id: 2, path: "b.mkv", edition: "Final Cut", streams: [] }] }
    lines = mediaInfoText(two).Split(Chr(10))
    tkEqual(lines[0], "VERSION 1", "each file under its name when there are several")
    tkEqual(lines[1], "File")
    tkTrue(lines.Count() > 11)
    tkEqual(lines[9], "")
    tkEqual(lines[10], "FINAL CUT", "an edition by its name")
    tkEqual(lines[11], "File")
end sub
