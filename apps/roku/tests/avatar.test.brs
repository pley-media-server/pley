' A profile's avatar: every case of the rule every app shares
' (packages/tv/test/fixtures/avatar.json, which the runner gives as
' pkg:/fixtures/avatar.json).

function avatarTestFixture() as object
    data = ParseJson(ReadAsciiFile("pkg:/fixtures/avatar.json"))
    if data = invalid or httpArray(data.cases).Count() = 0
        tkFail("the fixture packages/tv/test/fixtures/avatar.json didn't load")
        return invalid
    end if
    return data
end function

' A color as the fixture writes it ("#38bdf8") as Roku writes it.
function avatarTestRoku(hex as string) as string
    return "0x" + UCase(Mid(hex, 2)) + "FF"
end function

sub test_avatar_colors_and_inks_are_the_fixtures()
    fixture = avatarTestFixture()
    if fixture = invalid then return
    colors = []
    for each hex in fixture.colors
        colors.Push(avatarTestRoku(hex))
    end for
    tkEqual(avatarColors(), colors)
    tkEqual(avatarInks(), { dark: avatarTestRoku(fixture.ink.dark), light: avatarTestRoku(fixture.ink.light) })
end sub

sub test_avatar_as_the_fixture_has_it()
    fixture = avatarTestFixture()
    if fixture = invalid then return
    for each c in fixture.cases
        ' Each case is a profile as the server sends one: its name and
        ' avatar_color.
        avatar = avatarOf(c)
        what = "[" + c.name + "] " + httpString(c.avatar_color)
        tkEqual(avatar.color, avatarTestRoku(c.color), what + ": color")
        tkEqual(avatar.ink, avatarTestRoku(c.ink), what + ": ink")
        tkEqual(avatar.initial, c.initial, what + ": initial")
    end for
end sub

sub test_avatar_without_a_profile()
    tkEqual(avatarOf(invalid), { color: "0x38BDF8FF", ink: "0x0B0D10FF", initial: "?" })
    tkEqual(avatarOf({ name: 7, avatar_color: 7 }).initial, "?", "a name that isn't one")
    tkEqual(avatarOf({ name: "Ben", avatar_color: "#ABCDEF " }).color, "0xFB923CFF", "a color that isn't one: the name's")
end sub
