' Find subtitles… in the player's menu (docs/API.md, "Added subtitles"):
' what the server found for the file in the person's subtitle language, or
' another picked there (GET /api/items/{id}/subtitles/search), each named
' for the list, and what to say when it can't look. The one picked is added on the server
' (POST /api/items/{id}/subtitles) and comes back as an ordinary text
' track, which the player shows as it shows the file's own.

' The search's address for a file (0 for the item's first) in language
' (a code of fmtLanguages); for "", the server looks in the person's
' subtitle language, English without one.
function ssSearchPath(itemId as dynamic, fileId as dynamic, language as string) as string
    path = "/api/items/" + urlValue(itemId) + "/subtitles/search"
    query = []
    file = httpNumber(fileId, 0)
    if file > 0 then query.Push("file_id=" + urlValue(file))
    if language <> "" then query.Push("language=" + urlEncode(language))
    if query.Count() > 0 then path = path + "?" + query.Join("&")
    return path
end function

' The list's first entry, which picks the language looked in.
function ssLanguageEntry(language as string) as string
    return t("player.find.languageEntry", { language: fmtLanguageName(language, t("player.find.yourLanguage")) })
end function

' The languages to look in: Your subtitle language (the server's choice),
' then fmtLanguages; current, the one looked in now.
function ssLanguageChoices(language as string) as object
    choice = { labels: [t("player.find.yourLanguage")], values: [""], current: 0 }
    for each known in fmtLanguages()
        if known[0] = language then choice.current = choice.values.Count()
        choice.labels.Push(known[1])
        choice.values.Push(known[0])
    end for
    return choice
end function

' What the list says when nothing was found.
function ssNothingFound(language as string) as string
    return t("player.find.nothingFound", { language: fmtLanguageName(language, t("player.find.yourLanguageInSentence")) })
end function

' The results the list shows, in the server's order (this very file's
' first): [{ id, label, hint }].
function ssResults(data as dynamic) as object
    found = []
    answer = httpObject(data)
    if answer = invalid then return found
    for each result in httpArray(answer.results)
        entry = httpObject(result)
        if entry <> invalid and httpString(entry.id) <> ""
            found.Push({ id: httpString(entry.id), label: ssLabel(entry), hint: ssHint(entry) })
        end if
    end for
    return found
end function

' "English · Movie.2019.1080p.BluRay.x264-GRP": its language, then the
' release it was made for.
function ssLabel(result as object) as string
    parts = []
    language = httpString(result.language)
    if language <> "" then parts.Push(ssLanguageName(language))
    name = httpSentence(result.name)
    if name <> "" then parts.Push(name)
    if parts.Count() = 0 then parts.Push(t("player.find.subtitles"))
    return parts.Join(" · ")
end function

' A result's language by name: OpenSubtitles names them by their two
' letters (ISO 639-1), files by three.
function ssLanguageName(code as string) as string
    two = { ar: "ara", bg: "bul", ca: "cat", zh: "chi", hr: "hrv", cs: "cze", da: "dan", nl: "dut", en: "eng", fi: "fin", fr: "fre", de: "ger", el: "gre", he: "heb", hi: "hin", hu: "hun", id: "ind", it: "ita", ja: "jpn", ko: "kor", no: "nor", fa: "per", pl: "pol", pt: "por", ro: "rum", ru: "rus", sr: "srp", sk: "slo", es: "spa", sv: "swe", th: "tha", tr: "tur", uk: "ukr", vi: "vie" }
    wanted = LCase(code)
    ' "pt-BR", "zh-CN": the language before its country.
    if Len(wanted) > 2 and Mid(wanted, 3, 1) = "-" then wanted = Left(wanted, 2)
    if two.DoesExist(wanted) then return fmtLanguageName(two[wanted], code)
    return fmtLanguageName(code, code)
end function

' Under the list, for the result in focus: its whole name, then "Made for
' this file", "Hearing impaired", "Forced" and how often it was
' downloaded, as they apply.
function ssHint(result as object) as string
    lines = []
    name = httpSentence(result.name)
    if name <> "" then lines.Push(name)
    notes = []
    if httpString(result.matched) = "hash" then notes.Push(t("player.find.madeForFile"))
    if httpTrue(result.hearing_impaired) then notes.Push(t("player.find.hearingImpaired"))
    if httpTrue(result.forced) then notes.Push(t("player.tracks.forced"))
    downloads = httpNumber(result.downloads, -1)
    if downloads >= 0 then notes.Push(t("player.find.downloads", { count: Int(downloads) }))
    if notes.Count() > 0 then lines.Push(notes.Join(" · "))
    return lines.Join(Chr(10))
end function

' What to say when the search or adding a result failed: search not set up
' on the server (503 with setup false), a server from before it (404 on
' the search), the server out of reach, or the server's own words.
function ssProblem(answer as object, searching as boolean) as string
    data = httpObject(answer.data)
    if answer.status = 503 and data <> invalid and GetInterface(data.setup, "ifBoolean") <> invalid and not data.setup
        return t("player.find.notSetUp")
    end if
    if searching and answer.status = 404 then return t("player.find.needsNewer")
    if answer.error = "network" then return t("player.find.unreachable")
    if answer.message <> "" then return answer.message
    return t("player.find.failed")
end function

' The track the server added (201's SubtitleTrack), or invalid when the
' answer isn't one the player can show.
function ssAddedTrack(data as dynamic) as dynamic
    track = httpObject(data)
    if track = invalid then return invalid
    if httpNumber(track.index, -1) < 0 or httpString(track.url) = "" then return invalid
    return track
end function

' The session's subtitles with the added track among them (once, by its index).
function ssWithTrack(subtitles as dynamic, track as object) as object
    out = []
    index = httpNumber(track.index, -1)
    for each entry in httpArray(subtitles)
        data = httpObject(entry)
        if data = invalid or httpNumber(data.index, -2) <> index then out.Push(entry)
    end for
    out.Push(track)
    return out
end function
