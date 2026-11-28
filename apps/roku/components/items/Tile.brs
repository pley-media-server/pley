sub init()
    m.c = uiColors()
    uiRecolor(m.top, m.c)
    m.art = m.top.findNode("art")
    m.grow = m.top.findNode("grow")
    m.wellWidth = m.top.findNode("wellWidth")
    m.wellHeight = m.top.findNode("wellHeight")
    m.wellPlace = m.top.findNode("wellPlace")
    m.posterWidth = m.top.findNode("posterWidth")
    m.posterHeight = m.top.findNode("posterHeight")
    m.posterPlace = m.top.findNode("posterPlace")
    m.mask = m.top.findNode("mask")
    m.maskSizer = m.top.findNode("maskSizer")
    m.maskPlace = m.top.findNode("maskPlace")
    m.imageWidth = 0
    m.imageHeight = 0
    ' The size and shape laid out, and the marks measured, for onLayout.
    m.shapeShown = ""
    m.marksShown = ""
    m.well = m.top.findNode("well")
    m.fallback = m.top.findNode("fallback")
    m.poster = m.top.findNode("poster")
    m.hairline = m.top.findNode("hairline")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
    m.watched = m.top.findNode("watched")
    m.ring = m.top.findNode("ring")
    m.title = m.top.findNode("title")
    m.subtitle = m.top.findNode("subtitle")
    m.badgeBack = m.top.findNode("badgeBack")
    m.badge = m.top.findNode("badge")
    m.qualityBacks = [m.top.findNode("qualityBack0"), m.top.findNode("qualityBack1")]
    m.qualities = [m.top.findNode("quality0"), m.top.findNode("quality1")]
    m.captionFont = uiType("caption")
    m.boldFont = uiType("captionBold")
    m.title.font = m.captionFont
    m.subtitle.font = m.captionFont
    m.fallback.font = uiType("label")
    m.badge.font = uiType("badge")
    for each label in m.qualities
        label.font = uiType("badge")
    end for
    ' Round corners where the Roku draws them (OS 16's Effect, on the GPU),
    ' as Android TV's tiles have: one Effect for the well and the artwork.
    ' The artwork takes it only if told before its picture loads.
    ' Named through a variable: the compiler doesn't know OS 16's nodes yet.
    effect = "Effect"
    m.round = CreateObject("roSGNode", effect)
    if m.round <> invalid and m.round.supported
        m.round.borderRadius = [8]
        m.well.effect = m.round
        m.poster.shaderCompatible = true
        m.poster.effect = m.round
    end if
    m.wide = false
    m.square = false
    m.person = false
    m.progress = 0.0
    m.content = invalid
    ' The focus as the tile shows it (onFocus sets only what changes), once
    ' it has shown any.
    m.shown = false
    m.shownList = false
    m.shownFocused = false
end sub

