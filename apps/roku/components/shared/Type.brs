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
' bodyLarge, body, label, caption, badge.
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
        badge: ["bold", 22]
    }
    found = styles[style]
    if found = invalid then found = styles.body
    return uiFont(found[0], found[1])
end function
