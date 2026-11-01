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
