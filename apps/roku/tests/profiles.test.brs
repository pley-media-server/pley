' "Who's watching?": the household's profiles, when it's asked, and what
' the server's answer to a switch means.

sub test_profiles_as_tiles()
    tiles = profTiles(ParseJson("[{""id"":1,""name"":""anna"",""avatar_url"":""/api/users/1/avatar?v=3"",""avatar_color"":""#38bdf8"",""current"":true,""needs"":""none""},{""id"":2,""name"":""Ben"",""avatar_url"":null,""avatar_color"":null,""current"":false,""needs"":""pin""},{""id"":0,""name"":""broken""}]"), "http://nas:7539")
    tkEqual(tiles.Count(), 2)
    tkEqual(tiles[0].initial, "A")
    tkEqual(tiles[0].image, "http://nas:7539/api/users/1/avatar?v=3&w=220")
    tkEqual(tiles[0].color, "0x38BDF8FF")
    tkTrue(tiles[0].current)
    tkEqual(tiles[0].subtitle, "")
    tkEqual(tiles[1].subtitle, "PIN")
    tkEqual(tiles[1].color, "0x2E3440FF", "no colour: the app's surface")
    tkEqual(tiles[1].image, "")
    tkEqual(profNeedsLabel("password"), "Password")
    tkEqual(profColor("#12345"), "0x2E3440FF")
end sub

sub test_profiles_asked_at_a_start()
    scope = "library play playlists profiles"
    household = { count: 2, ids: [7, 9] }
    tkTrue(profAskWho(scope, household, 0))
    tkFalse(profAskWho(scope, { count: 1, ids: [7] }, 0), "a household of one")
    tkFalse(profAskWho("library play", { count: 3, ids: [7, 9, 11] }, 0), "a sign-in that can't switch")
    tkFalse(profAskWho(scope, household, 9), "Always start as names a profile")
    tkTrue(profAskWho(scope, household, 12), "a profile gone from the household: asked again")
    tkTrue(profAskWho(scope, { count: 2, ids: [] }, 9), "the household's profiles not known yet: asked")
    tkTrue(authAllows("library play", "play"))
    tkFalse(authAllows("library playlists", "play"))
end sub

sub test_profiles_always_start_as()
    section = CreateObject("roRegistrySection", "startAsTest")
    tkEqual(profStartAs(section, "http://nas:7539"), 0, "none kept")
    profSetStartAs(section, "http://nas:7539", 7)
    tkEqual(profStartAs(section, "http://nas:7539"), 7)
    tkEqual(profStartAs(section, "http://other:7539"), 0, "for each server")
    profSetStartAs(section, "http://nas:7539", 0)
    tkEqual(profStartAs(section, "http://nas:7539"), 0, "Ask who's watching forgets it")

    kept = CreateObject("roRegistrySection", "householdTest")
    profKeepHousehold(kept, "http://nas:7539", ParseJson("[{""id"":7,""current"":false},{""id"":9,""current"":true}]"))
    tkEqual(profKeptHousehold(kept, "http://nas:7539"), { count: 2, ids: [7, 9] }, "how many, and which")
    tkEqual(profKeptHousehold(kept, "http://other:7539"), { count: 0, ids: [] }, "for each server")
    profKeepHousehold(kept, "http://nas:7539", [])
    tkEqual(profKeptHousehold(kept, "http://nas:7539"), { count: 0, ids: [] })

    household = profHousehold(ParseJson("[{""id"":7,""name"":""Anna""},{""id"":9,""name"":""Kids""},{""id"":0,""name"":""?""}]"))
    tkEqual(household, [{ id: 7, name: "Anna" }, { id: 9, name: "Kids" }])
    choice = profStartAsChoices(household, 9)
    tkEqual(choice.labels, ["Ask who's watching", "Anna", "Kids"])
    tkEqual(choice.values, [0, 7, 9])
    tkEqual(choice.current, 2)
    tkEqual(profStartAsChoices(household, 0).current, 0)
    tkEqual(profStartAsLabel(household, 7), "Anna")
    tkEqual(profStartAsLabel(household, 3), "Ask who's watching", "a profile gone")
end sub

sub test_profiles_switch_answers()
    tkEqual(authSwitchOutcome(200, "{}").kind, "tokens")
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_request""}").kind, "through", "no PIN to check")
    wrong = authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""wrong_pin""}")
    tkEqual(wrong, { kind: "refused", reason: "wrong_pin", message: "Wrong PIN." })
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""throttled"",""error_description"":""Too many wrong PINs: try again in 12 minutes.""}").message, "Too many wrong PINs: try again in 12 minutes.")
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""browser""}").reason, "browser")
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_grant""}").kind, "ended", "the refresh token doesn't work")
    tkEqual(authSwitchOutcome(400, "{""error"":""unauthorized_client""}").message, "This sign-in can't switch profiles.")
    tkEqual(authSwitchOutcome(-28, "").kind, "failed")
    tkEqual(authSwitchOutcome(503, "").message, "The server refused: try again.")
end sub
