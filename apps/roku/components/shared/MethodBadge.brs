' Draws how something plays as a badge (MethodBadge.xml). Sizes are the
' Android TV badge's dp doubled.

sub init()
    ' Only in the players: in their colors, a light theme's twin's.
    m.c = uiPlayerColors()
    m.back = m.top.findNode("methodBack")
    m.tint = m.top.findNode("methodTint")
    m.words = m.top.findNode("methodText")
    m.words.font = uiType("caption")
    onLook()
end sub

sub onLook()
    ' Fields set in markup may come before init.
    if m.words = invalid then return
    ' Success for a file played as it is, info for a stream copied, the
    ' accent for a conversion.
    color = m.c.accent
    if m.top.kind = "direct" then color = m.c.success
    if m.top.kind = "remux" then color = m.c.info
    m.words.text = m.top.text
    m.words.color = color
    m.tint.blendColor = themeAlpha(color, "33")
    ' The label keeps no width of its own, as wide as its words: held to
    ' the width it measured, Roku cut the last letters for an ellipsis.
    m.words.width = 0
    width = Int(m.words.boundingRect().width + 0.999) + 32
    m.back.width = width
    m.tint.width = width
    m.top.width = width
end sub