' Bound again as the list scrolls: only a new title changes the tile. A
' title without a picture, or whose picture fails, shows Pley's mark in
' its frame (the poster's failed bitmap, drawn for its shape with its
' library's glyph), and not its title: the line under it names it. The
' mark needs no observer of its own.
sub onContent()
    item = m.top.itemContent
    if item = invalid then return
    if m.content <> invalid and m.content.isSameNode(item) then return
    m.content = item
    m.shown = false
    m.wide = item.hasField("wide") and item.wide = true
    m.square = item.hasField("square") and item.square = true
    m.person = item.hasField("kind") and (item.kind = "person" or item.kind = "cast")
    m.progress = 0.0
    if item.hasField("progress") then m.progress = item.progress
    m.watched.visible = item.hasField("watched") and item.watched = true
    m.title.text = item.title
    ' See all, a row's Try again or note, a grid's Look further and Request
    ' more seasons: their words in the well only.
    words = item.hasField("kind") and (item.kind = "more" or item.kind = "retry" or item.kind = "note" or item.kind = "further" or item.kind = "askMore")
    if words then m.title.text = ""
    m.subtitle.text = item.subtitle
    m.subtitle.color = m.c.muted
    ' Colors from lib/ come in Midnight's, the theme's here.
    if item.hasField("subtitleColor") then m.subtitle.color = uiThemed(item.subtitleColor, m.c)
    ' A Live TV channel without its logo shows its name in the well, as the
    ' other apps do, and no mark; a person without a photo, their initials
    ' in the circle, their name being under it already.
    kind = ""
    if item.hasField("kind") then kind = item.kind
    named = words or kind = "channel"
    m.fallback.text = ""
    if named then m.fallback.text = item.title
    if m.person
        m.fallback.text = tileInitials(item.title)
        m.fallback.font = uiType("section")
    else
        m.fallback.font = uiType("label")
    end if
    badge = ""
    if item.hasField("badge") then badge = item.badge
    m.badge.text = badge
    m.badge.visible = badge <> ""
    m.badgeBack.visible = badge <> ""
    if badge <> "" and item.hasField("badgeColor")
        fill = uiThemed(item.badgeColor, m.c)
        m.badgeBack.blendColor = fill
        m.badge.color = themeWordsOn(m.c, fill)
    end if
    qualities = []
    if item.hasField("qualityBadges") and item.qualityBadges <> invalid then qualities = item.qualityBadges
    for i = 0 to m.qualities.Count() - 1
        text = ""
        if i < qualities.Count() then text = qualities[i]
        ' Shown by onLayout, which knows which fit.
        m.qualities[i].text = text
    end for
    ' The scene's agent carries the token; it changes with each refresh.
    m.poster.setHttpAgent(m.top.getScene().getHttpAgent())
    if m.loadWatched <> true
        m.loadWatched = true
        m.poster.observeField("loadStatus", "onLoaded")
    end if
    onLayout()
    ' A frame without a title is one that loads (a row's skeleton, a
    ' library's place a jump passed over): it stays empty. A tile of words
    ' alone (Request more seasons, See all), bound where a picture was,
    ' shows none: a Poster keeps its last bitmap when its uri becomes "".
    placeholder = ""
    if not named and not m.person and item.title <> "" then placeholder = placeholderUri(item)
    m.poster.failedBitmapUri = placeholder
    image = ""
    if item.hasField("image") and item.image <> invalid then image = item.image
    if image = "" then image = placeholder
    m.poster.visible = image <> ""
    m.poster.uri = image
    onFocus()
end sub

' Pley's mark for this tile's kind and shape (lib/Mark.brs).
function placeholderUri(item as object) as string
    kind = ""
    if item.hasField("kind") then kind = item.kind
    detail = ""
    if item.hasField("tmdbKind") then detail = item.tmdbKind
    if item.hasField("playlistKind") then detail = item.playlistKind
    return markPlaceholderUri(markTileGlyph(kind, detail), m.wide, m.square)
end function

' The tile's item is 24 px wider than its artwork and 30 px taller: room
' at the left and top for the focused one's growth and ring, which a list
' cuts at its edges (the lists move 24 px left to match): grown, a wide
' tile and its ring reach 23 px out. At the right they reach as far past
' the item, so a row of them ends 24 px short of the screen's edge: the
' last tile the focus rests on keeps its ring.
sub onLayout()
    w = m.top.width - 24
    if w <= 0 then return
    imageHeight = Int(w * 1.5)
    if m.wide then imageHeight = Int(w * 9 / 16)
    if m.square or m.person then imageHeight = Int(w)
    ' The shape's layout only when the size or the shape changed: a list
    ' binds its tiles to title after title of the same shape as it scrolls.
    shape = w.ToStr() + " " + imageHeight.ToStr()
    if m.person then shape = shape + " person"
    if shape <> m.shapeShown
        m.shapeShown = shape
        layoutShape(w, imageHeight)
    end if
    layoutMarks(w, imageHeight)
end sub

