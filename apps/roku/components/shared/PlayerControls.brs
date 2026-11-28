' Draws the player's controls (PlayerControls.xml) as the Android TV
' player's: what the player sets, nothing more. Sizes are its dp doubled.

sub init()
    m.c = uiPlayerColors()
    m.controls = m.top.findNode("controls")
    m.title = m.top.findNode("title")
    m.line = m.top.findNode("line")
    m.clock = m.top.findNode("clock")
    m.method = m.top.findNode("method")
    m.elapsed = m.top.findNode("elapsed")
    m.remaining = m.top.findNode("remaining")
    m.played = m.top.findNode("played")
    m.gaps = m.top.findNode("gaps")
    m.thumb = m.top.findNode("thumb")
    m.thumbFocus = m.top.findNode("thumbFocus")
    m.marker = m.top.findNode("marker")
    m.preview = m.top.findNode("preview")
    m.previewImage = m.top.findNode("previewImage")
    m.previewWell = m.top.findNode("previewWell")
    m.previewTime = m.top.findNode("previewTime")
    m.previewChapter = m.top.findNode("previewChapter")
    m.row = m.top.findNode("row")
    m.badge = m.top.findNode("badge")
    m.badgeText = m.top.findNode("badgeText")
    m.buffering = m.top.findNode("buffering")
    m.ring = m.top.findNode("ring")
    m.spin = m.top.findNode("spin")
    m.bufferingNote = m.top.findNode("bufferingNote")
    m.bufferingBack = m.top.findNode("bufferingBack")
    m.bufferingLabel = m.top.findNode("bufferingLabel")
    m.fadeIn = m.top.findNode("fadeIn")
    m.fadeOut = m.top.findNode("fadeOut")
    m.fadeOut.observeField("state", "onFadeOut")
    m.clockTimer = m.top.findNode("clockTimer")
    m.clockTimer.observeField("fire", "onClock")
    m.title.font = uiType("title")
    m.line.font = uiType("label")
    m.clock.font = uiType("label")
    m.elapsed.font = uiType("caption")
    m.remaining.font = uiType("caption")
    m.marker.font = uiType("caption")
    m.previewTime.font = uiType("label")
    m.previewChapter.font = uiType("caption")
    m.badgeText.font = uiType("label")
    m.bufferingLabel.font = uiType("label")
    ' Roboto's digits are all as wide: times keep their width as they
    ' change. (A label's monospacedDigits finds no glyphs in it.)
    ' The row's buttons by id: { group, fill, icon, label, size, icon size }.
    m.buttonNodes = {}
    m.focusedButton = ""
end sub

' --- The top ---

sub onTop()
    m.title.text = m.top.title
    m.line.text = m.top.line
    ' The badge (MethodBadge), in its kind's color.
    method = m.top.method
    m.method.visible = method <> ""
    if method <> ""
        m.method.kind = m.top.methodKind
        m.method.text = method
    end if
    onClock()
end sub

' The time of day at the right, the badge before it.
sub onClock()
    zone = fmtZoneSeconds()
    now = CreateObject("roDateTime").AsSeconds()
    m.clock.text = fmtTimeOfDay(now, zone)
    m.clock.width = 0
    clockWidth = Int(m.clock.boundingRect().width)
    m.clock.width = 400
    if m.method.visible then m.method.translation = [1824 - clockWidth - 24 - m.method.width, 54]
end sub

' --- The scrubber ---

' Where it stands: a seek coming's target, else the playhead.
function scrubberAt() as integer
    if m.top.targetMs >= 0 then return m.top.targetMs
    return m.top.positionMs
end function

function xOf(ms as integer) as float
    duration = m.top.durationMs
    if duration <= 0 then return 0
    fraction = ms / duration
    if fraction < 0 then fraction = 0
    if fraction > 1 then fraction = 1
    return fraction * 1728
end function

sub onTrack()
    at = scrubberAt()
    duration = m.top.durationMs
    m.elapsed.text = fmtClock(at)
    left = duration - at
    if left < 0 then left = 0
    m.remaining.text = t("player.controls.remaining", { time: fmtClock(left) })
    x = xOf(at)
    m.played.width = x
    m.thumb.translation = [96 + x - 10, 840]
    m.thumbFocus.translation = [96 + x - 18, 832]
    showMarker(at)
    showPreview(at, x)
end sub

