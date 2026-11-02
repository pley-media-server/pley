' "Who's watching?": the household's profiles (GET /api/oauth/profiles)
' as the screen shows them, and when it asks at a start, as the other TVs
' do: on a server whose household has more than one profile, for a
' sign-in allowed to switch them.

' The profiles as tiles: [{ title, subtitle, image, color, initial, itemId,
' needs, current }]. needs: what switching to it takes ("none", "pin",
' "password", "locked").
function profTiles(profiles as dynamic, server as string) as object
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
    return tiles
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
' asked (household, profKeptHousehold's), unless Settings' "Always start
' as" names one of them (startAs, its id; 0 for none). As on the other TVs,
' that only skips the question: the app starts as the profile it was left
' on. A setting naming a profile gone from the household (deleted, or
' another account's, signed in since) asks again, as Settings then says.
function profAskWho(scope as string, household as object, startAs as integer) as boolean
    if not authAllows(scope, "profiles") or household.count < 2 then return false
    if startAs = 0 then return true
    for each id in household.ids
        if id = startAs then return false
    end for
    return true
end function

' The household as last asked for on server, kept on this Roku (registry
' section household): { count, ids (its profiles') }.
function profKeptHousehold(section as object, server as string) as object
    ids = []
    for each id in section.Read(server + "#ids").Tokenize(",")
        if Val(id) > 0 then ids.Push(Int(Val(id)))
    end for
    return { count: Int(Val(section.Read(server))), ids: ids }
end function

' Keeps the household's profiles (GET /api/oauth/profiles) for the next
' start: how many, and their ids.
sub profKeepHousehold(section as object, server as string, profiles as dynamic)
    list = httpArray(profiles)
    ids = []
    for each profile in list
        id = 0
        if GetInterface(profile, "ifAssociativeArray") <> invalid then id = Int(httpNumber(profile.id, 0))
        if id > 0 then ids.Push(id.ToStr())
    end for
    section.Write(server, list.Count().ToStr())
    section.Write(server + "#ids", ids.Join(","))
    section.Flush()
end sub

' "Always start as" for server, kept on this Roku (registry section
' startAs): the profile's id, 0 for none.
function profStartAs(section as object, server as string) as integer
    return Int(Val(section.Read(server)))
end function

sub profSetStartAs(section as object, server as string, id as integer)
    if id = 0
        section.Delete(server)
    else
        section.Write(server, id.ToStr())
    end if
    section.Flush()
end sub

' Settings' choices of "Always start as": Ask who's watching, then the
' household's profiles ([{ id, name }]); the one kept checked.
function profStartAsChoices(profiles as object, startAs as integer) as object
    choice = { title: "Always start as", labels: ["Ask who's watching"], values: [0], hints: [], current: 0 }
    for each profile in profiles
        if profile.id = startAs then choice.current = choice.values.Count()
        choice.labels.Push(profile.name)
        choice.values.Push(profile.id)
    end for
    return choice
end function

' What the row says: the profile kept, or Ask who's watching (also for one
' gone from the household).
function profStartAsLabel(profiles as object, startAs as integer) as string
    for each profile in profiles
        if profile.id = startAs then return profile.name
    end for
    return "Ask who's watching"
end function

' The household's profiles as Settings lists them: [{ id, name }].
function profHousehold(profiles as dynamic) as object
    out = []
    for each profile in httpArray(profiles)
        if GetInterface(profile, "ifAssociativeArray") <> invalid and httpNumber(profile.id, 0) > 0
            out.Push({ id: Int(httpNumber(profile.id, 0)), name: httpSentence(profile.name) })
        end if
    end for
    return out
end function
