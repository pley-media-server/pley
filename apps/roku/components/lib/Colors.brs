' Colors as Roku draws them ("0xRRGGBBAA"), in a theme (Themes.brs): the
' markup's recolored, a color at another alpha, the dim behind a panel and
' the words on a fill. Plain functions, for screens, items and tests.

' A color of a screen's markup in colors: one of Midnight's (sentinels,
' themeSentinels) takes colors' for its token, keeping its own alpha
' ("0x0B0D10CC", bg at 80 %, is Twilight's bg at 80 %); any other is
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

' 0 to 255 as two hex digits.
function themeByte(n as integer) as string
    if n < 0 then n = 0
    if n > 255 then n = 255
    digits = "0123456789ABCDEF"
    return Mid(digits, Int(n / 16) + 1, 1) + Mid(digits, (n mod 16) + 1, 1)
end function
