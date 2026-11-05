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
' starts), shuffle, and the tracks the remote picked, audioIndex and
' subtitleIndex (-1: subtitles off; -2: not named); "playlist" with
' playlistId and shuffle; or "channel" with channelId. receiver: this
' Roku's id; cast: the event's fields (receiver_id, item_id, playlist_id,
' channel_id, start_ms, shuffle, audio_index, subtitle_index).
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
        audio = Int(httpNumber(data.audio_index, -2))
        if audio < 0 then audio = -2
        subtitle = Int(httpNumber(data.subtitle_index, -2))
        if subtitle < -1 then subtitle = -2
        return { act: "item", itemId: itemId, startMs: startMs, shuffle: shuffle, audioIndex: audio, subtitleIndex: subtitle }
    end if
    playlistId = httpNumber(data.playlist_id, 0)
    if playlistId > 0 then return { act: "playlist", playlistId: playlistId, shuffle: shuffle }
    channelId = httpNumber(data.channel_id, 0)
    if channelId > 0 then return { act: "channel", channelId: channelId }
    return none
end function

' What a cast asks of the profile this Roku is on (profileId; 0 while it
' isn't known), as { needs } (docs/API.md, "Switching to the remote's
' profile"): "nothing" (it names no profile, or this one); "switch" to the
' remote's (profileId), with the ticket the switch-profile grant takes in
' place of a PIN (ticket); "ignore" (another profile's without a ticket,
' or one that isn't a ticket's shape), as the other TVs do.
function castSwitch(profileId as integer, cast as dynamic) as object
    data = httpObject(cast)
    if data = invalid then return { needs: "ignore" }
    target = Int(httpNumber(data.profile_id, 0))
    if target <= 0 or target = profileId then return { needs: "nothing" }
    ticket = httpString(data.switch_ticket)
    if not CreateObject("roRegex", "^[A-Za-z0-9_-]{1,128}$", "").IsMatch(ticket) then return { needs: "ignore" }
    return { needs: "switch", profileId: target, ticket: ticket }
end function

' What a cast's switch that failed says, in place of playing (answer: the
' API task's to the switch, { ok, error, message }): why, briefly; "" when
' the scene says it otherwise (signed out) or the sign-in changed
' meanwhile.
function castSwitchNotice(answer as object) as string
    error = httpString(answer.error)
    if error = "signed_out" or error = "switch_replaced" or error = "server_changed" then return ""
    message = httpSentence(answer.message)
    if httpTrue(answer.ok) or message = "" then message = "The server couldn't switch to the remote's profile."
    return message
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

' The tracks a cast named for a movie or an episode (audioIndex,
' subtitleIndex: stream indexes, -1 for subtitles off, -2 not named), as
' the player plays them for this play, in place of the user's choices:
' where the file it plays (the item's first, as for any cast) has them; a
' track it doesn't have (one of another version) leaves the choice to
' them. They go in that file's choice (selected_subtitle, selected_audio)
' of item, the details the player is given, which it shows and ticks them
' from; nothing is saved. The audio to ask the server for (the play
' request's audio_index), -1 to leave it to the server.
function castGivenTracks(item as dynamic, audioIndex as integer, subtitleIndex as integer) as integer
    data = httpObject(item)
    if data = invalid then return -1
    file = contentFile(data, invalid)
    if file = invalid then return -1
    if subtitleIndex = -1
        file.selected_subtitle = invalid
    else if subtitleIndex >= 0 and castFileHas(file, "subtitle", subtitleIndex)
        file.selected_subtitle = subtitleIndex
    end if
    if audioIndex < 0 or not castFileHas(file, "audio", audioIndex) then return -1
    file.selected_audio = audioIndex
    file.audio_picked = true
    return audioIndex
end function

' Whether a file's streams have one of kind ("audio", "subtitle") at index.
function castFileHas(file as object, kind as string, index as integer) as boolean
    for each stream in httpArray(file.streams)
        s = httpObject(stream)
        if s <> invalid and httpString(s.kind) = kind and httpNumber(s.index, -9) = index then return true
    end for
    return false
end function

' Where the track trackId is among its album's tracks (Music.brs's
' musicTracks), -1 when it isn't there.
function castTrackAt(tracks as object, trackId as dynamic) as integer
    for i = 0 to tracks.Count() - 1
        if httpNumber(tracks[i].itemId, -1) = httpNumber(trackId, -2) then return i
    end for
    return -1
end function
