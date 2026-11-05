' Casts from the user's remote, on another device (Cast.brs): the event
' stream's query that makes the Roku a receiver, whose cast it is and what
' it names, where it came from, and what a title of each kind plays.

' A cast as the event stream brings it, for receiver r-1 unless changed.
function castTestEvent(fields as object) as object
    cast = { type: "cast", receiver_id: "r-1", item_id: invalid, playlist_id: invalid, channel_id: invalid, start_ms: invalid, shuffle: false, from: "Living room phone" }
    cast.Append(fields)
    return cast
end function

sub test_cast_receiver_query()
    tkEqual(castReceiverQuery("8f2c-41", "Living room Roku", false), "receiver=8f2c-41&device_name=Living%20room%20Roku&device_kind=tv&casts=item")
    tkEqual(castReceiverQuery("8f2c-41", "Den", true), "receiver=8f2c-41&device_name=Den&device_kind=tv&casts=item,live", "channels where the profile has Live TV")
    tkEqual(castReceiverQuery("8f2c-41", "", false), "receiver=8f2c-41&device_kind=tv&casts=item", "no name: the server names it")
    tkEqual(castReceiverQuery("", "Den", true), "", "no id yet")
end sub

' Casts are taken signed in, past the sign-in and "Who's watching?", so a
' cast never plays past a profile's PIN.
sub test_cast_taken_only_past_the_sign_in_and_the_profiles()
    tkTrue(castTakes(true, ["home"]))
    tkTrue(castTakes(true, ["home", "details", "player"]))
    tkTrue(castTakes(true, ["home", "unreachable"]))
    tkFalse(castTakes(false, ["home"]), "signed out")
    tkFalse(castTakes(true, []), "before the first screen")
    tkFalse(castTakes(true, ["profiles"]), "who's watching, at the start")
    tkFalse(castTakes(true, ["home", "settings", "profiles"]), "who's watching, from Settings")
    tkFalse(castTakes(true, ["servers"]))
    tkFalse(castTakes(true, ["link"]))
    tkFalse(castTakes(true, ["home", "discover", "link"]), "signing in again for requests")
end sub

sub test_cast_for_this_roku()
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, start_ms: 60000 })), { act: "item", itemId: 5, startMs: 60000, shuffle: false })
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5 })), { act: "item", itemId: 5, startMs: -1, shuffle: false }, "where Play starts")
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 6, shuffle: true })), { act: "item", itemId: 6, startMs: -1, shuffle: true }, "an album shuffled")
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, start_ms: 0 })), { act: "item", itemId: 5, startMs: 0, shuffle: false }, "from the start")
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, start_ms: -40 })), { act: "item", itemId: 5, startMs: 0, shuffle: false })
    tkEqual(castAction("r-1", true, castTestEvent({ playlist_id: 3, shuffle: true })), { act: "playlist", playlistId: 3, shuffle: true })
    tkEqual(castAction("r-1", true, castTestEvent({ playlist_id: 3 })), { act: "playlist", playlistId: 3, shuffle: false })
    tkEqual(castAction("r-1", true, castTestEvent({ channel_id: 7 })), { act: "channel", channelId: 7 })
end sub

sub test_cast_for_another_device_or_while_not_taken_does_nothing()
    none = { act: "none" }
    tkEqual(castAction("r-1", true, castTestEvent({ receiver_id: "r-2", item_id: 5 })), none, "another device's")
    tkEqual(castAction("r-1", true, castTestEvent({ receiver_id: "r-2", channel_id: 7 })), none)
    tkEqual(castAction("r-1", false, castTestEvent({ item_id: 5 })), none, "signed out, or who's watching")
    tkEqual(castAction("r-1", true, castTestEvent({})), none, "nothing named")
    tkEqual(castAction("", true, castTestEvent({ receiver_id: "", item_id: 5 })), none, "no id of its own")
    tkEqual(castAction("r-1", true, invalid), none)
end sub

sub test_cast_says_where_it_came_from()
    tkEqual(castBanner("Living room phone"), "Playing from Living room phone")
    tkEqual(castBanner("Firefox on Windows"), "Playing from Firefox on Windows")
    tkEqual(castBanner(invalid), "Playing from another device")
    tkEqual(castBanner("  "), "Playing from another device")
    tkEqual(castBanner("Den" + Chr(10) + "phone"), "Playing from Den phone", "on one line")
    tkEqual(castNothing(), "Nothing to play")
end sub

' What each kind plays, as Play on its page does.
sub test_cast_plays_each_kind_as_its_play()
    tkEqual(castItemPlay("movie", 60000, false), { plays: "video", startMs: 60000 })
    tkEqual(castItemPlay("movie", -1, false), { plays: "video", startMs: -1 })
    tkEqual(castItemPlay("episode", 0, false), { plays: "video", startMs: 0 })
    tkEqual(castItemPlay("extra", -1, false), { plays: "video", startMs: -1 })
    tkEqual(castItemPlay("show", -1, false), { plays: "nextEpisode" })
    tkEqual(castItemPlay("season", 60000, true), { plays: "nextEpisode" }, "a place and shuffle mean nothing to a season")
    tkEqual(castItemPlay("album", -1, false), { plays: "album", shuffle: false })
    tkEqual(castItemPlay("album", -1, true), { plays: "album", shuffle: true })
    tkEqual(castItemPlay("artist", -1, true), { plays: "artist", shuffle: true })
    tkEqual(castItemPlay("track", 30000, true), { plays: "track", startMs: 30000 })
    tkEqual(castItemPlay("track", -1, false), { plays: "track", startMs: -1 })
    tkEqual(castItemPlay("photo", -1, false), { plays: "nothing" }, "a kind a newer server knows")
end sub

' A video starts where it was sent, else where its page's Play starts it:
' at its place, where there is one to resume from.
sub test_cast_video_starts_where_play_does()
    halfway = { id: 5, kind: "movie", duration_ms: 6000000, user_state: { position_ms: 3000000, watched: false } }
    tkEqual(castVideoStartMs(60000, halfway), 60000, "the place sent")
    tkEqual(castVideoStartMs(0, halfway), 0, "from the start, sent")
    tkEqual(castVideoStartMs(-1, halfway), 3000000, "Resume")
    nearly = { id: 5, kind: "movie", duration_ms: 6000000, user_state: { position_ms: 5900000, watched: true } }
    tkEqual(castVideoStartMs(-1, nearly), 0, "watched to its end: from the start")
    tkEqual(castVideoStartMs(-1, { id: 6, kind: "episode", duration_ms: 1800000 }), 0, "never played")
    tkEqual(castVideoStartMs(-1, invalid), 0)
end sub

sub test_cast_track_among_its_albums()
    tracks = [{ itemId: 51 }, { itemId: 52 }, { itemId: 53 }]
    tkEqual(castTrackAt(tracks, 52), 1)
    tkEqual(castTrackAt(tracks, 54), -1)
    tkEqual(castTrackAt([], 52), -1)
end sub
