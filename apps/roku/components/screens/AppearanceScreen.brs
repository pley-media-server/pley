' Appearance (AppearanceScreen.xml): the themes in two rows of five cards,
' then the accents in a row of chips. The arrows move between them, Up and
' Down between the cards and the chips landing on the pick in use; OK
' picks one: the scene keeps it for the profile and shows it at once
' (MainScene.brs's onThemeChoice), and this screen draws itself again in
' it. Left from a row's first card or chip opens the rail.

sub init()
    m.top.focusable = true
    m.themes = m.top.findNode("themes")
    m.accents = m.top.findNode("accents")
    m.top.findNode("title").font = uiType("pageTitle")
    for each id in ["themeTitle", "accentTitle"]
        m.top.findNode(id).font = uiType("bodyLarge")
    end for
    for each id in ["themeHint", "accentHint"]
        m.top.findNode(id).font = uiType("body")
    end for
    m.themeIds = themeIds()
    m.accentIds = [""]
    m.accentIds.Append(themeAccentIds())
    ' Where the focus is: "themes" or "accents", and which there; it starts
    ' on the theme in use, or the accent (onActive).
    m.zone = "themes"
    m.at = themeInUse()
    m.placed = false
    m.top.observeField("active", "onActive")
    m.top.observeField("focusedChild", "look")
    draw()
end sub

sub onActive()
    if not m.top.active then return
    if not m.placed
        m.placed = true
        params = m.top.params
        if params <> invalid and params.focus = "accent"
            m.zone = "accents"
            m.at = accentInUse()
        end if
    end if
    m.top.setFocus(true)
    look()
end sub

function themeInUse() as integer
    for i = 0 to m.themeIds.Count() - 1
        if m.themeIds[i] = m.global.theme then return i
    end for
    return 0
end function

function accentInUse() as integer
    for i = 0 to m.accentIds.Count() - 1
        if m.accentIds[i] = m.global.accent then return i
    end for
    return 0
end function

' --- Drawing ---

' The screen in the theme shown: its words, the cards and the chips.
sub draw()
    m.c = uiColors()
    m.theme = m.global.theme
    m.accent = m.global.accent
    m.top.findNode("title").update({ text: t("settings.appearance.title"), color: m.c.fg })
    sectionLine("themeTitle", "themeHint", t("settings.appearance.theme"), t("settings.appearance.hint"))
    sectionLine("accentTitle", "accentHint", t("settings.appearance.accent.title"), t("settings.appearance.accent.hint"))
    drawThemes()
    drawAccents()
    look()
end sub

' A section's label, and its hint after it on the same line.
sub sectionLine(titleId as string, hintId as string, title as string, hint as string)
    label = m.top.findNode(titleId)
    label.update({ text: title, color: m.c.fg, width: 0 })
    words = m.top.findNode(hintId)
    words.update({ text: hint, color: m.c.muted })
    x = 192 + Int(label.boundingRect().width) + 32
    words.translation = [x, words.translation[1]]
    words.width = 1824 - x
end sub

' A card for each theme: its preview (a small Pley in its colors, under
' the accent picked), its name and caption; the theme in use in the
' accent's border, with a check at the preview's bottom right, clear of
' its accent pill at the top. Focused, the ring stands in for the border:
' the two side by side read as a double line.
sub drawThemes()
    m.themes.removeChildrenIndex(m.themes.getChildCount(), 0)
    m.cards = []
    for i = 0 to m.themeIds.Count() - 1
        id = m.themeIds[i]
        picked = id = m.theme
        card = m.themes.createChild("Group")
        card.translation = [(i mod 5) * 332, Int(i / 5) * 334]
        if picked then addPoster(card, "pkg:/images/ui/r12.9.png", 0, 0, 302, 300, themeAlpha(m.c.accent, "14"))
        edge = m.c.border
        if picked then edge = m.c.accent
        edges = [addPoster(card, "pkg:/images/ui/edge.9.png", 0, 0, 302, 300, edge)]
        if picked then edges.Push(addPoster(card, "pkg:/images/ui/edge.9.png", 1, 1, 300, 298, edge))
        preview = card.createChild("Group")
        preview.translation = [12, 12]
        drawPreview(preview, themeColors(id, m.accent), id)
        ' 10 px in from the preview's corner (12 + 278 and 12 + 156).
        if picked then addCheck(card, 240, 118)
        name = addLabel(card, t("settings.appearance.names." + id), "bodyLarge", m.c.fg, 16, 180, 280)
        name.maxLines = 1
        ' Its caption on up to two lines, a size under the name's.
        about = addLabel(card, t("settings.appearance.about." + id), "body", m.c.muted, 16, 218, 280)
        about.update({ font: uiFont("regular", 26), wrap: true, maxLines: 2, lineSpacing: 2 })
        ring = addPoster(card, "pkg:/images/ui/ring12.9.png", -6, -6, 314, 312, m.c.accentHover)
        ring.visible = false
        m.cards.Push({ ring: ring, edges: edges })
    end for
