sub init()
    m.frame = m.top.findNode("frame")
    m.fallback = m.top.findNode("fallback")
    m.poster = m.top.findNode("poster")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
    m.watched = m.top.findNode("watched")
    m.title = m.top.findNode("title")
    m.subtitle = m.top.findNode("subtitle")
    m.badgeBack = m.top.findNode("badgeBack")
    m.badge = m.top.findNode("badge")
    m.qualityBacks = [m.top.findNode("qualityBack0"), m.top.findNode("qualityBack1")]
    m.qualities = [m.top.findNode("quality0"), m.top.findNode("quality1")]
    m.wide = false
    m.square = false
    m.progress = 0.0
end sub

' The title under the poster shows until a picture covers it, or when
' there is none: no observer per tile.
sub onContent()
    item = m.top.itemContent
    if item = invalid then return
    m.wide = false
    if item.hasField("wide") then m.wide = item.wide
    m.square = false
    if item.hasField("square") then m.square = item.square
    m.progress = 0.0
    if item.hasField("progress") then m.progress = item.progress
    m.watched.visible = item.hasField("watched") and item.watched = true
    m.title.text = item.title
    ' See all, a row's Try again or note, a grid's Look further and Request
    ' more seasons: their words in the frame only.
    if item.hasField("kind") and (item.kind = "more" or item.kind = "retry" or item.kind = "note" or item.kind = "further" or item.kind = "askMore") then m.title.text = ""
    m.subtitle.text = item.subtitle
    m.subtitle.color = "0xA1A9B6FF"
    if item.hasField("subtitleColor") then m.subtitle.color = item.subtitleColor
    m.fallback.text = item.title
    badge = ""
    if item.hasField("badge") then badge = item.badge
    m.badge.text = badge
    m.badge.visible = badge <> ""
    m.badgeBack.visible = badge <> ""
    if badge <> "" and item.hasField("badgeColor") then m.badgeBack.color = item.badgeColor
    qualities = []
    if item.hasField("qualityBadges") and item.qualityBadges <> invalid then qualities = item.qualityBadges
    for i = 0 to m.qualities.Count() - 1
        text = ""
        if i < qualities.Count() then text = qualities[i]
        m.qualities[i].text = text
        m.qualities[i].visible = text <> ""
        m.qualityBacks[i].visible = text <> ""
    end for
    ' The scene's agent carries the token; it changes with each refresh.
    m.poster.setHttpAgent(m.top.getScene().getHttpAgent())
    onLayout()
    m.poster.uri = item.image
    onFocus()
end sub

sub onLayout()
    w = m.top.width
    if w <= 0 then return
    imageHeight = Int(w * 1.5)
    if m.wide then imageHeight = Int(w * 9 / 16)
    if m.square then imageHeight = Int(w)
    m.frame.width = w
    m.frame.height = imageHeight
    m.poster.translation = [4, 4]
    m.poster.width = w - 8
    m.poster.height = imageHeight - 8
    ' Decoded at the size it's drawn: small textures for small devices.
    m.poster.loadWidth = w - 8
    m.poster.loadHeight = imageHeight - 8
    m.fallback.translation = [16, 16]
    m.fallback.width = w - 32
    m.fallback.height = imageHeight - 32
    barWidth = w - 8
    m.track.translation = [4, imageHeight - 14]
    m.track.width = barWidth
    m.track.height = 10
    m.fill.translation = [4, imageHeight - 14]
    m.fill.width = Int(barWidth * m.progress)
    m.fill.height = 10
    m.track.visible = m.progress > 0
    m.fill.visible = m.progress > 0
    m.watched.translation = [w - 52, 12]
    ' The badge at the poster's top left, as wide as its words; cut short
    ' only when they don't fit (a label as wide as it measures can lose
    ' its last letters to rounding).
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
    ' The quality badges side by side at the poster's foot, above how far
    ' along it is; one that doesn't fit whole isn't drawn.
    x = 12
    y = imageHeight - 46
    if m.progress > 0 then y = imageHeight - 56
    for i = 0 to m.qualities.Count() - 1
        label = m.qualities[i]
        back = m.qualityBacks[i]
        if label.text <> ""
            label.width = 0
            chipWidth = label.boundingRect().width + 24
            fits = x + chipWidth <= w - 12
            label.visible = fits
            back.visible = fits
            label.translation = [x + 12, y]
            back.translation = [x, y]
            back.width = chipWidth
            x = x + chipWidth + 8
        end if
    end for
    m.title.translation = [0, imageHeight + 12]
    m.title.width = w
    m.subtitle.translation = [0, imageHeight + 44]
    m.subtitle.width = w
end sub

sub onFocus()
    ' A RowList's rows say whether they have the focus; some Rokus also set
    ' gridHasFocus false on its tiles, which only a MarkupGrid's mean.
    gridHasFocus = m.top.gridHasFocus or inRowList()
    focused = m.top.itemHasFocus and m.top.rowHasFocus and m.top.rowListHasFocus and gridHasFocus
    ' Words in the frame (See all, a title without a picture) stay legible
    ' on the accent.
    if focused
        m.frame.color = "0x38BDF8FF"
        m.title.color = "0x7DD3FCFF"
        m.fallback.color = "0x16191EFF"
    else
        m.frame.color = "0x262B35FF"
        m.title.color = "0xEBEBEBFF"
        m.fallback.color = "0xA1A9B6FF"
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
        if kind = "RowList" or kind = "RowListItem"
            m.inRowList = true
            exit for
        end if
        node = node.getParent()
    end for
    return m.inRowList
end function
