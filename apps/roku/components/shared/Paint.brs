' Pley's colors, shared by screens and items: the theme's (lib/Themes.brs,
' kept on m.global by the scene) and the markup recolored into them
' (lib/Colors.brs).

' The colors of the pages in the theme shown, by token (lib/Themes.brs's
' themeColors: m.c.bg, m.c.accentFg…), which the scene keeps on m.global
' (MainScene.brs's showTheme). A component reads them once, as it is made:
' a theme picked makes the screens again (or draws them again, those with
' a theme field), and their lists' items with them.
function uiColors() as object
    return m.global.colors
end function

' The colors of the players, which stay dark: a light theme's twin's.
function uiPlayerColors() as object
    return m.global.playerColors
end function

' The players' colors where dark, the pages' otherwise: for a component
' that pages and players share, which a player marks dark (FactSheet,
' PillButtons).
function uiColorsOf(dark as boolean) as object
    if dark then return uiPlayerColors()
    return uiColors()
end function

' An item's colors: a player's where its content says it is drawn in one
' (a dark field), the pages' otherwise. Items shared by pages and players
' (PanelRow, LiveRailItem) ask with each content.
function uiItemColors(content as dynamic) as object
    return uiColorsOf(content <> invalid and content.hasField("dark") and content.dark = true)
end function

' Gives node and everything under it colors' where its markup has
' Midnight's own (lib/Themes.brs's themeSentinels, kept on m.global): the
' markup is written in Midnight, so a screen in it needs nothing, and one
' in another theme is recolored once, as it is made. A component with a
' colors field colors itself.
sub uiRecolor(node as object, colors as object)
    if colors.key = "midnight/" then return
    if m.uiSentinels = invalid then m.uiSentinels = m.global.sentinels
    uiRecolorNode(node, colors, m.uiSentinels)
end sub

sub uiRecolorNode(node as object, colors as object, sentinels as object)
    kind = node.subtype()
    if kind = "Label" or kind = "Rectangle"
        fields = ["color"]
    else if kind = "Poster"
        fields = ["blendColor"]
    else if kind = "Group" or kind = "LayoutGroup" or kind = "Font" or kind = "Timer" or kind = "Animation" or kind = "MaskGroup"
        fields = []
    else
        ' A component given its own colors (the logo, a theme's preview)
        ' is drawn in them, whatever the screen's.
        if node.hasField("colors") then return
        fields = uiColorFields()
    end if
    for each field in fields
        if node.hasField(field)
            color = themeRecolor(node.getField(field), sentinels, colors)
            ' Through update, which takes a color written as a string as
            ' setting the field does (setField wants its own type).
            if color <> invalid
                changed = {}
                changed[field] = color
                node.update(changed)
            end if
        end if
    end for
    for each child in node.getChildren(-1, 0)
        uiRecolorNode(child, colors, sentinels)
    end for
end sub

' The color fields of the other nodes the markup colors (lists, the
' guide, the components that pass a color on).
function uiColorFields() as object
    return ["color", "blendColor", "focusedColor", "textColor", "focusedTextColor", "rowLabelColor", "focusBitmapBlendColor", "focusFootprintBlendColor", "channelInfoTextColor", "programTitleColor", "programTitleFocusedColor", "timeLabelColor", "nowBarBlendColor"]
end function

' A color a library (lib/) gives in Midnight's, such as a request's stage,
' in colors.
function uiThemed(color as string, colors as object) as string
    if m.uiSentinels = invalid then m.uiSentinels = m.global.sentinels
    themed = themeRecolor(color, m.uiSentinels, colors)
    if themed = invalid then return color
    return themed
end function

' Dialogs and keyboards in the theme's colors: the Scene's palette.
function uiPalette(colors as object) as object
    palette = CreateObject("roSGNode", "RSGPalette")
    palette.colors = {
        DialogBackgroundColor: colors.surface,
        DialogItemColor: colors.fg,
        DialogTextColor: colors.fg,
        DialogFocusColor: colors.accent,
        DialogFocusItemColor: colors.accentFg,
        DialogSecondaryTextColor: colors.muted,
        DialogSecondaryItemColor: colors.muted,
        DialogInputFieldColor: colors.surface3,
        DialogKeyboardColor: colors.surface2,
        DialogFootprintColor: colors.surface3
    }
    return palette
end function
