' The themes and accents (lib/Themes.brs, ThemeTable.brs) held to the one
' definition every app draws them from, packages/themes/themes.json, which
' the runner gives as pkg:/themes/themes.json: every token of every theme
' under every accent, and its players', as the web's colorOf has them.

function themesTestSpec() as object
    spec = ParseJson(ReadAsciiFile("pkg:/themes/themes.json"))
    if spec = invalid or GetInterface(spec.themes, "ifArray") = invalid or GetInterface(spec.accents, "ifArray") = invalid
        tkFail("packages/themes/themes.json didn't load")
        return invalid
    end if
    return spec
end function

' "surface-2" as the app names it: surface2.
function themesTestCamel(token as string) as string
    out = ""
    upper = false
    for i = 1 to Len(token)
        c = Mid(token, i, 1)
        if c = "-"
            upper = true
        else if upper
            out = out + UCase(c)
            upper = false
        else
            out = out + c
        end if
    end for
    return out
end function

' A CSS color of the JSON's ("#0b0d10", "rgb(16 26 44 / 0.42)") as Roku's.
function themesTestRoku(css as string) as string
    if Left(css, 1) = "#" and Len(css) = 7 then return "0x" + UCase(Mid(css, 2)) + "FF"
    inner = Mid(css, 5, Len(css) - 5)
    parts = inner.Replace("/", " ").Tokenize(" ")
    hex = "0x"
    for i = 0 to 2
        hex = hex + themesTestByte(parts[i].ToInt())
    end for
    return hex + themesTestByte(Int(parts[3].ToFloat() * 255 + 0.5))
end function

function themesTestByte(n as integer) as string
    digits = "0123456789ABCDEF"
    return Mid(digits, Int(n / 16) + 1, 1) + Mid(digits, (n mod 16) + 1, 1)
end function

' colorOf (apps/web/src/theme/spec.ts): the accent's color where the theme
' follows the token and the accent has one, the theme's own otherwise.
function themesTestColorOf(theme as object, accent as dynamic, token as string) as string
    if accent <> invalid
        set = accent[theme.kind]
        for each followed in theme.follows
            if followed = token and set.DoesExist(token) then return set[token]
        end for
    end if
    return theme.colors[token]
end function

' By index: a for each here would share the list's place with one going
' through it already.
function themesTestFind(list as object, id as string) as dynamic
    for i = 0 to list.Count() - 1
        if list[i].id = id then return list[i]
    end for
    return invalid
end function

sub test_themes_ids_are_the_files_in_its_order()
    spec = themesTestSpec()
    if spec = invalid then return
    ids = []
    for each theme in spec.themes
        ids.Push(theme.id)
    end for
    tkEqual(themeIds(), ids)
    accents = []
    for each accent in spec.accents
        accents.Push(accent.id)
    end for
    tkEqual(themeAccentIds(), accents)
end sub

' The accents to try: the theme's own ("") and each of the file's.
function themesTestAccents(spec as object) as object
    ids = [""]
    for each accent in spec.accents
        ids.Push(accent.id)
    end for
    return ids
end function

' Every token of colors is the file's for theme under accentId, and
' nothing else is there.
sub themesTestHold(spec as object, theme as object, accentId as string, colors as object, what as string)
    accent = invalid
    if accentId <> "" then accent = themesTestFind(spec.accents, accentId)
    count = 0
    for each token in theme.colors
        count = count + 1
        expected = themesTestRoku(themesTestColorOf(theme, accent, token))
        tkEqual(colors[themesTestCamel(token)], expected, what + " " + token)
    end for
    ' Its tokens and key.
    have = 0
    for each token in colors
        have = have + 1
    end for
    tkEqual(have, count + 1, what + ": tokens")
end sub

sub test_themes_every_color_of_every_theme_and_accent_is_the_files()
    spec = themesTestSpec()
    if spec = invalid then return
    for each theme in spec.themes
        for each accentId in themesTestAccents(spec)
            what = theme.id + "/" + accentId
            colors = themeColors(theme.id, accentId)
            tkEqual(colors.key, what, what + ": key")
            themesTestHold(spec, theme, accentId, colors, what)
        end for
    end for
end sub

' A light theme's players draw in its twin's colors, a dark theme's in its
' own, under the same accent.
sub test_themes_players_draw_in_the_twins_colors()
    spec = themesTestSpec()
    if spec = invalid then return
    for each theme in spec.themes
        drawn = theme
        if theme.kind = "light" then drawn = themesTestFind(spec.themes, theme.twin)
        tkTrue(drawn <> invalid and drawn.kind = "dark", theme.id + "'s twin")
        if drawn = invalid then return
        tkEqual(themeIsLight(theme.id), theme.kind = "light", theme.id)
        for each accentId in themesTestAccents(spec)
            what = theme.id + "/" + accentId + " playing"
            themesTestHold(spec, drawn, accentId, themePlayerColors(theme.id, accentId), what)
        end for
    end for