' "Intro" or "Credits" under the span of the marker playing.
sub showMarker(at as integer)
    found = invalid
    for each marker in httpArray(m.top.markers)
        if at >= marker.startMs and at < marker.endMs then found = marker
    end for
    m.marker.visible = found <> invalid
    if found = invalid then return
    m.marker.text = found.label
    m.marker.width = 0
    width = Int(m.marker.boundingRect().width)
    middle = xOf(Int((found.startMs + found.endMs) / 2))
    x = middle - width / 2
    if x < 0 then x = 0
    if x > 1728 - width then x = 1728 - width
    m.marker.translation = [96 + x, 866]
end sub

' The chapters' starts as 4 px gaps in the track, the background's color
' over it.
sub onChapters()
    m.gaps.removeChildrenIndex(m.gaps.getChildCount(), 0)
    for each start in httpArray(m.top.chapters)
        x = xOf(start)
        if x > 4 and x < 1724
            gap = m.gaps.createChild("Rectangle")
            gap.translation = [96 + x - 2, 846]
            gap.width = 4
            gap.height = 8
            gap.color = themeAlpha(m.c.bg, "E6")
        end if
    end for
end sub

' --- The seek preview ---

' Over the thumb while a seek is coming, kept in the safe area: the
' server's preview when it has them, the time under it and the chapter.
sub showPreview(at as integer, x as float)
    shown = m.top.targetMs >= 0 and m.controls.visible
    m.preview.visible = shown
    if not shown then return
    pictured = m.top.previewsReady
    m.previewWell.visible = pictured
    m.previewImage.visible = pictured
    m.top.findNode("previewFrame").visible = pictured
    left = 96 + x - 240
    if left < 96 then left = 96
    if left > 1824 - 480 then left = 1824 - 480
    top = 424
    ' Without pictures, the time and chapter stand where the picture's
    ' bottom would.
    if not pictured then top = 424 + 270 - 70
    m.preview.translation = [left, top]
    m.previewTime.translation = [0, 282]
    m.previewChapter.translation = [0, 320]
    if not pictured
        m.previewTime.translation = [0, 0]
        m.previewChapter.translation = [0, 38]
    end if
    m.previewTime.text = fmtClock(at)
end sub

sub onPreview()
    ' None yet: the well, not the last one's picture.
    if m.previewImage.uri <> m.top.previewUri then m.previewImage.uri = m.top.previewUri
    ' The well until a picture shows, then dark where the picture doesn't
    ' reach, as the video is letterboxed.
    if m.top.previewUri <> ""
        m.previewWell.color = m.c.bg
    else
        m.previewWell.color = m.c.surface2
    end if
    m.previewChapter.text = m.top.previewChapter
end sub

' --- The row ---

' The buttons, made again when the list changes: 72 px with 40 px icons,
' the transport's icons 48, Play/Pause 88 with 52; the left group from the
' safe edge, the middle one centered, the right one to the right edge.
sub onButtons()
    m.row.removeChildrenIndex(m.row.getChildCount(), 0)
    m.buttonNodes = {}
    groups = { left: [], center: [], right: [] }
    for each button in httpArray(m.top.buttons)
        list = groups[button.group]
        if list <> invalid then list.Push(button)
    end for
    placeRow(groups.left, 96, 20)
    centerWidth = rowWidth(groups.center, 32)
    placeRow(groups.center, Int(960 - centerWidth / 2), 32)
    placeRow(groups.right, 1824 - rowWidth(groups.right, 20), 20)
    onPlaying()
    onFocusKey()
end sub

function buttonSize(button as object) as integer
    if button.id = "playPause" then return 88
    return 72
end function

function rowWidth(buttons as object, gap as integer) as integer
    width = 0
    for each button in buttons
        if width > 0 then width = width + gap
        width = width + buttonSize(button)
    end for
    return width
end function

