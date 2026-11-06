sub init()
    m.fill = m.top.findNode("fill")
    m.globe = m.top.findNode("globe")
    m.letters = m.top.findNode("letters")
    m.top.observeField("focusedChild", "onFocus")
    onLanguage()
end sub

' The letters, then the globe at their left and the fill around both, all
' left of the origin.
sub onLanguage()
    m.letters.text = UCase(m.top.language)
    width = Int(m.letters.boundingRect().width)
    pad = 14
    m.letters.translation = [-pad - width, 0]
    m.globe.translation = [-pad - width - 8 - 28, 10]
    m.fill.width = pad + 28 + 8 + width + pad
    m.fill.translation = [-m.fill.width, 0]
end sub

' Focused, it fills with the accent and its globe and letters turn dark,
' as a focused button does.
sub onFocus()
    focused = m.top.hasFocus()
    m.fill.visible = focused
    color = "0xEBEBEBFF"
    if focused then color = "0x16191EFF"
    m.globe.blendColor = color
    m.letters.color = color
end sub
