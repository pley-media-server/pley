' Being cast to: this Roku's id and name, the event stream that lists it,
' the commands that come on it, and what the player tells back.

sub test_cast_device_id_is_made_once_and_kept()
    section = tkRegistrySection()
    id = castDeviceId(section, "0B7C4D1E-9A2F-4E3B-8C5D-6F7A8B9C0D1E")
    tkEqual(id, "0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e", "32 hex digits")
    tkEqual(section.Read("cast"), id)
    tkEqual(section.flushes, 1)
    tkEqual(castDeviceId(section, "11111111-2222-3333-4444-555555555555"), id, "kept, not made again")
    tkEqual(section.flushes, 1)
    section.Write("cast", "bad id")
    tkEqual(castDeviceId(section, "11111111-2222-3333-4444-555555555555"), "11111111222233334444555555555555", "one the server wouldn't take is made again")
    tkTrue(castValidId("device-tv_1"))
    tkFalse(castValidId("short"))
    tkFalse(castValidId(String(65, "a")))
    tkFalse(castValidId("not/ok-at-all"))
end sub

sub test_cast_name_is_the_rokus_own()
    tkEqual(castName("Living room Roku", "Roku Ultra"), "Living room Roku")
    tkEqual(castName("", "Roku Ultra"), "Roku Ultra", "no name given: the model's")
    tkEqual(castName("  ", invalid), "Roku")
    tkEqual(castName(invalid, ""), "Roku")
    tkEqual(castName("Den" + Chr(10) + "TV", ""), "Den TV", "no control characters")
    tkEqual(Len(castName(String(80, "x"), "")), 64, "64 characters at most")
end sub

sub test_cast_stream_lists_the_roku()
    crlf = Chr(13) + Chr(10)
    cast = { device: "0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e", name: "Living room Roku" }
    tkEqual(castQuery(cast), "?cast_device=0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e&cast_name=Living%20room%20Roku&cast_platform=roku")
    request = evRequest("http://nas:7539", "pley_at_1", "ua", cast)
    tkEqual(request.text.Split(crlf)[0], "GET /api/events?cast_device=0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e&cast_name=Living%20room%20Roku&cast_platform=roku HTTP/1.1")
    tkEqual(evRequest("http://nas:7539", "pley_at_1", "ua").text.Split(crlf)[0], "GET /api/events HTTP/1.1", "without cast")
    tkEqual(castQuery(invalid), "")
    tkEqual(castQuery({ device: "", name: "Roku" }), "", "no id yet")
    tkEqual(castQuery({ device: "0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e", name: "" }), "", "no name")
    tkEqual(castQuery({ device: "0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e", name: "Salle de jeux · TV" }), "?cast_device=0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e&cast_name=Salle%20de%20jeux%20%C2%B7%20TV&cast_platform=roku", "UTF-8, encoded")
    tkTrue(evCastRefused(400), "a name or id the server won't take")
    tkTrue(evCastRefused(403), "a sign-in without play")
    tkTrue(evCastRefused(429), "the profile's devices full")
    tkFalse(evCastRefused(401), "the token ran out: renewed, as ever")
    tkFalse(evCastRefused(503))
end sub

