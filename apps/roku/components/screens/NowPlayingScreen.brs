' Now Playing: what the music player (m.global.music) plays, and Up next,
' the tracks after it: OK on one plays it, and the queue goes on from
' there. The remote's play pauses or goes on, fast forward and rewind go
' to the next track or back. Down goes from the buttons to Up next, Up
' from its first track back. Its fields are followed only while this
' screen shows.

sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.cover = m.top.findNode("cover")
    m.title = m.top.findNode("title")
    m.artist = m.top.findNode("artist")
    m.album = m.top.findNode("album")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
    m.elapsed = m.top.findNode("elapsed")
    m.left = m.top.findNode("left")
    m.buttons = m.top.findNode("buttons")
    m.queue = m.top.findNode("queue")
    m.message = m.top.findNode("message")
    m.upNextTitle = m.top.findNode("upNextTitle")
    m.upNext = m.top.findNode("upNext")
    m.watching = false
    m.labels = []
    ' Up next: the queue it lists (its queueId), its rows, and the place
    ' in the queue of the first (-1 before any).
    m.queueId = -1
    m.rows = CreateObject("roSGNode", "ContentNode")
    m.from = -1
    showButtons("Pause")
    m.buttons.focusIndex = 1
    m.buttons.observeField("chosen", "onButton")
    m.upNext.observeField("itemSelected", "onUpNext")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    music = m.global.music
    if not m.top.active
        if m.watching
            for each field in ["track", "at", "state", "positionMs", "message"]
                music.unobserveFieldScoped(field)
            end for
            m.watching = false
        end if
        return
    end if
    if not m.watching
        music.observeFieldScoped("track", "onTrack")
        music.observeFieldScoped("at", "onAt")
        music.observeFieldScoped("state", "onMusicState")
        music.observeFieldScoped("positionMs", "onPosition")
        music.observeFieldScoped("message", "onMessage")
        m.watching = true
    end if
    onTrack()
    onAt()
    onMusicState()
    onPosition()
    onMessage()
    m.buttons.setFocus(true)
end sub

sub onTrack()
    track = m.global.music.track
    if track = invalid then return
    m.title.text = track.title
    m.artist.text = track.artist
    m.album.text = track.album
    if m.cover.uri <> track.image
        agent = m.top.getScene().getHttpAgent()
        m.cover.setHttpAgent(agent)
        m.cover.uri = track.image
        m.backdrop.setHttpAgent(agent)
        m.backdrop.uri = track.image
    end if
end sub

' Another track, or another queue: Up next lists what comes after the
' track playing. Moved on in the same queue, it loses its first rows;
' another queue, or a track before, lists it again.
sub onAt()
    music = m.global.music
    at = music.at
    m.queue.text = "Track " + urlValue(at + 1) + " of " + urlValue(music.queueSize)
    from = at + 1
    if music.queueId <> m.queueId or m.from < 0 or from < m.from
        m.queueId = music.queueId
        m.rows = nodesContent(musicUpNextRows(music.queue, at))
        m.upNext.content = m.rows
    else if from > m.from
        gone = from - m.from
        if gone > m.rows.getChildCount() then gone = m.rows.getChildCount()
        ' The track in focus keeps it, unless it went.
        focused = m.upNext.itemFocused - gone
        m.rows.removeChildrenIndex(gone, 0)
        if focused < 0 then focused = 0
        if m.upNext.hasFocus() and m.rows.getChildCount() > 0 then m.upNext.jumpToItem = focused
    end if
    m.from = from
    shown = m.rows.getChildCount() > 0
    m.upNextTitle.visible = shown
    m.upNext.visible = shown
    if not shown and m.upNext.hasFocus() then m.buttons.setFocus(true)
end sub

' Stopped (the queue played out, or Stop), the screen goes; paused, the
' button says Play.
sub onMusicState()
    state = m.global.music.state
    if state = "stopped"
        if m.top.active then uiNavigate({ back: true })
        return
    end if
    label = "Pause"
    if state = "paused" then label = "Play"
    showButtons(label)
end sub

' Previous, Pause (or Play), Next and Stop side by side.
sub showButtons(toggle as string)
    labels = ["Previous", toggle, "Next", "Stop"]
    if FormatJson(labels) = FormatJson(m.labels) then return
    m.labels = labels
    chips = []
    for each label in labels
        chips.Push({ label: label, width: 210 })
    end for
    m.buttons.chips = chips
end sub

sub onPosition()
    music = m.global.music
    position = music.positionMs
    duration = music.durationMs
    m.fill.width = Int(m.track.width * fmtProgress(position, duration))
    m.elapsed.text = fmtClock(position)
    m.left.text = ""
    if duration > 0 then m.left.text = "-" + fmtClock(duration - position)
end sub

sub onMessage()
    m.message.text = m.global.music.message
end sub

sub onButton(event as object)
    actions = ["previous", "toggle", "next", "stop"]
    m.global.music.control = actions[event.getData()]
end sub

sub onUpNext(event as object)
    m.global.music.jump = m.from + event.getData()
end sub

function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    if key = "play"
        m.global.music.control = "toggle"
        return true
    end if
    if key = "fastforward"
        m.global.music.control = "next"
        return true
    end if
    if key = "rewind" or key = "replay"
        m.global.music.control = "previous"
        return true
    end if
    if key = "down" and m.buttons.hasFocus() and m.upNext.visible
        m.upNext.setFocus(true)
        return true
    end if
    if key = "up" and m.upNext.hasFocus()
        m.buttons.setFocus(true)
        return true
    end if
    return false
end function