end sub

sub test_themes_an_unknown_id_shows_midnight_and_the_themes_own_accent()
    tkEqual(themeShown(invalid), "midnight")
    tkEqual(themeShown(""), "midnight")
    tkEqual(themeShown("dusk"), "midnight")
    tkEqual(themeShown("sunshine"), "sunshine")
    tkEqual(themeAccentShown(invalid), "")
    tkEqual(themeAccentShown("teal"), "")
    tkEqual(themeAccentShown("gold"), "gold")
    tkEqual(themeColors("dusk", "teal"), themeColors("midnight", ""))
end sub

' The screens' markup is in Midnight's own colors: each of them stands for
' one token, which takes the theme's color with the markup's alpha; other
' colors stay as they are.
sub test_themes_markup_colors_take_the_themes()
    sentinels = themeSentinels()
    twilight = themeColors("twilight", "gold")
    tkEqual(sentinels["0B0D10"], "bg")
    tkEqual(sentinels["E9EBEF"], "fg")
    tkEqual(sentinels["38BDF8"], "accent")
    tkEqual(themeRecolor("0x0B0D10CC", sentinels, twilight), Left(twilight.bg, 8) + "CC")
    tkEqual(themeRecolor("0x38BDF8FF", sentinels, twilight), twilight.accent)
    ' Roku gives a color field back as an integer.
    tkEqual(themeRecolor(&hE9EBEFFF, sentinels, twilight), twilight.fg)
    tkEqual(themeRecolor(&h0B0D1080, sentinels, twilight), Left(twilight.bg, 8) + "80")
    ' Black and white are pictures' and video's.
    tkInvalid(themeRecolor("0x000000CC", sentinels, twilight))
    tkInvalid(themeRecolor("0xFFFFFFFF", sentinels, twilight))
    tkInvalid(themeRecolor(&hFFFFFFFF, sentinels, twilight))
    ' Midnight's own needs no change.
    tkInvalid(themeRecolor("0xE9EBEFFF", sentinels, themeColors("midnight", "")))
    tkEqual(themeHex(&h12345678), "12345678")
    tkEqual(themeHex("0x0b0d10ff"), "0B0D10FF")
    tkEqual(themeHex("blue"), "")
end sub

' The dim behind a panel is the theme's shade at a share of its own alpha;
' the words on a fill are that fill's words color.
sub test_themes_shade_and_words_on_a_fill()
    midnight = themeColors("midnight", "")
    tkEqual(themeShade(midnight, 0.5), "0x00000080")
    ' Midday's shade is rgb(16 26 44 / 0.42): 0x6B of alpha, half of it 0x36.
    tkEqual(themeShade(themeColors("midday", ""), 0.5), "0x101A2C36")
    tkEqual(themeAlpha(midnight.fg, "1F"), "0xE9EBEF1F")
    starlight = themeColors("starlight", "mint")
    tkEqual(themeWordsOn(starlight, starlight.accent), starlight.accentFg)
    tkEqual(themeWordsOn(starlight, starlight.danger), starlight.dangerFg)
    tkEqual(themeWordsOn(starlight, starlight.success), starlight.bg)
    tkEqual(themeWordsOn(starlight, starlight.warning), starlight.bg)
end sub

' Colors a screen sets itself before the scene recolors it, or that a
' screen recolors twice, stay right: no theme's color is Midnight's for
' another token unless that token has the same color in the theme too.
' The logo's are left out: the logo colors itself, and the recoloring
' passes it by (a component with a colors field).
sub test_themes_recoloring_twice_changes_nothing()
    sentinels = themeSentinels()
    accents = [""]
    accents.Append(themeAccentIds())
    for each theme in themeIds()
        for each accent in accents
            for each colors in [themeColors(theme, accent), themePlayerColors(theme, accent)]
                for each token in colors
                    if token <> "key" and LCase(Left(token, 5)) <> "brand"
                        again = themeRecolor(colors[token], sentinels, colors)
                        if again <> invalid then tkFail(colors.key + "'s " + token + " " + colors[token] + " would turn " + again)
                    end if
                end for
            end for
        end for
    end for
end sub

