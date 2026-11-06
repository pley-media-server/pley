sub test_ss_the_search_asks_for_the_file_playing()
    tkEqual(ssSearchPath(12, 7012, ""), "/api/items/12/subtitles/search?file_id=7012")
    tkEqual(ssSearchPath(12, invalid, ""), "/api/items/12/subtitles/search", "the item's first file")
    tkEqual(ssSearchPath(12, 7012, "fre"), "/api/items/12/subtitles/search?file_id=7012&language=fre", "another language")
    tkEqual(ssSearchPath(12, 0, "ger"), "/api/items/12/subtitles/search?language=ger")
end sub

sub test_ss_another_language_is_picked_from_the_list()
    tkEqual(ssLanguageEntry(""), "Language: Your subtitle language")
    tkEqual(ssLanguageEntry("fre"), "Language: French")
    choice = ssLanguageChoices("fre")
    tkEqual(choice.labels[0], "Your subtitle language")
    tkEqual(choice.values[0], "")
    tkEqual(choice.labels[choice.current], "French")
    tkEqual(ssLanguageChoices("").current, 0)
    tkEqual(ssNothingFound(""), "No subtitles were found for this file in your subtitle language. Language picks another.")
    tkEqual(ssNothingFound("ger"), "No subtitles were found for this file in German. Language picks another.")
end sub

sub test_ss_results_are_named_for_the_list()
    data = { results: [
        { id: "opensubtitles:1", provider: "opensubtitles", language: "en", name: "Movie.2019.1080p.BluRay.x264-GRP", format: "srt", hearing_impaired: true, forced: false, downloads: 1234567, matched: "hash" },
        { id: "opensubtitles:2", language: "fre", name: "Movie.2019.WEB", hearing_impaired: false, forced: true, downloads: 1, matched: "title" },
        { id: "", name: "no id" },
        "junk"
    ] }
    found = ssResults(data)
    tkEqual(found.Count(), 2, "only results with an id")
    tkEqual(found[0].id, "opensubtitles:1")
    tkEqual(found[0].label, "English · Movie.2019.1080p.BluRay.x264-GRP")
    tkEqual(found[0].hint, "Movie.2019.1080p.BluRay.x264-GRP" + Chr(10) + "Made for this file · Hearing impaired · 1,234,567 downloads")
    tkEqual(found[1].label, "French · Movie.2019.WEB")
    tkEqual(found[1].hint, "Movie.2019.WEB" + Chr(10) + "Forced · 1 download")
    tkEqual(ssResults(invalid).Count(), 0)
    tkEqual(ssResults({ results: "x" }).Count(), 0)
    tkEqual(ssLabel({}), "Subtitles")
end sub

sub test_ss_words_in_french_and_spanish()
    data = { results: [{ id: "a", language: "en", name: "Movie.WEB", hearing_impaired: true, forced: true, downloads: 1234567, matched: "hash" }, { id: "b", language: "pt-BR", name: "Movie.HDTV", downloads: 0 }] }
    i18nUse("fr")
    found = ssResults(data)
    tkEqual(found[0].label, "Anglais · Movie.WEB")
    tkEqual(found[0].hint, "Movie.WEB" + Chr(10) + "Conçus pour ce fichier · Pour malentendants · Forcés · 1" + Chr(160) + "234" + Chr(160) + "567 téléchargements")
    tkEqual(found[1].hint, "Movie.HDTV" + Chr(10) + "0 téléchargement", "French counts 0 as one")
    tkEqual(ssLanguageEntry(""), "Langue" + Chr(160) + ": Celle de vos sous-titres")
    tkEqual(ssNothingFound("ger"), "Aucun sous-titre trouvé pour ce fichier (Allemand). Changez de langue avec «" + Chr(160) + "Langue" + Chr(160) + "».")
    i18nUse("es")
    found = ssResults(data)
    tkEqual(found[0].hint, "Movie.WEB" + Chr(10) + "Hechos para este archivo · Para personas sordas · Forzados · 1.234.567 descargas")
    tkEqual(found[1].label, "Portugués · Movie.HDTV", "the language before its country")
    tkEqual(ssNothingFound(""), "No se encontraron subtítulos para este archivo (tu idioma de subtítulos). Elige otro idioma en «Idioma».")
    tkEqual(ssProblem({ ok: false, status: 0, data: invalid, error: "network", message: "x" }, true), "No se puede conectar con el servidor.")
end sub

sub test_ss_what_is_said_when_it_cant_look()
    notSetUp = { ok: false, status: 503, data: { code: "unavailable", message: "No OpenSubtitles key", setup: false }, error: "unavailable", message: "No OpenSubtitles key" }
    tkEqual(ssProblem(notSetUp, true), "Subtitle search isn't set up on this server. An administrator sets it up on the web, in Settings → Server.")
    older = { ok: false, status: 404, data: { code: "not_found", message: "nothing at /api/items/1/subtitles/search" }, error: "not_found", message: "Nothing at …" }
    tkEqual(ssProblem(older, true), "Finding subtitles needs a newer Pley server.")
    tkEqual(ssProblem(older, false), "Nothing at …", "a result gone when adding it")
    tkEqual(ssProblem({ ok: false, status: 0, data: invalid, error: "network", message: "x" }, true), "The server can't be reached.")
    tkEqual(ssProblem({ ok: false, status: 502, data: { code: "upstream", message: "OpenSubtitles didn't answer." }, error: "upstream", message: "OpenSubtitles didn't answer." }, false), "OpenSubtitles didn't answer.")
end sub

sub test_ss_the_added_track_joins_the_streams_text_tracks()
    track = ssAddedTrack({ index: 10001, codec: "subrip", language: "eng", title: invalid, default: false, forced: false, external: true, text_based: true, added: true, url: "/api/files/7/subtitles/10001.vtt" })
    tkEqual(track.index, 10001)
    tkEqual(ssAddedTrack({ index: 3, url: invalid }), invalid, "nothing to show")
    tkEqual(ssAddedTrack("x"), invalid)
    subtitles = [{ index: 3, url: "/api/files/7/subtitles/3.vtt" }, { index: 10001, url: "/old" }]
    joined = ssWithTrack(subtitles, track)
    tkEqual(joined.Count(), 2, "once, by its index")
    tkEqual(joined[1].url, "/api/files/7/subtitles/10001.vtt")
    stream = contentStream({ url: "/api/files/7/stream", file_id: 7, subtitles: joined }, { kind: "movie", title: "Heat", files: [{ id: 7, selected_subtitle: 10001 }] }, "http://srv", 0)
    tkEqual(stream.subtitle, "http://srv/api/files/7/subtitles/10001.vtt", "shown as the pick")
    tkEqual(stream.subtitleTracks[1].Description, "English")
end sub
