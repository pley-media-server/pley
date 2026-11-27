' Pley's themes and accents (docs/THEMES.md): which one a Roku shows, and
' the colors its pages and its players are drawn in (ThemeTable.brs, from
' packages/themes/themes.json). The Roku keeps its pick in the registry
' section appearance; the server keeps none.

' The tables of ThemeTable.brs, made once in a component.
function themeSpec() as object
    if m.themeSpec = invalid then m.themeSpec = { themes: themeTable(), accents: themeAccentTable() }
    return m.themeSpec
end function

' The theme shown for an id kept: Midnight for none, or one this app
' doesn't know.
function themeShown(id as dynamic) as string
    if GetInterface(id, "ifString") <> invalid and themeSpec().themes.DoesExist(id) then return LCase(id)
    return "midnight"
end function

' The accent shown for an id kept: "" (the theme's own) for none, or one
' this app doesn't know.
function themeAccentShown(id as dynamic) as string
    if GetInterface(id, "ifString") <> invalid and id <> "" and themeSpec().accents.DoesExist(id) then return LCase(id)
    return ""
end function

' The colors of a theme under an accent ("" for the theme's own), by token
' (bg, surface2, accentFg, brandTile…), as Roku colors ("0x0B0D10FF"). A
' token takes the accent's color, from its set for the theme's kind, where
' the theme follows it, and the theme's own otherwise (colorOf in
' apps/web/src/theme/spec.ts). key names the pair ("twilight/gold").
function themeColors(theme as string, accent as string) as object
    theme = themeShown(theme)
    accent = themeAccentShown(accent)
    own = themeSpec().themes[theme]
    set = {}
    if accent <> "" then set = themeSpec().accents[accent][own.kind]
    follows = {}
    for each token in own.follows
        follows[token] = true
    end for
    colors = { key: theme + "/" + accent }
    for each token in own.colors
        color = own.colors[token]
        if follows.DoesExist(token) and set.DoesExist(token) then color = set[token]
        colors[token] = "0x" + color
    end for
    return colors
end function

' The colors a theme's players are drawn in: a light theme's twin's, a dark
' one's own, under the same accent (which then takes its color for the
' dark themes).
function themePlayerColors(theme as string, accent as string) as object
    twin = themeSpec().themes[themeShown(theme)].twin
    if twin <> "" then return themeColors(twin, accent)
    return themeColors(theme, accent)
end function

' Whether a theme is light (its players then draw in their twin's colors).
function themeIsLight(theme as string) as boolean
    return themeSpec().themes[themeShown(theme)].kind = "light"
end function

' The picture behind a theme's pages, its light drawn on its bg (Twilight's
' sunset, Moonlight's moon, Nightshade's light, Sunshine's bubbles; the
' web's --glow, drawn by scripts/render-roku-images.mjs at the repository's
' root), or "" for a theme with none.
function themeBackdrop(theme as string) as string
    theme = themeShown(theme)
    if theme = "twilight" or theme = "moonlight" or theme = "nightshade" or theme = "sunshine"
        return "pkg:/images/ui/backdrop-" + theme + ".png"
    end if
    return ""
end function

' The picture over the open rail (the web's --sidebar): Sunshine's soft
' rainbow, or "" for none.
function themeSidebar(theme as string) as string
    if themeShown(theme) = "sunshine" then return "pkg:/images/ui/sidebar-sunshine.png"
    return ""
end function

' The colors the screens' markup is written in, Midnight's own, by their
' red, green and blue ("0B0D10": "bg"), for themeRecolor. The logo's and
' the shade, which the markup doesn't take, are left out, and so is
' danger-fg, Midnight's bg too: a screen sets those itself. Black and
' white aren't Midnight's: they are what sits on pictures and video, and
' stay.
function themeSentinels() as object
    sentinels = {}
    colors = themeColors("midnight", "")
    for each token in colors
        if token <> "key" and token <> "shade" and token <> "dangerFg" and LCase(Left(token, 5)) <> "brand"
            sentinels[Mid(colors[token], 3, 6)] = token
        end if
    end for
    return sentinels
end function

' --- The picks kept on this Roku (registry section appearance) ---

' The theme and accent ({ theme, accent }, the accent "" for the theme's
' own) the screens on top are drawn in. Outside a profile (choosing a
' server, signing in, "Who's watching?", the server gone at a start):
' Midnight in its own accent. Inside one: the profile's own pick on this
' Roku (its id on server), Midnight for one that picked none here; while
' the profile isn't known yet (a start, profileId 0), the pick shown last,
' which is the profile signed in at the last start's, so its pages start
' in it.
function themeFor(section as object, server as string, profileId as integer, outside as boolean) as object
    if outside then return { theme: "midnight", accent: "" }
    return themePick(section, server, profileId)
end function

' A profile's pick on this Roku (profileId 0: the pick shown last), as
' themeFor has it inside a profile.
function themePick(section as object, server as string, profileId as integer) as object
    key = ""
    if profileId > 0 then key = themeProfileKey(server, profileId)
    return { theme: themeShown(section.Read("theme" + key)), accent: themeAccentShown(section.Read("accent" + key)) }
end function

' Keeps a pick: the profile's, when one is known, and as the one shown
' last.
sub themeKeep(section as object, server as string, profileId as integer, theme as string, accent as string)
    if profileId > 0
        key = themeProfileKey(server, profileId)
        section.Write("theme" + key, theme)
        section.Write("accent" + key, accent)
    end if
    themeKeepShown(section, theme, accent)
end sub

' Keeps the pick shown last, for the next start's first screens: the
' profile's inside one, Midnight outside.
sub themeKeepShown(section as object, theme as string, accent as string)
    if section.Read("theme") = theme and section.Read("accent") = accent then return
    section.Write("theme", theme)
    section.Write("accent", accent)
    section.Flush()
end sub

' Whether a screen is outside a profile, drawn in Midnight: choosing a
' server, signing in, "Who's watching?" (its PIN pad is Roku's own, over
' it), and the server gone when nothing else was shown (alone: the only
' screen), as at a start.
function themeOutside(route as string, alone as boolean) as boolean
    if route = "servers" or route = "link" or route = "profiles" then return true
    return route = "unreachable" and alone
end function

' A profile's keys after "theme" and "accent": its server and its id, a
' profile's id being its server's.
function themeProfileKey(server as string, profileId as integer) as string
    return " " + server + " " + profileId.ToStr()
end function
