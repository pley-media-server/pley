sub init()
    m.background = m.top.findNode("background")
    m.bar = m.top.findNode("bar")
    m.poster = m.top.findNode("poster")
    m.unseen = m.top.findNode("unseen")
    m.title = m.top.findNode("title")
    m.subtitle = m.top.findNode("subtitle")
    m.who = m.top.findNode("who")
    m.stage = m.top.findNode("stage")
    m.line = m.top.findNode("line")
end sub

sub onContent()
    request = m.top.itemContent
    if request = invalid then return
    m.title.text = request.title
    m.subtitle.text = request.subtitle
    m.who.text = request.who
    m.line.text = request.line
    ' The mark of news pushes the title along.
    news = request.unseen = true
    m.unseen.visible = news
    if news
        m.title.translation = [184, 14]
        m.title.width = 896
    else
        m.title.translation = [160, 14]
        m.title.width = 920
    end if
    m.stage.color = request.stageColor
    m.stage.progress = request.progress
    m.stage.text = request.stageText
    ' The scene's agent carries the token; it changes with each refresh.
    m.poster.setHttpAgent(m.top.getScene().getHttpAgent())
    m.poster.uri = request.image
end sub

sub onFocus()
    focused = m.top.itemHasFocus and m.top.listHasFocus
    m.bar.visible = focused
    if focused
        m.background.color = "0x2E3440FF"
        m.title.color = "0x7DD3FCFF"
    else
        m.background.color = "0x1E222AFF"
        m.title.color = "0xEBEBEBFF"
    end if
end sub
