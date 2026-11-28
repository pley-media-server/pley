' A profile's avatar, as every app draws it (docs/THEMES.md, "Profile
' colors"; packages/tv/test/fixtures/avatar.json pins the rule): its
' picture, and under it, for a profile without one or while it loads or
' fails, a circle in the profile's color with its initial in the ink that
' reads better on it. The colors are the profile's, not the theme's: they
' stay under every theme and accent. Colors as Roku writes them
' ("0x38BDF8FF").

' The eight colors the web offers profiles, one of them picked from the
' name for a profile without its own.
function avatarColors() as object
    return ["0x38BDF8FF", "0xA78BFAFF", "0xF472B6FF", "0xFB923CFF", "0xFACC15FF", "0x4ADE80FF", "0x2DD4BFFF", "0xF87171FF"]
end function

' The initial's two inks: Midnight's background on light circles, white
' on dark ones.
function avatarInks() as object
    return { dark: "0x0B0D10FF", light: "0xFFFFFFFF" }
end function

' A profile's circle, from the profile as the server sends it (its name
' and avatar_color): { color, ink, initial }.
function avatarOf(profile as dynamic) as object
    name = ""
    own = invalid
    if GetInterface(profile, "ifAssociativeArray") <> invalid
        if GetInterface(profile.name, "ifString") <> invalid then name = profile.name
        own = profile.avatar_color
    end if
    color = avatarColor(name, own)
    return { color: color, ink: avatarInk(color), initial: avatarInitial(name) }
end function

' The circle's color: the profile's own when it is #rgb or #rrggbb, in
' any case; otherwise one of avatarColors, picked from the name as the
' server has it (untrimmed, as on the web): its code points folded as
' h = (h * 31 + code point) mod 2^32, from 0.
function avatarColor(name as string, own as dynamic) as string
    if GetInterface(own, "ifString") <> invalid
        found = CreateObject("roRegex", "^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$", "").Match(own)
        if found.Count() > 1
            digits = UCase(found[1])
            if Len(digits) = 3
                digits = String(2, Mid(digits, 1, 1)) + String(2, Mid(digits, 2, 1)) + String(2, Mid(digits, 3, 1))
            end if
            return "0x" + digits + "FF"
        end if
    end if
    ' In a LongInteger: the Roku's Integer is 32 bits and signed.
    h = 0&
    count = Len(name)
    at = 1
    while at <= count
        point = Asc(Mid(name, at, 1))
        ' Two UTF-16 surrogates are one code point (brs-node's strings
        ' count them apart; the Roku's count code points).
        if point >= &hD800 and point < &hDC00 and at < count
            low = Asc(Mid(name, at + 1, 1))
            if low >= &hDC00 and low < &hE000
                point = &h10000 + (point - &hD800) * &h400 + low - &hDC00
                at = at + 1
            end if
        end if
        h = (h * 31 + point) mod 4294967296&
        at = at + 1
    end while
    colors = avatarColors()
    return colors[Int(h mod colors.Count())]
end function

' The initial's ink on a circle: dark or white, whichever has the higher
' WCAG contrast with it (dark on a tie).
function avatarInk(color as string) as string
    inks = avatarInks()
    circle = avatarLuminance(color) + 0.05#
    dark = circle / (avatarLuminance(inks.dark) + 0.05#)
    light = (avatarLuminance(inks.light) + 0.05#) / circle
    if dark >= light then return inks.dark
    return inks.light
end function

' WCAG's relative luminance of a color ("0xRRGGBBFF"), its channels
' linearized as sRGB's.
function avatarLuminance(color as string) as double
    weights = [0.2126#, 0.7152#, 0.0722#]
    luminance = 0#
    for i = 0 to 2
        c = Val(Mid(color, 3 + 2 * i, 2), 16) / 255#
        if c <= 0.04045#
            c = c / 12.92#
        else
            c = ((c + 0.055#) / 1.055#) ^ 2.4#
        end if
        luminance = luminance + weights[i] * c
    end for
    return luminance
end function

' The name's first character past white space, in capitals; "?" for a
' blank name.
function avatarInitial(name as string) as string
    count = Len(name)
    for at = 1 to count
        first = Mid(name, at, 1)
        point = Asc(first)
        if not avatarBlank(point)
            ' A character past the first 65536, as two UTF-16 surrogates.
            if point >= &hD800 and point < &hDC00 then first = Mid(name, at, 2)
            return UCase(first)
        end if
    end for
    return "?"
end function

' Whether a code point is white space as JavaScript's trim() has it,
' which the web trims a name by.
function avatarBlank(point as integer) as boolean
    if point = 32 or (point >= 9 and point <= 13) then return true
    if point < 160 then return false
    if point = 160 or point = &h1680 or point = &h2028 or point = &h2029 or point = &h202F or point = &h205F or point = &h3000 or point = &hFEFF then return true
    return point >= &h2000 and point <= &h200A
end function
