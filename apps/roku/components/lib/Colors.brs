' Colors as Roku draws them ("0xRRGGBBAA"), in a theme (Themes.brs): the
' markup's recolored, a color at another alpha, the dim behind a panel and
' the words on a fill. Plain functions, for screens, items and tests.

' A color of a screen's markup in colors: one of Midnight's (sentinels,
' themeSentinels) takes colors' for its token, keeping its own alpha
' ("0x15171ACC", bg at 80 %, is Twilight's bg at 80 %); any other is
' returned as it is. value is a field's, a string or Roku's integer;
' invalid for a color that doesn't change.
function themeRecolor(value as dynamic, sentinels as object, colors as object) as dynamic
    hex = themeHex(value)
    if hex = "" then return invalid
    token = sentinels[Left(hex, 6)]
    if token = invalid then return invalid
    color = Left(colors[token], 8) + Right(hex, 2)
    if UCase(color) = "0X" + hex then return invalid
    return color
end function

' A color as RRGGBBAA, from "0xRRGGBBAA" or Roku's integer for it; "" for
' anything else.
function themeHex(value as dynamic) as string
    if GetInterface(value, "ifString") <> invalid
        if Len(value) = 10 and LCase(Left(value, 2)) = "0x" then return UCase(Mid(value, 3))
        return ""
    end if
    if GetInterface(value, "ifInt") = invalid then return ""
    digits = "0123456789ABCDEF"
    hex = ""
    for shift = 28 to 0 step -4
        hex = hex + Mid(digits, ((value >> shift) and 15) + 1, 1)
    end for
    return hex
end function

' A color at another alpha (two hex digits): fg at "1F" for a pill at
' rest.
function themeAlpha(color as string, alpha as string) as string
    return Left(color, 8) + alpha
end function

' The dim over a page behind a panel or a dialog: the theme's shade at
' share of its own alpha (the web's bg-shade/60 is 0.6), black in the dark
' themes and a tinted one in the light ones.
function themeShade(colors as object, share as float) as string
    alpha = Val(Right(colors.shade, 2), 16)
    return Left(colors.shade, 8) + themeByte(Int(alpha * share + 0.5))
end function

' The words' color on a fill of colors: accent-fg on the accent, danger-fg
' on danger, and bg on the other states (success, warning, a gone
' request's subtle).
function themeWordsOn(colors as object, fill as string) as string
    rgb = UCase(Left(fill, 8))
    if rgb = UCase(Left(colors.accent, 8)) then return colors.accentFg
    if rgb = UCase(Left(colors.danger, 8)) then return colors.dangerFg
    return colors.bg
end function

' A picture of the Live TV guide (Roku's TimeGrid takes pictures, not
' colors) in a theme's color: a program's box ("cell", surface-2), the
' focus ("focus", the accent) or what has aired ("past", surface), drawn
' for every theme by scripts/make_images.py.
function themeGuidePicture(kind as string, color as string) as string
    return "pkg:/images/guide/" + kind + "-" + UCase(Mid(color, 3, 6)) + ".9.png"
end function

' A color ("0xRRGGBBAA") within Roku's broadcast-safe range, which its
' certification asks of a channel's colors and bslint's color-cert rule
' holds the code to: its luma (BT.709, on the 0-255 channels) no darker
' than #161616's and no lighter than #EBEBEB's. Moved the least way that
' keeps its hue: each channel raised (or lowered) by the same amount, one
' at its end leaving the rest to the others; its alpha kept. The same rule
' as scripts/broadcast-safe.mjs, which writes ThemeTable.brs's colors.
function themeBroadcastSafe(color as string) as string
    c = [themeChannel(color, 0), themeChannel(color, 1), themeChannel(color, 2)]
    weights = [0.2126#, 0.7152#, 0.0722#]
    darkest = themeLuma([22, 22, 22])
    lightest = themeLuma([235, 235, 235])
    start = themeLuma(c)
    if start >= darkest and start <= lightest then return "0x" + UCase(Mid(color, 3))
    up = start < darkest
    target = lightest
    if up then target = darkest
    for pass = 1 to 3
        weight = 0.0#
        for k = 0 to 2
            if (up and c[k] < 255) or (not up and c[k] > 0) then weight = weight + weights[k]
        end for
        if weight = 0 then exit for
        stepBy = (target - themeLuma(c)) / weight
        for k = 0 to 2
            if (up and c[k] < 255) or (not up and c[k] > 0)
                v = c[k] + stepBy
                if v < 0 then v = 0
                if v > 255 then v = 255
                c[k] = v
            end if
        end for
    end for
    for k = 0 to 2
        c[k] = Int(c[k] + 0.5)
    end for
    while themeLuma(c) < darkest
        for k = 0 to 2
            if c[k] < 255 then c[k] = c[k] + 1
        end for
    end while
    while themeLuma(c) > lightest
        for k = 0 to 2
            if c[k] > 0 then c[k] = c[k] - 1
        end for
    end while
    return "0x" + themeByte(c[0]) + themeByte(c[1]) + themeByte(c[2]) + UCase(Mid(color, 9, 2))
end function

' A color's channel (0 red, 1 green, 2 blue) from "0xRRGGBBAA", 0 to 255.
function themeChannel(color as string, at as integer) as integer
    return Val(Mid(color, 3 + at * 2, 2), 16)
end function

' A color's luma (BT.709) on its 0-255 channels, as bslint's color-cert
' rule reckons it.
function themeLuma(c as object) as double
    return 0.2126# * c[0] + 0.7152# * c[1] + 0.0722# * c[2]
end function

' 0 to 255 as two hex digits.
function themeByte(n as integer) as string
    if n < 0 then n = 0
    if n > 255 then n = 255
    digits = "0123456789ABCDEF"
    return Mid(digits, Int(n / 16) + 1, 1) + Mid(digits, (n mod 16) + 1, 1)
end function
