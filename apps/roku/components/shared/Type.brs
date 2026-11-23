' Pley's type, shared by screens and items (I18n.brs for the language).

' Pley's type in Roboto, as the Android TV app's (PleyType.Tv, its sp
' doubled for 1080 lines): a Font node of weight ("regular", "medium" or
' "bold") and size, one per family, weight and size in this component.
function uiFont(weight as string, size as integer) as object
    return uiFontOf(uiFontFamily(), weight, size)
end function

' The family the language shown is drawn in: Roboto, or for Chinese Noto
' Sans SC or TC, which have its characters where Roboto and the Roku's own
' font have none (fonts/, cut to the app's words by
' scripts/subset-roku-cjk.py).
function uiFontFamily() as string
    return i18nFontFamily(i18nLanguage())
end function

function uiFontOf(family as string, weight as string, size as integer) as object
    if m.uiFonts = invalid then m.uiFonts = {}
    key = family + weight + size.ToStr()
    font = m.uiFonts[key]
    if font <> invalid then return font
    font = CreateObject("roSGNode", "Font")
    files = { regular: "400", medium: "500", bold: "700" }
    font.uri = "pkg:/fonts/" + family + "-" + files[weight] + ".ttf"
    font.size = size
    m.uiFonts[key] = font
    return font
end function

' A font for words that may be Chinese where the language shown isn't, as
' the language list's 简体中文 and 繁體中文: Noto Sans SC, which has their
' characters and the Latin ones.
function uiFontFor(text as string, weight as string, size as integer) as object
    if uiFontFamily() = "roboto" and i18nHasCjk(text) then return uiFontOf("noto-sc", weight, size)
    return uiFont(weight, size)
end function

' The weight of one of Pley's fonts, by its file; "" for another font.
function uiFontWeight(font as object) as string
    if Left(font.uri, 11) <> "pkg:/fonts/" then return ""
    weights = { "400.ttf": "regular", "500.ttf": "medium", "700.ttf": "bold" }
    weight = weights[Right(font.uri, 7)]
    if weight = invalid then return ""
    return weight
end function

' A style of PleyType.Tv by name: heroTitle, pageTitle, section, title,
' bodyLarge, body, label, caption (captionBold, a focused tile's), badge.
function uiType(style as string) as object
    styles = {
        heroTitle: ["bold", 76],
        pageTitle: ["bold", 56],
        section: ["bold", 36],
        title: ["bold", 32],
        bodyLarge: ["regular", 30],
        body: ["regular", 28],
        label: ["medium", 28],
        caption: ["medium", 26],
        captionBold: ["bold", 26],
        badge: ["bold", 22]
    }
    found = styles[style]
    if found = invalid then found = styles.body
    return uiFont(found[0], found[1])
end function

' Gives a screen Pley's type wherever it still has the Roku's: every font
' of its labels, lists and button groups whose node is a system font (no
' uri), by size, as near Android TV's styles as the size tells (a system
' font's node doesn't say whether it is bold: 45 px and up are titles).
' The scene calls it on each screen it makes; what a screen sets itself in
' Pley's type stays, in the family of the language shown (a font of the
' other family, from before the language changed, takes this one's).
sub uiRestyle(node as object)
    family = "pkg:/fonts/" + uiFontFamily() + "-"
    for each field in ["font", "focusedFont", "textFont", "focusedTextFont", "rowLabelFont"]
        if node.hasField(field)
            font = node.getField(field)
            if type(font) = "roSGNode"
                styled = invalid
                if font.uri = ""
                    styled = uiSystemFont(font.size)
                    ' A row's title is a section's, as Android TV's rows have.
                    if field = "rowLabelFont" then styled = uiType("section")
                else if Left(font.uri, Len(family)) <> family and uiFontWeight(font) <> ""
                    styled = uiFont(uiFontWeight(font), font.size)
                end if
                if styled <> invalid then node.setField(field, styled)
            end if
        end if
    end for
    for each child in node.getChildren(-1, 0)
        uiRestyle(child)
    end for
end sub

' Roboto for a Roku system font of size (FHD): Tiny 24, Smallest 27, Smaller
' 30, Small 33, Medium 36, Large 45, ExtraLarge 54.
function uiSystemFont(size as integer) as dynamic
    if size <= 0 then return invalid
    if size <= 24 then return uiFont("medium", 22)
    if size <= 27 then return uiFont("regular", 26)
    if size <= 30 then return uiFont("regular", 28)
    if size <= 33 then return uiFont("regular", 30)
    if size <= 36 then return uiFont("medium", 32)
    if size <= 45 then return uiFont("bold", 44)
    return uiFont("bold", 56)
end function