sub placeRow(buttons as object, x as integer, gap as integer)
    for each button in buttons
        size = buttonSize(button)
        iconSize = 40
        if button.id = "playPause" then iconSize = 52
        if button.id = "previous" or button.id = "next" then iconSize = 48
        ' The 72 px buttons sit lower, their centers on Play/Pause's.
        y = 882
        if size < 88 then y = 882 + 8
        group = m.row.createChild("Group")
        group.translation = [x, y]
        fill = group.createChild("Poster")
        fill.uri = "pkg:/images/ui/r8.9.png"
        fill.width = size
        fill.height = size
        fill.blendColor = m.c.accent
        fill.visible = false
        icon = group.createChild("Poster")
        icon.width = iconSize
        icon.height = iconSize
        icon.translation = [(size - iconSize) / 2, (size - iconSize) / 2]
        icon.blendColor = m.c.fg
        icon.uri = "pkg:/images/ui/icon-" + button.icon + "-" + iconSize.ToStr() + ".png"
        label = group.createChild("Label")
        label.font = uiType("caption")
        label.color = m.c.fg
        label.text = button.label
        ' Its name 16 px under it, centered, while focused.
        textWidth = Int(label.boundingRect().width)
        label.translation = [(size - textWidth) / 2, size + 16]
        label.visible = false
        ' Previous, Play/Pause and Next say what they are by their icons
        ' alone; the others' icons name them less plainly.
        named = button.id <> "previous" and button.id <> "playPause" and button.id <> "next"
        m.buttonNodes[button.id] = { group: group, fill: fill, icon: icon, label: label, iconSize: iconSize, base: button.icon, named: named }
        x = x + size + gap
    end for
end sub

' Play/Pause shows what it would do.
sub onPlaying()
    node = m.buttonNodes["playPause"]
    if node = invalid then return
    name = "play"
    if m.top.playing then name = "pause"
    node.icon.uri = "pkg:/images/ui/icon-" + name + "-52.png"
    if m.top.playing
        node.label.text = t("player.controls.pause")
    else
        node.label.text = t("player.controls.play")
    end if
    textWidth = Int(node.label.boundingRect().width)
    node.label.translation = [(88 - textWidth) / 2, 88 + 16]
end sub

' The focused button fills with the accent, its icon in on-accent, its
' name under it (but Previous', Play/Pause's and Next's); it keeps its size,
' as a page's buttons do. The scrubber's thumb grows a ring.
sub onFocusKey()
    key = m.top.focusKey
    m.thumbFocus.visible = key = "scrubber"
    m.thumb.visible = key <> "scrubber"
    if m.focusedButton <> "" and m.buttonNodes.DoesExist(m.focusedButton) then paintButton(m.buttonNodes[m.focusedButton], false)
    m.focusedButton = ""
    if m.buttonNodes.DoesExist(key)
        paintButton(m.buttonNodes[key], true)
        m.focusedButton = key
    end if
end sub

sub paintButton(node as object, focused as boolean)
    node.fill.visible = focused
    node.label.visible = focused and node.named
    if focused
        node.icon.blendColor = m.c.accentFg
    else
        node.icon.blendColor = m.c.fg
    end if
end sub

' --- Showing and hiding ---

sub onShown()
    if m.top.shown
        m.fadeOut.control = "stop"
        if not m.controls.visible then m.controls.opacity = 0
        m.controls.visible = true
        onClock()
        onTrack()
        m.clockTimer.control = "start"
        if m.controls.opacity < 1 then m.fadeIn.control = "start"
        m.badge.visible = false
    else
        m.clockTimer.control = "stop"
        m.fadeIn.control = "stop"
        if m.controls.visible then m.fadeOut.control = "start"
    end if
end sub

sub onFadeOut()
    if m.fadeOut.state = "stopped" and not m.top.shown then m.controls.visible = false
end sub

' The seek badge with the controls hidden, in the left third for a step
' back, the right one for a step on.
sub onBadge()
    text = m.top.badge
    m.badge.visible = text <> "" and not m.top.shown
    if text = "" then return
    m.badgeText.text = text
    x = 320 - 64
    if m.top.badgeForward then x = 1600 - 64
    m.badge.translation = [x, 476]
end sub

sub onBuffering()
    shown = m.top.buffering
    m.buffering.visible = shown
    if shown
        if m.spin.state <> "running" then m.spin.control = "start"
    else
        m.spin.control = "stop"
    end if
    text = m.top.bufferingText
    m.bufferingNote.visible = text <> ""
    if text = "" then return
    m.bufferingLabel.text = text
    m.bufferingLabel.width = 0
    width = Int(m.bufferingLabel.boundingRect().width) + 48
    m.bufferingLabel.width = width
    m.bufferingBack.width = width
    m.bufferingNote.translation = [960 - width / 2, 612]
end sub
