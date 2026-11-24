sub test_tracks_defaults_from_the_profile()
    tkEqual(tracksDefaults({ name: "Anna", audio_language: "jpn", subtitle_language: invalid, subtitle_mode: "foreign" }), { audio: "jpn", subtitle: "", mode: "foreign" })
    tkEqual(tracksDefaults({ audio_language: invalid, subtitle_language: "eng", subtitle_mode: "always" }), { audio: "", subtitle: "eng", mode: "always" })
    tkEqual(tracksDefaults({ subtitle_mode: "sometimes" }), { audio: "", subtitle: "", mode: "forced" }, "a mode this app doesn't know: the server's default")
    tkEqual(tracksDefaults(invalid), { audio: "", subtitle: "", mode: "forced" })
end sub

sub test_tracks_defaults_bodies_send_all_three()
    tkEqual(tracksDefaultsBody({ audio: "jpn", subtitle: "eng", mode: "foreign" }), { audio_language: "jpn", subtitle_language: "eng", subtitle_mode: "foreign" })
    body = tracksDefaultsBody({ audio: "", subtitle: "", mode: "off" })
    tkTrue(body.DoesExist("audio_language"), "the language left to the file is sent")
    tkEqual(FormatJson(body), "{""audio_language"":null,""subtitle_language"":null,""subtitle_mode"":""off""}")
end sub

sub test_tracks_changes_one_default()
    before = { audio: "", subtitle: "", mode: "forced" }
    after = tracksWith(before, "mode", "always")
    tkEqual(after, { audio: "", subtitle: "", mode: "always" })
    tkEqual(before.mode, "forced", "the defaults before stay as they were")
    tkFalse(tracksSame(before, after))
    tkTrue(tracksSame(before, tracksWith(after, "mode", "forced")))
    tkEqual(tracksWith(before, "audio", "fre").audio, "fre")
    tkEqual(tracksWith(before, "subtitle", "ger").subtitle, "ger")
end sub

sub test_tracks_refusals_send_to_the_web()
    tkTrue(tracksRefused(403))
    tkTrue(tracksRefused(404), "a server from before")
    tkTrue(tracksRefused(405))
    tkFalse(tracksRefused(400))
    tkFalse(tracksRefused(500))
    tkFalse(tracksRefused(0))
end sub

sub test_tracks_settings_rows()
    tkEqual(tracksRows(invalid, false), [{ kind: "audio", label: "Audio language", value: "" }, { kind: "mode", label: "Subtitles", value: "" }], "before the profile came")
    tkEqual(tracksRows({ audio: "", subtitle: "", mode: "forced" }, false), [
        { kind: "audio", label: "Audio language", value: "The file's default" },
        { kind: "mode", label: "Subtitles", value: "Only for foreign dialogue" }
    ])
    tkEqual(tracksRows({ audio: "jpn", subtitle: "", mode: "foreign" }, false), [
        { kind: "audio", label: "Audio language", value: "Japanese" },
        { kind: "mode", label: "Subtitles", value: "When the audio is in another language" },
        { kind: "subtitle", label: "Subtitle language", value: "Same as the audio language" }
    ], "the subtitle language for foreign")
    rows = tracksRows({ audio: "fr", subtitle: "eng", mode: "always" }, true)
    tkEqual(rows[0].value, "fr · change on the web", "a code not offered shows as it is")
    tkEqual(rows[2].value, "English · change on the web")
    tkEqual(tracksRows({ audio: "", subtitle: "eng", mode: "off" }, false).Count(), 2, "no subtitle language when they stay off")
end sub