end sub

' The web's preview of a theme (ThemeCard.tsx), 278 × 156: its sidebar
' with the accent's bar and three lines, a heading with a button, a line
' and a row of posters, the first one framed in the accent; on its
' backdrop where it has one.
sub drawPreview(preview as object, colors as object, theme as string)
    addRect(preview, colors.bg, 0, 0, 278, 156)
    backdrop = themeBackdrop(theme)
    if backdrop <> ""
        picture = addPoster(preview, backdrop, 0, 0, 278, 156, "")
        picture.loadWidth = 278
        picture.loadHeight = 156
    end if
    addRect(preview, colors.surface, 0, 0, 70, 156)
    sidebar = themeSidebar(theme)
    if sidebar <> "" then addPoster(preview, sidebar, 0, 0, 70, 156, "")
    addRect(preview, colors.border, 70, 0, 2, 156)
    addBar(preview, colors.accent, 10, 12, 36, 8)
    for each line in [[26, 50], [40, 34], [54, 42]]
        addBar(preview, themeAlpha(colors.fg, "26"), 10, line[0], line[1], 8)
    end for
    addBar(preview, themeAlpha(colors.fg, "CC"), 84, 12, 92, 10)
    addBar(preview, colors.accent, 218, 11, 48, 12)
    addBar(preview, themeAlpha(colors.muted, "80"), 84, 32, 62, 8)
    for i = 0 to 3
        x = 84 + i * 46
        if i = 0 then addRect(preview, colors.accent, x - 2, 82, 44, 66)
        addRect(preview, colors.surface3, x, 84, 40, 62)
    end for
    ' Its edge, the text color at 10 %, as the web's.
    edge = themeAlpha(m.c.fg, "1A")
    addRect(preview, edge, 0, 0, 278, 2)
    addRect(preview, edge, 0, 154, 278, 2)
    addRect(preview, edge, 0, 2, 2, 152)
    addRect(preview, edge, 276, 2, 2, 152)
end sub

' A chip for each accent, Default first: its swatch, in the color it gives
' the theme shown, and its name; the accent in use filled with the accent,
' a check in place of its swatch.
sub drawAccents()
    m.accents.removeChildrenIndex(m.accents.getChildCount(), 0)
    m.chips = []
    x = 0
    for each id in m.accentIds
        picked = id = m.accent
        colors = themeColors(m.theme, id)
        words = t("settings.appearance.accent.own")
        if id <> "" then words = t("settings.appearance.accent.names." + id)
        chip = m.accents.createChild("Group")
        chip.translation = [x, 0]
        edges = []
        label = CreateObject("roSGNode", "Label")
        label.font = uiType("bodyLarge")
        label.text = words
        width = 80 + Int(label.boundingRect().width) + 2 + 32
        if picked
            addPoster(chip, "pkg:/images/ui/r12.9.png", 0, 0, width, 80, m.c.accent)
            addPoster(chip, "pkg:/images/ui/edge-round.png", 24, 20, 40, 40, themeAlpha(m.c.accentFg, "99"))
            addIcon(chip, "check", 32, 28, 24, m.c.accentFg)
            label.color = m.c.accentFg
        else
            edges.Push(addPoster(chip, "pkg:/images/ui/edge.9.png", 0, 0, width, 80, m.c.border))
            addPoster(chip, "pkg:/images/ui/circle.png", 24, 20, 40, 40, colors.accent)
            addPoster(chip, "pkg:/images/ui/edge-round.png", 24, 20, 40, 40, themeAlpha(m.c.fg, "33"))
            label.color = m.c.fg
        end if
        label.update({ translation: [80, 0], height: 80, vertAlign: "center" })
        chip.appendChild(label)
        ring = addPoster(chip, "pkg:/images/ui/ring12.9.png", -6, -6, width + 12, 92, m.c.accentHover)
        ring.visible = false
        m.chips.Push({ ring: ring, edges: edges, x: x, width: width })
        x = x + width + 22
    end for
