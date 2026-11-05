' Casts from the user's remote, on another of their devices (docs/API.md,
' "Casting"): while its event stream is open the Roku is one of the
' devices the remote plays things on, and plays what it is sent as its own
' Play would, in place of what plays, saying where it came from. The rules
' are the other TV apps'; the scene (MainScene.brs) fetches what a cast
' needs and plays it.

' The event stream's query that makes this Roku a receiver: its id (the
' client_id its players send), the name its owner gave it ("" leaves the
' server to name it, as it names its players), a TV, taking titles and
' playlists always and channels where the profile has Live TV. "" without
' an id.
function castReceiverQuery(id as string, name as string, live as boolean) as string
    if id = "" then return ""
    query = "receiver=" + urlEncode(id)
    if name <> "" then query = query + "&device_name=" + urlEncode(name)
    casts = "item"
    if live then casts = "item,live"
    return query + "&device_kind=" + remoteDeviceKind() + "&casts=" + casts
end function

' Whether this Roku takes casts now: signed in, and past the sign-in and
' "Who's watching?" (routes: the screens of the stack, none at all before
' the first), so a cast never plays past a profile's PIN.
function castTakes(signedIn as boolean, routes as object) as boolean
    if not signedIn or routes.Count() = 0 then return false
    for each route in routes
        if route = "servers" or route = "link" or route = "profiles" then return false
    end for
    return true
end function

' What a cast asks of this Roku, as { act }: "none" (one for another
' device, one without anything to play, or one that comes while casts
' aren't taken: takes false); "item" with itemId, startMs (-1: where Play
' starts) and shuffle; "playlist" with playlistId and shuffle; or
' "channel" with channelId. receiver: this Roku's id; cast: the event's
' fields (receiver_id, item_id, playlist_id, channel_id, start_ms,
' shuffle).
function castAction(receiver as string, takes as boolean, cast as dynamic) as object
    none = { act: "none" }
    data = httpObject(cast)
    if not takes or data = invalid or receiver = "" or httpString(data.receiver_id) <> receiver then return none
    shuffle = httpTrue(data.shuffle)
    itemId = httpNumber(data.item_id, 0)
    if itemId > 0
        startMs = -1
        given = httpNumber(data.start_ms, invalid)
        if given <> invalid
            startMs = Int(given)
            if startMs < 0 then startMs = 0
        end if
        return { act: "item", itemId: itemId, startMs: startMs, shuffle: shuffle }
    end if
    playlistId = httpNumber(data.playlist_id, 0)
    if playlistId > 0 then return { act: "playlist", playlistId: playlistId, shuffle: shuffle }
    channelId = httpNumber(data.channel_id, 0)
    if channelId > 0 then return { act: "channel", channelId: channelId }
    return none
end function

' What says where a cast came from as it starts: "Playing from" and the
' remote's device, "another device" when it has no name.
function castBanner(from as dynamic) as string
    name = httpClean(from, 64, false)
    if name = "" then name = "another device"
    return "Playing from " + name
end function

' Said when a show, an album, an artist or a playlist cast here has
' nothing to play.
function castNothing() as string
    return "Nothing to play"
end function

' What a title cast here plays, as Play on its page does, as { plays }:
' "video" (a movie, an episode or an extra in the player, from startMs,
' -1 where Play starts); "nextEpisode" (a show or a season: its next
' episode); "album" or "artist" (its tracks as the music's queue, with
' shuffle); "track" (its album from it, startMs into it, -1 from its
' start); "nothing" (a kind a newer server knows).
function castItemPlay(kind as string, startMs as integer, shuffle as boolean) as object
    if kind = "movie" or kind = "episode" or kind = "extra" then return { plays: "video", startMs: startMs }
    if kind = "show" or kind = "season" then return { plays: "nextEpisode" }
    if kind = "album" or kind = "artist" then return { plays: kind, shuffle: shuffle }
    if kind = "track" then return { plays: "track", startMs: startMs }
    return { plays: "nothing" }
end function

' Where a video cast here starts: the place sent (startMs), else where its
' page's Play starts it (item: its details), at its place where there is
' one to resume from.
function castVideoStartMs(startMs as integer, item as dynamic) as integer
    if startMs >= 0 then return startMs
    data = httpObject(item)
    if data = invalid then return 0
    state = httpObject(data.user_state)
    position = 0
    if state <> invalid then position = httpNumber(state.position_ms, 0)
    return playerStartOf(position, data.duration_ms)
end function

' Where the track trackId is among its album's tracks (Music.brs's
' musicTracks), -1 when it isn't there.
function castTrackAt(tracks as object, trackId as dynamic) as integer
    for i = 0 to tracks.Count() - 1
        if httpNumber(tracks[i].itemId, -1) = httpNumber(trackId, -2) then return i
    end for
    return -1
end function