sub test_tracks_settings_choices()
    defaults = { audio: "eng", subtitle: "", mode: "always" }
    audio = tracksChoices("audio", defaults)
    tkEqual(audio.title, "Audio language")
    tkEqual(audio.labels.Count(), 35, "the file's default and 34 languages")
    tkEqual(audio.labels[0], "The file's default")
    tkEqual(audio.values[0], "")
    tkEqual(audio.values[audio.current], "eng")
    tkEqual(audio.labels[audio.current], "English")
    tkEqual(audio.hints, [])
    modes = tracksChoices("mode", defaults)
    tkEqual(modes.title, "Subtitles")
    tkEqual(modes.values, ["off", "forced", "foreign", "always"])
    tkEqual(modes.labels, ["Off", "Only for foreign dialogue", "When the audio is in another language", "Always"])
    tkEqual(modes.hints[0], "Subtitles stay off until you turn them on.")
    tkEqual(modes.current, 3)
    subtitles = tracksChoices("subtitle", defaults)
    tkEqual(subtitles.title, "Subtitle language")
    tkEqual(subtitles.labels[0], "Same as the audio language")
    tkEqual(subtitles.current, 0)
    tkEqual(tracksChoices("audio", { audio: "fr", subtitle: "", mode: "off" }).current, -1, "a code not offered")
end sub

' Settings' rows say their value in fewer words where the language needs
' them to fit; the choices say it in full.
sub test_tracks_settings_in_french_and_spanish()
    i18nUse("fr")
    tkEqual(tracksRows({ audio: "", subtitle: "", mode: "foreign" }, false), [
        { kind: "audio", label: "Langue audio", value: "Celle du fichier" },
        { kind: "mode", label: "Sous-titres", value: "Audio dans une autre langue" },
        { kind: "subtitle", label: "Langue des sous-titres", value: "Celle de l’audio" }
    ])
    tkEqual(tracksRows({ audio: "jpn", subtitle: "", mode: "off" }, true)[0].value, "Japonais · à modifier sur le web")
    modes = tracksChoices("mode", { audio: "", subtitle: "", mode: "forced" })
    tkEqual(modes.title, "Sous-titres")
    tkEqual(modes.labels[1], "Seulement pour les dialogues étrangers")
    tkEqual(tracksChoices("audio", { audio: "", subtitle: "", mode: "off" }).labels[0], "Celle du fichier par défaut")
    i18nUse("es")
    tkEqual(tracksRows({ audio: "", subtitle: "", mode: "forced" }, false)[1].value, "Solo diálogos en otro idioma")
    subtitles = tracksChoices("subtitle", { audio: "", subtitle: "ger", mode: "always" })
    tkEqual(subtitles.title, "Idioma de los subtítulos")
    tkEqual(subtitles.labels[0], "El mismo que el del audio")
    tkEqual(subtitles.labels[subtitles.current], "Alemán")
    tkEqual(tracksModeLabel("always"), "Siempre")
end sub

sub test_tracks_languages_are_the_other_apps()
    languages = fmtLanguages()
    tkEqual(languages.Count(), 34)
    tkEqual(languages[0], ["ara", "Arabic"])
    tkEqual(languages[33], ["vie", "Vietnamese"])
    tkEqual(fmtLanguageName("ger", "-"), "German")
    tkEqual(fmtLanguageName("", "None"), "None")
    tkEqual(tracksModeLabel("off"), "Off")
    tkEqual(tracksModeLabel("what"), "")
    tkTrue(tracksAsksSubtitleLanguage("foreign"))
    tkTrue(tracksAsksSubtitleLanguage("always"))
    tkFalse(tracksAsksSubtitleLanguage("forced"))
    tkFalse(tracksAsksSubtitleLanguage("off"))
end sub

