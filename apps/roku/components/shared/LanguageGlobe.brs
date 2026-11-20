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
    ' The language's letters ("FR"), or for Chinese its script, as the
    ' other apps show it.
    letters = UCase(m.top.language)
    if m.top.language = "zh-CN" then letters = "简"
    if m.top.language = "zh-TW" then letters = "繁"
    m.letters.text = letters
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