' The Live TV guide has its pictures in every theme's colors
' (scripts/make_images.py draws them).
sub test_themes_the_guide_has_every_themes_pictures()
    files = CreateObject("roFileSystem")
    accents = [""]
    accents.Append(themeAccentIds())
    for each theme in themeIds()
        for each accent in accents
            colors = themeColors(theme, accent)
            for each picture in [themeGuidePicture("cell", colors.surface2), themeGuidePicture("focus", colors.accent), themeGuidePicture("past", colors.surface)]
                tkTrue(files.Exists(picture), colors.key + " " + picture)
            end for
        end for
    end for
    tkEqual(themeGuidePicture("cell", "0x191D24FF"), "pkg:/images/guide/cell-191D24.9.png")
end sub

' Each profile keeps its own pick on this Roku; one new to it shows
' Midnight in its own accent; switching profiles switches the pick; and
' before a profile is known, the pick shown last shows.
sub test_themes_each_profile_keeps_its_pick()
    section = tkRegistrySection()
    nas = "http://nas:7539"
    tkEqual(themePick(section, nas, 0), { theme: "midnight", accent: "" }, "a first start")
    themeKeep(section, nas, 1, "twilight", "gold")
    themeKeep(section, nas, 2, "midday", "")
    tkEqual(themePick(section, nas, 1), { theme: "twilight", accent: "gold" }, "profile 1")
    tkEqual(themePick(section, nas, 2), { theme: "midday", accent: "" }, "profile 2")
    tkEqual(themePick(section, nas, 1), { theme: "twilight", accent: "gold" }, "back to profile 1")
    tkEqual(themePick(section, nas, 3), { theme: "midnight", accent: "" }, "a profile new here")
    tkEqual(themePick(section, "http://other:7539", 1), { theme: "midnight", accent: "" }, "another server's profile 1")
    ' The same id on two servers is two profiles, with a pick each.
    themeKeep(section, "http://other:7539", 1, "sunshine", "mint")
    tkEqual(themePick(section, "http://other:7539", 1), { theme: "sunshine", accent: "mint" }, "the other server's profile 1")
    tkEqual(themePick(section, nas, 1), { theme: "twilight", accent: "gold" }, "this server's profile 1")
    themeKeep(section, nas, 2, "midday", "")
    ' Before a profile is known: the one picked last.
    tkEqual(themePick(section, nas, 0), { theme: "midday", accent: "" }, "unknown profile")
    ' The scene keeps the one it shows for a profile as the one shown last.
    themeKeepShown(section, "twilight", "gold")
    tkEqual(themePick(section, "", 0), { theme: "twilight", accent: "gold" }, "after a switch")
    tkTrue(section.flushes > 0, "flushed")
    section.Write("theme", "dusk")
    section.Write("accent", "teal")
    tkEqual(themePick(section, "", 0), { theme: "midnight", accent: "" }, "ids this app doesn't know")
end sub

' Outside a profile (choosing a server, signing in, "Who's watching?", the
' server gone at a start) the screens are Midnight in its own accent, a
' profile with another pick shown before or not; the profile's pick comes
' back inside it; a start inside a profile not yet heard is in the pick
' shown last, the profile's, and outside one in Midnight.
sub test_themes_outside_a_profile_is_midnight()
    section = tkRegistrySection()
    nas = "http://nas:7539"
    midnight = { theme: "midnight", accent: "" }
    themeKeep(section, nas, 1, "twilight", "gold")
    tkEqual(themeFor(section, nas, 1, false), { theme: "twilight", accent: "gold" }, "inside")
    tkEqual(themeFor(section, nas, 1, true), midnight, "outside, after the profile's")
    themeKeepShown(section, "midnight", "")
    tkEqual(themeFor(section, nas, 1, false), { theme: "twilight", accent: "gold" }, "inside again")
    ' A start straight into the profile, before it is heard: the pick shown
    ' last, its own, as the scene keeps what it shows.
    themeKeepShown(section, "twilight", "gold")
    tkEqual(themeFor(section, "", 0, false), { theme: "twilight", accent: "gold" }, "a start inside")
    tkEqual(themeFor(section, "", 0, true), midnight, "a start outside")
    ' Left outside: the next start is in Midnight until the profile is heard.
    themeKeepShown(section, "midnight", "")
    tkEqual(themeFor(section, "", 0, false), midnight, "a start after leaving outside")
    for each route in ["servers", "link", "profiles"]
        tkTrue(themeOutside(route, false), route)
    end for
    tkTrue(themeOutside("unreachable", true), "the server gone at a start")
    tkFalse(themeOutside("unreachable", false), "the server gone over pages")
    for each route in ["home", "settings", "appearance", "player", "details"]
        tkFalse(themeOutside(route, false), route)
    end for
end sub
