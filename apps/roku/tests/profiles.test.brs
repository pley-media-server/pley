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

' Who the app is says which profile is current, and that one's PIN, as on
' the other TVs (packages/tv/test/fixtures/profiles.json).
sub test_profiles_with_who_the_app_is()
    household = ParseJson("[{""id"":1,""name"":""Anna"",""current"":true,""needs"":""none""},{""id"":2,""name"":""Sam"",""current"":false,""needs"":""pin""}]")
    me = profMe(ParseJson("{""id"":1,""name"":""Anna"",""has_pin"":true}"))
    tkEqual(me, { id: 1, hasPin: true })
    tiles = profTiles(household, "http://nas:7539", me)
    tkTrue(tiles[0].current)
    tkEqual(tiles[0].needs, "pin", "the server says none: switching to it takes nothing")
    tkEqual(tiles[0].subtitle, "PIN")
    tkEqual(tiles[1].needs, "pin")
    tkEqual(profTiles(household, "", { id: 1, hasPin: false })[0].needs, "none")
    ' Signed in to another profile since.
    tiles = profTiles(household, "", { id: 2, hasPin: false })
    tkFalse(tiles[0].current)
    tkTrue(tiles[1].current)
    tkEqual(tiles[1].needs, "none")
    tkEqual(tiles[1].subtitle, "")
    ' Unknown, or not in the household: as listed.
    tkEqual(profTiles(household, "", invalid)[0].needs, "none")
    tiles = profTiles(household, "", { id: 9, hasPin: true })
    tkTrue(tiles[0].current)
    tkEqual(tiles[0].needs, "none")
    tkEqual(profMe(ParseJson("{""name"":""Anna""}")), invalid)
    tkEqual(profMe(invalid), invalid)
end sub

sub test_profiles_choice()
    household = ParseJson("[{""id"":1,""name"":""Anna"",""current"":true,""needs"":""none""},{""id"":2,""name"":""Sam"",""current"":false,""needs"":""pin""}]")
    me = { id: 1, hasPin: true }
    tiles = profTiles(household, "", me)
    tkEqual(profChoice(tiles[0], me, true), "confirm", "at a start")
    tkEqual(profChoice(tiles[0], me, false), "done", "from Settings")
    tkEqual(profChoice(tiles[1], me, true), "pin")
    noPin = { id: 1, hasPin: false }
    tkEqual(profChoice(profTiles(household, "", noPin)[0], noPin, true), "done")
    tkEqual(profChoice(profTiles(household, "", invalid)[0], invalid, true), "done", "who the app is unknown")
    sam = { id: 2, hasPin: true }
    tiles = profTiles(household, "", sam)
    tkEqual(profChoice(tiles[1], sam, true), "confirm")
    tkEqual(profChoice(tiles[0], sam, true), "switch")
    others = profTiles(ParseJson("[{""id"":3,""name"":""Robin"",""needs"":""password""},{""id"":4,""name"":""Kids"",""needs"":""locked""},{""id"":5,""name"":""Alex"",""needs"":""passkey""}]"), "")
    tkEqual(profChoice(others[0], invalid, false), "approve")
    tkEqual(profChoice(others[1], invalid, false), "locked")
    tkEqual(profChoice(others[2], invalid, false), "locked", "a way in this app doesn't know")
end sub

sub test_profiles_asked_at_a_start()
    scope = "library play playlists profiles"
    ' The profile signed in has no PIN: straight in.
    household = { count: 2, ids: [7, 9], pin: 0 }
    tkEqual(profAtStart(scope, household, 0), "", "no PIN")
    tkEqual(profAtStart(scope, household, 9), "", "Always start as names a profile")
    tkEqual(profAtStart(scope, household, 12), "", "a profile gone from the household: as without one")
    ' It has one, or the Roku doesn't know: the profiles.
    tkEqual(profAtStart(scope, { count: 2, ids: [7, 9], pin: 1 }, 0), "ask", "a PIN")
    tkEqual(profAtStart(scope, { count: 2, ids: [7, 9], pin: -1 }, 0), "ask", "not known whether it has one")
    tkEqual(profAtStart(scope, { count: 2, ids: [7, 9], pin: 1 }, 12), "ask", "a PIN, and a profile gone from the household")
    tkEqual(profAtStart(scope, { count: 2, ids: [], pin: 1 }, 9), "ask", "the household's profiles not known yet: asked")
    tkEqual(profAtStart(scope, { count: 1, ids: [7], pin: 1 }, 0), "", "a household of one")
    tkEqual(profAtStart("library play", { count: 3, ids: [7, 9, 11], pin: 1 }, 0), "", "a sign-in that can't switch")
    tkTrue(authAllows("library play", "play"))
    tkFalse(authAllows("library playlists", "play"))
end sub

