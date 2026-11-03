' Search as the other apps have it (the web TV app's search screen,
' Android's SearchViewModel): the results in rows by kind, and the recent
' searches kept on the Roku.

' The rows results come in, in order: [{ key, title }]; key is a title's
' kind, "channel" (Live TV) or "person". The libraries' titles come first,
' then Live TV, then people; the server puts the closest matches first in
' each.
function searchGroups() as object
    return [
        { key: "movie", title: "Movies" },
        { key: "show", title: "Shows" },
        { key: "season", title: "Seasons" },
        { key: "episode", title: "Episodes" },
        { key: "artist", title: "Artists" },
        { key: "album", title: "Albums" },
        { key: "track", title: "Tracks" },
        { key: "channel", title: "Live TV" },
        { key: "person", title: "People" }
    ]
end function

' GET /api/search for query: up to 200 titles, 200 people and 200 Live TV
' channels, the most the server gives, so every row holds all it found.
function searchPath(query as string) as string
    return urlWithQuery("/api/search", { q: query, limit: 200 })
end function

' GET /api/search's answer as a row per kind with results, in
' searchGroups' order: [{ key, title, wide, square, tiles }]. Episodes are
' wide, music square, Live TV's channels cards (Live.brs's liveCard; now
' and twelve say what's on and the clock).
function searchRows(data as dynamic, server as string, now as integer, twelve as boolean) as object
    answer = httpObject(data)
    if answer = invalid then answer = {}
    byKind = {}
    for each group in searchGroups()
        byKind[group.key] = []
    end for
    for each person in httpArray(answer.people)
        if GetInterface(person, "ifAssociativeArray") <> invalid and httpNumber(person.id, 0) > 0 then byKind.person.Push(pagePersonTile(person, server, ""))
    end for
    for each channel in liveChannels(answer.channels, server)
        byKind.channel.Push(liveCard(channel, now, twelve))
    end for
    for each item in httpArray(answer.items)
        if GetInterface(item, "ifAssociativeArray") <> invalid
            kind = httpString(item.kind)
            if contentShown(kind) then byKind[kind].Push(contentTile(item, server, kind = "episode"))
        end if
    end for
    rows = []
    for each group in searchGroups()
        tiles = byKind[group.key]
        if tiles.Count() > 0 then rows.Push({ key: group.key, title: group.title, wide: group.key = "episode", square: contentMusic(group.key), tiles: tiles })
    end for
    return rows
end function

' The size of a row's tiles, and the row's height, beside the keyboard: a
' channel's card, a 16:9 picture, a square or a poster, with their lines.
function searchRowSize(key as string) as object
    if key = "channel" then return { size: [220, 240], height: 300 }
    if key = "episode" then return { size: [300, 250], height: 310 }
    if contentMusic(key) then return { size: [196, 280], height: 340 }
    return { size: [196, 370], height: 430 }
end function

' --- Recent searches ---

' The last 10 searches whose results were wanted, newest first, kept on
' the Roku for each server and profile.

function searchRecentKey(server as string, profileId as dynamic) as string
    return server + "|" + urlValue(profileId)
end function

function searchRecentLoad(section as object, key as string) as object
    recent = []
    for each query in httpArray(httpJson(section.Read(key)))
        text = httpSentence(query)
        if text <> "" and recent.Count() < 10 then recent.Push(text)
    end for
    return recent
end function

sub searchRecentSave(section as object, key as string, recent as object)
    if recent.Count() = 0
        section.Delete(key)
    else
        section.Write(key, FormatJson(recent))
    end if
    section.Flush()
end sub

' recent with query at the top, the same words in other letters' case
' taken out, 10 at most.
function searchRemember(recent as object, query as string) as object
    text = query.Trim()
    if text = "" then return recent
    out = [text]
    for each earlier in recent
        if LCase(earlier) <> LCase(text) and out.Count() < 10 then out.Push(earlier)
    end for
    return out
end function