sub test_cast_play_from_the_stream()
    crlf = Chr(13) + Chr(10)
    own = "0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e"
    reader = evReader()
    body = "data: {""type"":""cast_play"",""device_id"":""" + own + """,""item_id"":412,""file_id"":null,""position_ms"":null}" + crlf + crlf
    events = evRead(reader, eventsTestBytes("HTTP/1.1 200 OK" + crlf + crlf + body))
    tkEqual(events.Count(), 1)
    command = castCommand(events[0], own)
    tkEqual(command, { kind: "play", itemId: 412, startMs: -1 }, "no position: where the profile left off")
    tkEqual(castPlayerParams(command), { itemId: 412, startMs: -1 })
    command = castCommand({ type: "cast_play", device_id: own, item_id: 412, file_id: 77, position_ms: 600000 }, own)
    tkEqual(command, { kind: "play", itemId: 412, startMs: 600000, fileId: 77 })
    tkEqual(castPlayerParams(command), { itemId: 412, startMs: 600000, fileId: 77 }, "that version")
    tkEqual(castCommand({ type: "cast_play", device_id: own, item_id: 412, position_ms: 0 }, own).startMs, 0, "0: from the start")
    tkInvalid(castCommand({ type: "cast_play", device_id: "another-device", item_id: 412 }, own), "another device's")
    tkInvalid(castCommand({ type: "cast_play", device_id: own, item_id: 412 }, ""), "not listed")
    tkInvalid(castCommand({ type: "cast_play", device_id: own, item_id: invalid }, own), "nothing to play")
    tkInvalid(castCommand({ type: "cast_state", device_id: own, state: invalid }, own), "what a device plays isn't a command")
    tkInvalid(castCommand({ type: "user_state_changed", item_ids: [1] }, own))
end sub

sub test_cast_control_from_the_stream()
    own = "0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e"
    tkEqual(castCommand({ type: "cast_control", device_id: own, action: "pause", position_ms: invalid }, own), { kind: "control", action: "pause", positionMs: -1 })
    tkEqual(castCommand({ type: "cast_control", device_id: own, action: "resume", position_ms: 5000 }, own), { kind: "control", action: "resume", positionMs: -1 }, "a position only for a seek")
    tkEqual(castCommand({ type: "cast_control", device_id: own, action: "seek", position_ms: 1834500 }, own), { kind: "control", action: "seek", positionMs: 1834500 })
    tkEqual(castCommand({ type: "cast_control", device_id: own, action: "stop", position_ms: invalid }, own).action, "stop")
    tkInvalid(castCommand({ type: "cast_control", device_id: own, action: "seek", position_ms: invalid }, own), "a seek to nowhere")
    tkInvalid(castCommand({ type: "cast_control", device_id: own, action: "rewind" }, own), "an action this version doesn't know")
    tkInvalid(castCommand({ type: "cast_control", device_id: "another-device", action: "pause" }, own))
end sub

sub test_cast_commands_wait_for_a_profile()
    tkTrue(castTakesCommands("home", true))
    tkTrue(castTakesCommands("player", true))
    tkTrue(castTakesCommands("details", true))
    tkFalse(castTakesCommands("profiles", true), "Who's watching? and its PIN")
    tkFalse(castTakesCommands("link", true), "signing in")
    tkFalse(castTakesCommands("servers", true))
    tkFalse(castTakesCommands("home", false), "signed out")
    tkFalse(castTakesCommands("", true))
    tkTrue(castReplaces("player"), "the title playing leaves")
    tkTrue(castReplaces("livePlayer"), "and so does Live TV")
    tkFalse(castReplaces("details"), "Back from the title goes back there")
    tkFalse(castReplaces("nowPlaying"))
end sub

sub test_cast_title_sent_without_a_position_resumes()
    tkEqual(playerResumeOf({ duration_ms: 6000000, user_state: { position_ms: 600000 } }), 600000)
    tkEqual(playerResumeOf({ duration_ms: 6000000, user_state: { position_ms: 5900000 } }), 0, "near the end: from the start")
    tkEqual(playerResumeOf({ duration_ms: 6000000, user_state: invalid }), 0)
    tkEqual(playerResumeOf(invalid), 0)
end sub

sub test_cast_reports_what_plays()
    tkEqual(castStatePath("0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e"), "/api/cast/devices/0b7c4d1e9a2f4e3b8c5d6f7a8b9c0d1e/state")
    tkEqual(ParseJson(castStateBody(412, 61234, 5400000, "playing")), { item_id: 412, position_ms: 61234, duration_ms: 5400000, state: "playing" })
    tkEqual(ParseJson(castStateBody(412, -1, 0, "buffering")), { item_id: 412, position_ms: 0, state: "buffering" }, "no length known yet")
    tkEqual(ParseJson(castStateBody(412, 1500.7, 5400000#, "paused")), { item_id: 412, position_ms: 1500, duration_ms: 5400000, state: "paused" })
    tkEqual(castStateBody(0, 61234, 5400000, ""), "{""item_id"":null}", "nothing plays")
    tkEqual(castStateBody(invalid, 0, 0, "playing"), "{""item_id"":null}")
    tkEqual(castPlayState("playing"), "playing")
    tkEqual(castPlayState("paused"), "paused")
    tkEqual(castPlayState("buffering"), "buffering")
    tkEqual(castPlayState("none"), "playing")
end sub