' The languages sort as their names read, accents and all: "Hébreu" before
' "Hindi" in French, "Árabe" among the A's in Spanish.
sub test_tracks_languages_sort_in_the_language_shown()
    i18nUse("fr")
    tkEqual(tracksTestNames(fmtLanguages()), ["Allemand", "Anglais", "Arabe", "Bulgare", "Catalan", "Chinois", "Coréen", "Croate",
        "Danois", "Espagnol", "Finnois", "Français", "Grec", "Hébreu", "Hindi", "Hongrois", "Indonésien", "Italien", "Japonais",
        "Néerlandais", "Norvégien", "Persan", "Polonais", "Portugais", "Roumain", "Russe", "Serbe", "Slovaque", "Suédois",
        "Tchèque", "Thaï", "Turc", "Ukrainien", "Vietnamien"])
    i18nUse("es")
    tkEqual(tracksTestNames(fmtLanguages()), ["Alemán", "Árabe", "Búlgaro", "Catalán", "Checo", "Chino", "Coreano", "Croata",
        "Danés", "Eslovaco", "Español", "Finés", "Francés", "Griego", "Hebreo", "Hindi", "Húngaro", "Indonesio", "Inglés",
        "Italiano", "Japonés", "Neerlandés", "Noruego", "Persa", "Polaco", "Portugués", "Rumano", "Ruso", "Serbio", "Sueco",
        "Tailandés", "Turco", "Ucraniano", "Vietnamita"])
end sub

function tracksTestNames(languages as object) as object
    names = []
    for each language in languages
        names.Push(language[1])
    end for
    return names
end function

function tracksTestSubtitles() as object
    return [
        { index: 3, language: "eng", url: "/api/files/7/subtitles/3.vtt" },
        { index: 4, language: "fre", url: invalid, text_based: false },
        { index: 13, language: "ger", url: "/api/files/7/subtitles/13.vtt" },
        { index: 1000, language: "spa", external: true, url: "/api/files/7/subtitles/1000.vtt" }
    ]
end function

sub test_tracks_subtitle_shown_from_the_track_address()
    server = "http://nas:7539"
    subs = tracksTestSubtitles()
    tkEqual(tracksSubtitleShown("On", "http://nas:7539/api/files/7/subtitles/3.vtt", subs, server), 3)
    tkEqual(tracksSubtitleShown("On", "http://nas:7539/api/files/7/subtitles/13.vtt", subs, server), 13, "not taken for 3")
    tkEqual(tracksSubtitleShown("On", "http://nas:7539/api/files/7/subtitles/1000.vtt", subs, server), 1000, "a file beside the video")
    tkEqual(tracksSubtitleShown("On", "http://nas:7539/api/files/7/subtitles/13.vtt?offset_ms=300", subs, server), 13, "moved by subtitle sync")
    tkEqual(tracksSubtitleShown("On", syncTrackUrl("http://nas:7539/api/files/7/subtitles/3.vtt", -1200), subs, server), 3)
    tkEqual(tracksSubtitleShown("On", "https://other.example/api/files/7/subtitles/3.vtt", subs, server), 3, "the same path under another address")
    tkEqual(tracksSubtitleShown("on", "http://nas:7539/api/files/7/subtitles/3.vtt", subs, server), 3)
end sub

sub test_tracks_subtitles_off_or_unknown()
    server = "http://nas:7539"
    subs = tracksTestSubtitles()
    tkEqual(tracksSubtitleShown("Off", "http://nas:7539/api/files/7/subtitles/3.vtt", subs, server), -1, "captions off")
    tkEqual(tracksSubtitleShown("On", "", subs, server), -1, "no track")
    tkEqual(tracksSubtitleShown("When mute", "http://nas:7539/api/files/7/subtitles/3.vtt", subs, server), -2, "only on mute")
    tkEqual(tracksSubtitleShown("Instant replay", "http://nas:7539/api/files/7/subtitles/3.vtt", subs, server), -2)
    tkEqual(tracksSubtitleShown("", "http://nas:7539/api/files/7/subtitles/3.vtt", subs, server), -2)
    tkEqual(tracksSubtitleShown("On", "eia608/1", subs, server), -2, "captions in the video")
    tkEqual(tracksSubtitleShown("On", "http://nas:7539/api/files/7/subtitles/9.vtt", subs, server), -2)
    tkEqual(tracksSubtitleShown("On", "http://nas:7539/api/files/7/subtitles/3.vtt", invalid, server), -2)
end sub

