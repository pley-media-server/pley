' Search as the other apps have it (the web TV app's search screen,
' Android's SearchViewModel): the results in rows by kind.

' The rows results come in, in order: [{ key, title }]; key is a title's
' kind, "person" or "channel" (Live TV).
function searchGroups() as object
    return [
        { key: "person", title: "People" },
        { key: "channel", title: "Live TV" },
        { key: "movie", title: "Movies" },
        { key: "show", title: "Shows" },
        { key: "season", title: "Seasons" },
        { key: "episode", title: "Episodes" },
        { key: "artist", title: "Artists" },
        { key: "album", title: "Albums" },
        { key: "track", title: "Tracks" }
    ]
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
