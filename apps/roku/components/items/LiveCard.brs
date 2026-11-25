sub init()
    m.frame = m.top.findNode("frame")
    m.plate = m.top.findNode("plate")
    m.fallback = m.top.findNode("fallback")
    m.logo = m.top.findNode("logo")
    m.numberBack = m.top.findNode("numberBack")
    m.number = m.top.findNode("number")
    m.star = m.top.findNode("star")
    m.title = m.top.findNode("title")
    m.line = m.top.findNode("line")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
    m.nextLine = m.top.findNode("nextLine")
    m.progress = 0.0
    ' Logos are often see-through: the name only where none shows.
    m.logo.observeField("loadStatus", "onLogo")
    ' Pley's type where the markup names Roku's (a list's items are made
    ' after the scene restyled the screen).
    uiRestyle(m.top)
end sub

sub onLogo()
    m.fallback.visible = m.logo.uri = "" or m.logo.loadStatus = "failed"
end sub

sub onContent()
    item = m.top.itemContent
    if item = invalid then return
    m.progress = 0.0
    if item.hasField("progress") then m.progress = item.progress
    m.title.text = item.title
    m.fallback.text = item.title
    m.line.text = item.line
    m.nextLine.text = item.nextLine
    m.number.text = item.number
    m.star.visible = item.favorite = true
    m.logo.setHttpAgent(m.top.getScene().getHttpAgent())
    onLayout()
    m.logo.uri = item.image
    onLogo()
    onFocus()
end sub

sub onLayout()
    w = m.top.width
    if w <= 0 then return
    plateHeight = Int(w / 2)
    m.frame.width = w
    m.frame.height = plateHeight
    m.plate.translation = [4, 4]
    m.plate.width = w - 8
    m.plate.height = plateHeight - 8
    ' The logo within 80 % of the plate, as the web app draws it.
    logoWidth = Int((w - 8) * 0.8)
    logoHeight = Int((plateHeight - 8) * 0.78)
    m.logo.translation = [Int((w - logoWidth) / 2), Int((plateHeight - logoHeight) / 2)]
    m.logo.width = logoWidth
    m.logo.height = logoHeight
    m.logo.loadWidth = logoWidth
    m.logo.loadHeight = logoHeight
    ' Under the number badge, which would cover it.
    m.fallback.translation = [20, 52]
    m.fallback.width = w - 40
    m.fallback.height = plateHeight - 60
    numberWidth = 24 + 14 * Len(m.number.text)
    m.numberBack.translation = [12, 12]
    m.numberBack.width = numberWidth
    m.numberBack.visible = m.number.text <> ""
    m.number.translation = [12, 12]
    m.number.width = numberWidth
    m.star.translation = [w - 52, 12]
    m.title.translation = [0, plateHeight + 12]
    m.title.width = w
    m.line.translation = [0, plateHeight + 46]
    m.line.width = w
    barWidth = w
    m.track.translation = [0, plateHeight + 86]
    m.track.width = barWidth
    m.fill.translation = [0, plateHeight + 86]
    m.fill.width = Int(barWidth * m.progress)
    m.track.visible = m.progress > 0
    m.fill.visible = m.progress > 0
    m.nextLine.translation = [0, plateHeight + 102]
    m.nextLine.width = w
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.rowHasFocus and m.top.rowListHasFocus and m.top.gridHasFocus
    if focused
        m.frame.color = "0x38BDF8FF"
        m.title.color = "0x7DD3FCFF"
    else
        m.frame.color = "0x262B35FF"
        m.title.color = "0xEBEBEBFF"
    end if
end sub
