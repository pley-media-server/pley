' Casts from the user's remote, on another device (Cast.brs): the event
' stream's query that makes the Roku a receiver, whose cast it is and what
' it names, where it came from, whether it switches to the remote's
' profile first, and what a title of each kind plays.

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
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, start_ms: 60000 })), { act: "item", itemId: 5, startMs: 60000, shuffle: false, audioIndex: -2, subtitleIndex: -2 })
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5 })), { act: "item", itemId: 5, startMs: -1, shuffle: false, audioIndex: -2, subtitleIndex: -2 }, "where Play starts")
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 6, shuffle: true })), { act: "item", itemId: 6, startMs: -1, shuffle: true, audioIndex: -2, subtitleIndex: -2 }, "an album shuffled")
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, start_ms: 0 })), { act: "item", itemId: 5, startMs: 0, shuffle: false, audioIndex: -2, subtitleIndex: -2 }, "from the start")
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, start_ms: -40 })), { act: "item", itemId: 5, startMs: 0, shuffle: false, audioIndex: -2, subtitleIndex: -2 })
    tkEqual(castAction("r-1", true, castTestEvent({ playlist_id: 3, shuffle: true })), { act: "playlist", playlistId: 3, shuffle: true })
    tkEqual(castAction("r-1", true, castTestEvent({ playlist_id: 3 })), { act: "playlist", playlistId: 3, shuffle: false })
    tkEqual(castAction("r-1", true, castTestEvent({ channel_id: 7 })), { act: "channel", channelId: 7 })
end sub

' The tracks the remote picked on its page, with a movie or an episode.
sub test_cast_names_the_tracks_picked()
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, audio_index: 2, subtitle_index: 4 })), { act: "item", itemId: 5, startMs: -1, shuffle: false, audioIndex: 2, subtitleIndex: 4 })
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, audio_index: 1, subtitle_index: -1 })), { act: "item", itemId: 5, startMs: -1, shuffle: false, audioIndex: 1, subtitleIndex: -1 }, "subtitles off")
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, audio_index: invalid, subtitle_index: 3 })), { act: "item", itemId: 5, startMs: -1, shuffle: false, audioIndex: -2, subtitleIndex: 3 }, "the audio not named")
    tkEqual(castAction("r-1", true, castTestEvent({ item_id: 5, audio_index: -1, subtitle_index: -7 })), { act: "item", itemId: 5, startMs: -1, shuffle: false, audioIndex: -2, subtitleIndex: -2 }, "out of range: not named")
end sub

' A cast's tracks play in place of the user's choices where the file the
' cast plays (its first) has them, in the details the player is given;
' one of another version's leaves the choice as it was.
sub test_cast_tracks_go_in_the_files_choice()
    castTestItem = function() as object
        return { id: 5, kind: "movie", files: [
            { id: 50, selected_audio: 1, audio_picked: false, selected_subtitle: 3, streams: [{ kind: "video", index: 0 }, { kind: "audio", index: 1 }, { kind: "audio", index: 2 }, { kind: "subtitle", index: 3 }, { kind: "subtitle", index: 4 }] },
            { id: 51, streams: [{ kind: "video", index: 0 }, { kind: "audio", index: 1 }, { kind: "audio", index: 7 }, { kind: "subtitle", index: 8 }] }
        ] }
    end function
    item = castTestItem()
    tkEqual(castGivenTracks(item, 2, 4), 2)
    tkEqual([item.files[0].selected_audio, item.files[0].audio_picked, item.files[0].selected_subtitle], [2, true, 4])
    item = castTestItem()
    tkEqual(castGivenTracks(item, -2, -1), -1, "the audio not named: the server's pick")
    tkEqual([item.files[0].selected_audio, item.files[0].audio_picked, item.files[0].selected_subtitle], [1, false, invalid], "subtitles off")
    item = castTestItem()
    tkEqual(castGivenTracks(item, 7, 8), -1, "another version's tracks")
    tkEqual([item.files[0].selected_audio, item.files[0].selected_subtitle], [1, 3], "the user's choices stay")
    tkEqual(castGivenTracks({ id: 6, kind: "movie", files: [] }, 1, 3), -1, "no file")
    tkEqual(castGivenTracks(invalid, 1, 3), -1)
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

' A cast from another profile of the household names it and a ticket: the
' Roku switches to it first. One for the profile it is on plays as it is;
' another profile's without a ticket does nothing, as on the other TVs.
sub test_cast_from_another_profile_switches_first()
    nothing = { needs: "nothing" }
    ignore = { needs: "ignore" }
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5 })), nothing, "the remote's profile is this one")
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5, profile_id: 4 })), nothing, "named, and this one")
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5, profile_id: 7, switch_ticket: "Zm9vYmFy_-12" })), { needs: "switch", profileId: 7, ticket: "Zm9vYmFy_-12" })
    tkEqual(castSwitch(0, castTestEvent({ item_id: 5, profile_id: 7, switch_ticket: "t1" })), { needs: "switch", profileId: 7, ticket: "t1" }, "the profile not known yet")
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5, profile_id: 7 })), ignore, "no ticket")
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5, profile_id: 7, switch_ticket: "" })), ignore)
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5, profile_id: 7, switch_ticket: 12 })), ignore, "not a string")
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5, profile_id: 7, switch_ticket: "a b&profile=1" })), ignore, "not a ticket's shape")
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5, profile_id: 7, switch_ticket: String(129, "a") })), ignore, "too long")
    tkEqual(castSwitch(4, castTestEvent({ item_id: 5, profile_id: "7", switch_ticket: "t1" })), nothing, "a profile that isn't a number names none")
    tkEqual(castSwitch(4, invalid), ignore)
end sub

' A switch that failed says why in place of playing, unless the sign-in
' ended (the scene shows sign-in) or changed meanwhile.
sub test_cast_switch_failed_says_why()
    spent = authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""ticket_spent"",""error_description"":""this cast's switch has expired or was used: cast again""}")
    tkEqual(spent, { kind: "refused", reason: "ticket_spent", message: "This cast's switch expired: cast again." })
    tkEqual(castSwitchNotice({ ok: false, error: spent.reason, message: spent.message }), "This cast's switch expired: cast again.")
    tkEqual(castSwitchNotice({ ok: false, error: "not_allowed", message: "You can't switch to this profile." }), "You can't switch to this profile.")
    tkEqual(castSwitchNotice({ ok: false, error: "network", message: "The server can't be reached." }), "The server can't be reached.")
    tkEqual(castSwitchNotice({ ok: false, error: "x", message: "" }), "The server couldn't switch to the remote's profile.")
    tkEqual(castSwitchNotice({ ok: true, data: { through: true } }), "The server couldn't switch to the remote's profile.", "nothing switched")
    tkEqual(castSwitchNotice({ ok: false, error: "signed_out", message: "" }), "")
    tkEqual(castSwitchNotice({ ok: false, error: "server_changed", message: "The sign-in changed." }), "")
    tkEqual(castSwitchNotice({ ok: false, error: "switch_replaced", message: "Another profile was chosen." }), "")
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