' Always start as goes on as the profile signed in, but never past its PIN.
sub test_profiles_always_start_as_never_past_a_pin()
    scope = "library play playlists profiles"
    tkEqual(profAtStart(scope, { count: 2, ids: [7, 9], pin: 1 }, 9), "pin", "its pad first")
    tkEqual(profAtStart(scope, { count: 2, ids: [7, 9], pin: -1 }, 9), "ask", "not known whether it has one")
    tkEqual(profAtStart(scope, { count: 2, ids: [7, 9], pin: 1 }, 0), "ask", "without Always start as, the profiles as before")

    kept = CreateObject("roRegistrySection", "householdPinTest")
    profKeepHousehold(kept, "http://nas:7539", ParseJson("[{""id"":7},{""id"":9}]"))
    tkEqual(profKeptHousehold(kept, "http://nas:7539").pin, -1, "never heard")
    profKeepPin(kept, "http://nas:7539", ParseJson("{""id"":9,""has_pin"":true}"))
    tkEqual(profKeptHousehold(kept, "http://nas:7539").pin, 1)
    tkEqual(profKeptHousehold(kept, "http://other:7539").pin, -1, "for each server")
    profKeepPin(kept, "http://nas:7539", ParseJson("{""id"":9}"))
    tkEqual(profKeptHousehold(kept, "http://nas:7539").pin, 0)
    profKeepPin(kept, "http://nas:7539", invalid)
    tkEqual(profKeptHousehold(kept, "http://nas:7539").pin, -1, "another profile signed in")
    tkEqual(profKeptHousehold(kept, "http://nas:7539").count, 2, "the household stays")

    tiles = profTiles(ParseJson("[{""id"":7,""name"":""Anna"",""current"":true,""needs"":""none""},{""id"":9,""name"":""Sam"",""needs"":""pin""}]"), "", { id: 7, hasPin: true })
    tkEqual(profFirstPad(tiles, true).itemId, 7, "the profile signed in")
    tkInvalid(profFirstPad(tiles, false), "not a start as it")
    tiles = profTiles(ParseJson("[{""id"":7,""name"":""Anna"",""current"":true,""needs"":""none""},{""id"":9,""name"":""Sam"",""needs"":""pin""}]"), "", { id: 7, hasPin: false })
    tkInvalid(profFirstPad(tiles, true), "no PIN since")
end sub

sub test_profiles_always_start_as()
    section = CreateObject("roRegistrySection", "startAsTest")
    tkEqual(profStartAs(section, "http://nas:7539"), 0, "none kept")
    profSetStartAs(section, "http://nas:7539", 7)
    tkEqual(profStartAs(section, "http://nas:7539"), 7)
    tkEqual(profStartAs(section, "http://other:7539"), 0, "for each server")
    profSetStartAs(section, "http://nas:7539", 0)
    tkEqual(profStartAs(section, "http://nas:7539"), 0, "Profile used last forgets it")

    kept = CreateObject("roRegistrySection", "householdTest")
    profKeepHousehold(kept, "http://nas:7539", ParseJson("[{""id"":7,""current"":false},{""id"":9,""current"":true}]"))
    tkEqual(profKeptHousehold(kept, "http://nas:7539"), { count: 2, ids: [7, 9], pin: -1 }, "how many, and which")
    tkEqual(profKeptHousehold(kept, "http://other:7539"), { count: 0, ids: [], pin: -1 }, "for each server")
    profKeepHousehold(kept, "http://nas:7539", [])
    tkEqual(profKeptHousehold(kept, "http://nas:7539"), { count: 0, ids: [], pin: -1 })

    household = profHousehold(ParseJson("[{""id"":7,""name"":""Anna""},{""id"":9,""name"":""Kids""},{""id"":0,""name"":""?""}]"))
    tkEqual(household, [{ id: 7, name: "Anna" }, { id: 9, name: "Kids" }])
    choice = profStartAsChoices(household, 9)
    tkEqual(choice.labels, ["Profile used last", "Anna", "Kids"])
    tkEqual(choice.values, [0, 7, 9])
    tkEqual(choice.current, 2)
    tkEqual(profStartAsChoices(household, 0).current, 0)
    tkEqual(profStartAsLabel(household, 7), "Anna")
    tkEqual(profStartAsLabel(household, 3), "Profile used last", "a profile gone")
end sub

sub test_profiles_switch_answers()
    tkEqual(authSwitchOutcome(200, "{}").kind, "tokens")
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_request""}").kind, "through", "no PIN to check")
    wrong = authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""wrong_pin""}")
    tkEqual(wrong, { kind: "refused", reason: "wrong_pin", message: "Wrong PIN." })
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""throttled"",""error_description"":""Too many wrong PINs: try again in 12 minutes.""}").message, "Too many wrong PINs: try again in 12 minutes.")
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""browser""}").reason, "browser")
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_grant"",""reason"":""ticket_spent""}").message, "This cast's switch expired: cast again.", "a cast's ticket")
    tkEqual(authSwitchOutcome(400, "{""error"":""invalid_grant""}").kind, "ended", "the refresh token doesn't work")
    tkEqual(authSwitchOutcome(400, "{""error"":""unauthorized_client""}").message, "This sign-in can't switch profiles.")
    tkEqual(authSwitchOutcome(-28, "").kind, "failed")
    tkEqual(authSwitchOutcome(503, "").message, "The server refused: try again.")
end sub