' The artwork, its well, ring and growth for its size and shape.
sub layoutShape(w as float, imageHeight as integer)
    ' A person's picture is a circle, its ring one too; the others' corners
    ' round 8 px.
    if m.round <> invalid and m.round.supported
        radius = 8
        if m.person then radius = w / 2
        m.round.borderRadius = [radius]
        ' A person's picture takes the mask instead (onLoaded keeps the
        ' others' shapes right too).
        if m.person
            m.poster.effect = invalid
        else
            m.poster.effect = m.round
        end if
    end if
    if m.person
        m.mask.maskUri = "pkg:/images/ui/circle.png"
    else
        m.mask.maskUri = ""
    end if
    m.hairline.visible = not m.person
    if m.person
        m.ring.uri = "pkg:/images/ui/ring-round.png"
    else
        m.ring.uri = "pkg:/images/ui/ring.9.png"
    end if
    m.imageWidth = w
    m.imageHeight = imageHeight
    m.grow.translation = [w / 2, imageHeight / 2]
    m.art.translation = [-w / 2, -imageHeight / 2]
    ' The well and the artwork grow by their size, from the middle.
    grownWidth = w * 1.08
    grownHeight = imageHeight * 1.08
    grownPlace = [-(grownWidth - w) / 2, -(grownHeight - imageHeight) / 2]
    for each pair in [[m.wellWidth, m.posterWidth, [w, grownWidth]], [m.wellHeight, m.posterHeight, [imageHeight, grownHeight]]]
        pair[0].keyValue = pair[2]
        pair[1].keyValue = pair[2]
    end for
    m.wellPlace.keyValue = [[0, 0], grownPlace]
    m.posterPlace.keyValue = [[0, 0], grownPlace]
    m.maskSizer.keyValue = [[w, imageHeight], [grownWidth, grownHeight]]
    m.maskPlace.keyValue = [[0, 0], grownPlace]
    growTo(m.grow.scale[0] > 1)
    ' Decoded at the size it's drawn: small textures for small devices.
    m.poster.loadWidth = w
    m.poster.loadHeight = imageHeight
    m.hairline.width = w
    m.hairline.height = imageHeight
    m.ring.translation = [-6, -6]
    m.ring.width = w + 12
    m.ring.height = imageHeight + 12
    m.fallback.translation = [16, 16]
    m.fallback.width = w - 32
    m.fallback.height = imageHeight - 32
    m.watched.translation = [w - 52, 12]
    m.title.translation = [0, imageHeight + 28]
    m.title.width = w
    m.subtitle.translation = [0, imageHeight + 62]
    m.subtitle.width = w
end sub

' What the title has on its artwork: how far along, its badge and its
' quality badges, measured again only when their words change.
sub layoutMarks(w as float, imageHeight as integer)
    ' The round corners again for each title: onLoaded takes them off a
    ' picture of another shape.
    if m.round <> invalid and m.round.supported and not m.person then m.poster.effect = m.round
    ' How far along: an 8 px bar along the artwork's foot.
    m.track.translation = [0, imageHeight - 8]
    m.track.width = w
    m.track.height = 8
    m.fill.translation = [0, imageHeight - 8]
    m.fill.width = Int(w * m.progress)
    m.fill.height = 8
    m.track.visible = m.progress > 0
    m.fill.visible = m.progress > 0
    marks = m.shapeShown + " " + m.badge.text
    if m.progress > 0 then marks = marks + " along"
    for each label in m.qualities
        marks = marks + " " + label.text
    end for
    if marks = m.marksShown then return
    m.marksShown = marks
    ' The badge at the artwork's top left, as wide as its words; cut short
    ' only when they don't fit.
    if m.badge.visible
        m.badge.width = 0
        textWidth = m.badge.boundingRect().width
        if textWidth > w - 48
            textWidth = w - 48
            m.badge.width = textWidth
        end if
        m.badge.translation = [24, 12]
        m.badgeBack.translation = [12, 12]
        m.badgeBack.width = textWidth + 24
    end if
    ' The quality badges side by side at the artwork's foot, 12 px in, above
    ' how far along it is; one that doesn't fit whole isn't drawn.
    x = 12
    y = imageHeight - 48
    if m.progress > 0 then y = imageHeight - 56
    for i = 0 to m.qualities.Count() - 1
        label = m.qualities[i]
        back = m.qualityBacks[i]
        label.visible = false
        back.visible = false
        if label.text <> ""
            label.width = 0
            chipWidth = label.boundingRect().width + 24
            fits = x + chipWidth <= w - 12
            label.visible = fits
            back.visible = fits
            label.translation = [x + 12, y]
            back.translation = [x, y]
            back.width = chipWidth
            x = x + chipWidth + 12
        end if
    end for