end sub

' The focus, as the app's tiles show it: the accent's ring around the card
' or chip, in place of its own edge.
sub look()
    if m.cards = invalid then return
    focused = m.top.isInFocusChain()
    for i = 0 to m.cards.Count() - 1
        showRing(m.cards[i], focused and m.zone = "themes" and i = m.at)
    end for
    for i = 0 to m.chips.Count() - 1
        showRing(m.chips[i], focused and m.zone = "accents" and i = m.at)
    end for
end sub

sub showRing(item as object, shown as boolean)
    item.ring.visible = shown
    for each edge in item.edges
        edge.visible = not shown
    end for
end sub

function addRect(parent as object, color as string, x as integer, y as integer, width as integer, height as integer) as object
    node = parent.createChild("Rectangle")
    node.update({ color: color, translation: [x, y], width: width, height: height })
    return node
end function

' A picture, tinted in color ("" for as it is).
function addPoster(parent as object, uri as string, x as integer, y as integer, width as integer, height as integer, color as string) as object
    node = parent.createChild("Poster")
    node.update({ uri: uri, translation: [x, y], width: width, height: height })
    if color <> "" then node.blendColor = color
    return node
end function

' A line of the preview, its ends round.
sub addBar(parent as object, color as string, x as integer, y as integer, width as integer, height as integer)
    addPoster(parent, "pkg:/images/ui/bar.9.png", x, y, width, height, color)
end sub

sub addIcon(parent as object, name as string, x as integer, y as integer, size as integer, color as string)
    addPoster(parent, "pkg:/images/ui/icon-" + name + "-40.png", x, y, size, size, color)
end sub

' The mark of the theme in use: a check on an accent disc.
sub addCheck(parent as object, x as integer, y as integer)
    addPoster(parent, "pkg:/images/ui/circle.png", x, y, 40, 40, m.c.accent)
    addIcon(parent, "check", x + 8, y + 8, 24, m.c.accentFg)
end sub

function addLabel(parent as object, text as string, style as string, color as string, x as integer, y as integer, width as integer) as object
    node = parent.createChild("Label")
    node.update({ text: text, font: uiType(style), color: color, translation: [x, y], width: width })
    return node
end function

' --- Keys ---

' Left and Right along a row; Up and Down between the rows of cards, and
' between the cards and the chips, onto the pick in use. OK picks.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "OK"
        pick()
        return true
    end if
    if m.zone = "themes"
        column = m.at mod 5
        row = Int(m.at / 5)
        if key = "left"
            if column = 0 then return false
            m.at = m.at - 1
        else if key = "right"
            if column < 4 and m.at < m.themeIds.Count() - 1 then m.at = m.at + 1
        else if key = "up"
            if row > 0 then m.at = m.at - 5
        else if key = "down"
            if row = 0
                m.at = m.at + 5
                if m.at > m.themeIds.Count() - 1 then m.at = m.themeIds.Count() - 1
            else
                m.zone = "accents"
                m.at = accentInUse()
            end if
        else
            return false
        end if
    else
        if key = "left"
            if m.at = 0 then return false
            m.at = m.at - 1
        else if key = "right"
            if m.at < m.chips.Count() - 1 then m.at = m.at + 1
        else if key = "up"
            m.zone = "themes"
            m.at = themeInUse()
        else if key = "down"
            ' The last row: nothing under it.
        else
            return false
        end if
    end if
    look()
    return true
end function

' The card or chip focused, picked: shown at once, kept for the profile.
sub pick()
    theme = m.theme
    accent = m.accent
    if m.zone = "themes" then theme = m.themeIds[m.at] else accent = m.accentIds[m.at]
    m.top.getScene().themeChoice = { theme: theme, accent: accent }
end sub
