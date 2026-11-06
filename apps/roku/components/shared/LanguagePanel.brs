sub init()
    m.title = m.top.findNode("title")
    m.choices = m.top.findNode("choices")
    m.values = []
    m.top.visible = false
    m.choices.observeField("itemSelected", "onChoice")
end sub

' Shows the languages in the language shown, the choice in force checked
' and focused.
sub onOpen()
    if not m.top.open then return
    choice = i18nChoices(m.top.automatic, i18nSupported(m.top.current))
    m.values = choice.values
    items = []
    for each label in choice.labels
        items.Push({ title: label })
    end for
    m.title.text = t("signin.language.choose")
    m.choices.content = nodesContent(items)
    m.choices.checkedItem = choice.current
    m.choices.jumpToItem = choice.current
    m.top.visible = true
    m.choices.setFocus(true)
end sub

sub onChoice(event as object)
    at = event.getData()
    if at < 0 or at >= m.values.Count() then return
    m.top.visible = false
    value = m.values[at]
    if value = "" then value = "auto"
    m.top.picked = value
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press or not m.top.visible then return false
    if key = "back" or key = "left"
        m.top.visible = false
        m.top.closed = true
        return true
    end if
    ' The screen under it never takes the focus while it shows.
    return key = "right" or key = "up" or key = "down" or key = "options"
end function