function tracksTestFile() as object
    return {
        id: 7,
        selected_audio: invalid,
        selected_subtitle: 3,
        streams: [
            { index: 0, kind: "video" },
            { index: 1, kind: "audio", language: "eng", default: false },
            { index: 2, kind: "audio", language: "jpn", default: true },
            { index: 3, kind: "subtitle", language: "eng" }
        ]
    }
end function

sub test_tracks_audio_playing()
    file = tracksTestFile()
    tkEqual(tracksAudioPlaying({ audio_index: 1 }, file), 1, "the session's")
    tkEqual(tracksAudioPlaying({ audio_index: invalid }, file), 2, "the file's default")
    file.selected_audio = 1
    tkEqual(tracksAudioPlaying({ audio_index: invalid }, file), 1, "the person's choices'")
    file.selected_audio = invalid
    file.streams[2].default = false
    tkEqual(tracksAudioPlaying(invalid, file), 1, "none marked: the first")
    tkEqual(tracksAudioPlaying(invalid, { streams: [{ index: 0, kind: "video" }] }), -1, "no audio")
    tkEqual(tracksAudioPlaying(invalid, invalid), -1)
end sub

sub test_tracks_audio_shown_from_rokus_tracks()
    file = tracksTestFile()
    tracks = [{ Language: "eng", Name: "English", Track: "1" }, { Language: "jpn", Name: "Japanese", Track: "2" }]
    tkEqual(tracksAudioShown(tracks, "2", file), 2)
    tkEqual(tracksAudioShown(tracks, "1", file), 1)
    tkEqual(tracksAudioShown([{ Language: "", Track: "a" }, { Language: "und", Track: "b" }], "b", file), 2, "languages unknown")
    tkEqual(tracksAudioShown([{ Language: "eng", Track: 1 }, { Language: "jpn", Track: 2 }], "2", file), 2, "numbers for Track")
    tkEqual(tracksTrackId(2), "2")
    tkEqual(tracksTrackId("x"), "x")
end sub

sub test_tracks_audio_not_told_when_it_cant_be()
    file = tracksTestFile()
    tracks = [{ Language: "eng", Track: "1" }, { Language: "jpn", Track: "2" }]
    tkEqual(tracksAudioShown([{ Language: "eng", Track: "1" }], "1", file), -2, "fewer tracks than the file has")
    tkEqual(tracksAudioShown([{ Language: "jpn", Track: "1" }, { Language: "eng", Track: "2" }], "2", file), -2, "languages that disagree")
    tkEqual(tracksAudioShown(tracks, "", file), -2, "none playing")
    tkEqual(tracksAudioShown(tracks, "9", file), -2, "a track not listed")
    tkEqual(tracksAudioShown(invalid, "1", file), -2)
    tkEqual(tracksAudioShown(tracks, "1", invalid), -2)
end sub

sub test_tracks_streams_bodies()
    tkEqual(FormatJson(tracksStreamsBody(7, 2, 13)), "{""audio_index"":2,""file_id"":7,""subtitle_index"":13}")
    tkEqual(FormatJson(tracksStreamsBody(7, 2, -1)), "{""audio_index"":2,""file_id"":7,""subtitle_index"":null}", "subtitles off")
    tkEqual(FormatJson(tracksStreamsBody(7, -1, 1000)), "{""audio_index"":null,""file_id"":7,""subtitle_index"":1000}", "the default audio")
    tkFalse(tracksStreamsBody(invalid, 1, 3).DoesExist("file_id"), "the item's first file")
    tkEqual(FormatJson(tracksStreamsBody(7, -2, 13)), "{""file_id"":7,""subtitle_index"":13}", "subtitles alone: the audio left as picked before")
end sub

sub test_tracks_new_session_only_away_from_drawn_pictures()
    tkTrue(tracksNeedNewSession(4, 3), "text subtitles over drawn pictures")
    tkTrue(tracksNeedNewSession(4, -1), "subtitles off")
    tkFalse(tracksNeedNewSession(4, 4))
    tkFalse(tracksNeedNewSession(-1, 3), "nothing drawn")
end sub
