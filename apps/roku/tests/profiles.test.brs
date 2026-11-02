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
    tkTrue(profAskWho("library play playlists profiles", 2))
    tkFalse(profAskWho("library play playlists profiles", 1), "a household of one")
    tkFalse(profAskWho("library play", 3), "a sign-in that can't switch")
    tkTrue(authAllows("library play", "play"))
    tkFalse(authAllows("library playlists", "play"))
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
