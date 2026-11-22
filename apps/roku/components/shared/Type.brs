' Pley's type, shared by screens and items (I18n.brs for the language).

' Pley's type in Roboto, as the Android TV app's (PleyType.Tv, its sp
' doubled for 1080 lines): a Font node of weight ("regular", "medium" or
' "bold") and size, one per pair in this component. Chinese is in the
' Roku's own font, which has its characters: Roboto has the Latin ones.
function uiFont(weight as string, size as integer) as object
    if m.uiFonts = invalid then m.uiFonts = {}
    key = weight + size.ToStr()
    font = m.uiFonts[key]
    if font <> invalid then return font
    font = CreateObject("roSGNode", "Font")
    if Left(i18nLanguage(), 2) = "zh"
        font.uri = "font:SystemFontFile"
        if weight = "bold" then font.uri = "font:BoldSystemFontFile"
    else
        files = { regular: "400", medium: "500", bold: "700" }
        font.uri = "pkg:/fonts/roboto-" + files[weight] + ".ttf"
    end if
    font.size = size
    m.uiFonts[key] = font
    return font
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
' uri; Roboto's have theirs), by size, as near Android TV's styles as the
' size tells (a system font's node doesn't say whether it is bold: 45 px
' and up are titles). The scene calls it on each screen it makes; what a
' screen sets itself in Roboto stays. Chinese keeps the Roku's own font.
sub uiRestyle(node as object)
    if Left(i18nLanguage(), 2) = "zh" then return
    for each field in ["font", "focusedFont", "textFont", "focusedTextFont", "rowLabelFont"]
        if node.hasField(field)
            font = node.getField(field)
            if type(font) = "roSGNode" and font.uri = ""
                styled = uiSystemFont(font.size)
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