end sub

' The list's focusPercent grows the tile and fades its ring in as focus
' comes and goes; a list without the focus shows none on its current tile,
' and the focused title is bold.
sub onFocus()
    ' A RowList's rows say whether they have the focus; some Rokus also set
    ' gridHasFocus false on its tiles, which only a MarkupGrid's mean.
    gridHasFocus = m.top.gridHasFocus or inRowList()
    listFocused = m.top.rowHasFocus and m.top.rowListHasFocus and gridHasFocus
    focused = m.top.itemHasFocus and listFocused
    ' Only what changed is set: a row taking or losing the focus tells every
    ' tile in it, and most stay as they were, on the render thread.
    fresh = not m.shown
    if fresh or listFocused <> m.shownList then m.ring.visible = listFocused
    m.shownList = listFocused
    if not fresh and focused = m.shownFocused then return
    m.shown = true
    m.shownFocused = focused
    ' The list leaves focusPercent alone for a tile focused as it loaded: its
    ' look follows the focus here too, the next frames of focusPercent
    ' taking over while focus glides.
    growTo(focused)
    if focused
        m.ring.opacity = 1
        m.title.font = m.boldFont
        m.fallback.color = m.c.fg
    else
        m.ring.opacity = 0
        m.title.font = m.captionFont
        m.fallback.color = m.c.muted
    end if
end sub

' Whether this tile is in a RowList's row, rather than a MarkupGrid: found
' once among its parents (a list's tiles stay in it), once it has them.
function inRowList() as boolean
    if m.inRowList <> invalid then return m.inRowList
    node = m.top.getParent()
    if node = invalid then return false
    m.inRowList = false
    for i = 1 to 4
        if node = invalid then exit for
        kind = node.subtype()
        if kind = "RowList" or kind = "QuickRowList" or kind = "RowListItem"
            m.inRowList = true
            exit for
        end if
        node = node.getParent()
    end for
    return m.inRowList
end function

' The tile at rest or grown, as focusPercent leaves it at either end.
sub growTo(grown as boolean)
    if m.imageWidth <= 0 then return
    w = m.imageWidth
    h = m.imageHeight
    if grown
        m.grow.scale = [1.08, 1.08]
        w = w * 1.08
        h = h * 1.08
    else
        m.grow.scale = [1, 1]
    end if
    place = [-(w - m.imageWidth) / 2, -(h - m.imageHeight) / 2]
    m.well.width = w
    m.well.height = h
    m.well.translation = place
    m.poster.width = w
    m.poster.height = h
    m.poster.translation = place
    m.mask.maskSize = [w, h]
    m.mask.maskOffset = place
end sub

' A picture of another shape than its tile (a scope still in a 16:9 tile)
' leaves OS 16's Effect, which drew it stretched: its corners go square,
' the picture zoomed and cropped as it should be.
sub onLoaded()
    if m.poster.loadStatus <> "ready" or m.person or m.round = invalid or not m.round.supported then return
    bw = m.poster.bitmapWidth
    bh = m.poster.bitmapHeight
    if bw <= 0 or bh <= 0 or m.imageHeight <= 0 then return
    shape = (bw / bh) / (m.imageWidth / m.imageHeight)
    if shape > 1.03 or shape < 0.97 then m.poster.effect = invalid
end sub

' "Ada Lovelace" as "AL", "Cher" as "C".
function tileInitials(name as string) as string
    out = ""
    for each word in name.Split(" ")
        if word <> "" and Len(out) < 2 then out = out + UCase(Left(word, 1))
    end for
    return out
end function
