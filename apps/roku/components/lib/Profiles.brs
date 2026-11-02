' "Who's watching?": the household's profiles (GET /api/oauth/profiles)
' as the screen shows them, and when it asks at a start, as the other TVs
' do: on a server whose household has more than one profile, for a
' sign-in allowed to switch them.

' The profiles as tiles: [{ title, subtitle, image, color, initial, itemId,
' needs, current }]. needs: what switching to it takes ("none", "pin",
' "password", "locked"). me (profMe), when known, says which is current:
' the server says switching to that one needs nothing, and it shows "PIN"
' when it has one, which a start asks.
function profTiles(profiles as dynamic, server as string, me = invalid as dynamic) as object
    tiles = []
    for each profile in httpArray(profiles)
        if GetInterface(profile, "ifAssociativeArray") <> invalid and httpNumber(profile.id, 0) > 0
            name = httpSentence(profile.name)
            needs = httpString(profile.needs)
            if needs = "" then needs = "none"
            tiles.Push({
                title: name,
                subtitle: profNeedsLabel(needs),
                image: urlImage(server, profile.avatar_url, 220),
                color: profColor(profile.avatar_color),
                initial: UCase(Left(name, 1)),
                itemId: httpNumber(profile.id, 0),
                needs: needs,
                current: httpTrue(profile.current)
            })
        end if
    end for
    if me = invalid then return tiles
    listed = false
    for each tile in tiles
        if tile.itemId = me.id then listed = true
    end for
    if not listed then return tiles
    for each tile in tiles
        tile.current = tile.itemId = me.id
        if tile.current
            tile.needs = "none"
            if me.hasPin then tile.needs = "pin"
            tile.subtitle = profNeedsLabel(tile.needs)
        end if
    end for
    return tiles
end function

' Who the app is (GET /api/auth/me), as "Who's watching?" needs it:
' { id, hasPin }, or invalid without an id.
function profMe(data as dynamic) as dynamic
    me = httpObject(data)
    if me = invalid or httpNumber(me.id, 0) <= 0 then return invalid
    return { id: httpNumber(me.id, 0), hasPin: httpTrue(me.has_pin) }
end function

' What choosing a tile leads to, as on the other TVs: "confirm" (the PIN of
' the profile signed in, checked with the switch-profile grant), "done",
' "switch", "pin", "approve" (signing in to it with a code) or "locked".
' atStart: going on as a profile with a PIN asks it then.
function profChoice(tile as object, me as dynamic, atStart as boolean) as string
    if tile.current
        if atStart and me <> invalid and me.id = tile.itemId and me.hasPin then return "confirm"
        return "done"
    end if
    if tile.needs = "none" then return "switch"
    if tile.needs = "pin" then return "pin"
    if tile.needs = "password" then return "approve"
    return "locked"
end function

' What a profile asks for under its name.
function profNeedsLabel(needs as string) as string
    if needs = "pin" then return "PIN"
    if needs = "password" then return "Password"
    if needs = "locked" then return "Not from the app"
    return ""
end function

' A profile's colour ("#38bdf8") as Roku writes colours; the app's own
' surface without one.
function profColor(hex as dynamic) as string
    text = httpString(hex)
    re = CreateObject("roRegex", "^#[0-9a-fA-F]{6}$", "")
    if not re.IsMatch(text) then return "0x2E3440FF"
    return "0x" + UCase(Mid(text, 2)) + "FF"
end function

' Whether the app asks "Who's watching?" as it starts: a sign-in allowed to
' switch profiles, on a server whose household had more than one when last
' asked.
function profAskWho(scope as string, household as integer) as boolean
    return authAllows(scope, "profiles") and household > 1
end function
