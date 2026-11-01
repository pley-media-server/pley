' Now Playing: what the music player (m.global.music) plays, and Up next,
' the tracks after it: OK on one plays it, and the queue goes on from
' there. Shuffle plays the other tracks after this one in a random order;
' Stop shuffling puts the order back. The sleep timer's button steps
' through its choices, and in the timer's last 30 s, while a line counts
' down, it is Keep listening. The remote's play pauses or goes on, fast
' forward and rewind go to the next track or back. Down goes from the
' buttons to Up next, Up from its first track back. Its fields are
' followed only while this screen shows.

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
    ' Whether the sleep timer's last 30 s were counting down.
    m.noticing = false
    showButtons()
    m.buttons.focusIndex = 2
    m.buttons.observeField("chosen", "onButton")
    m.upNext.observeField("itemSelected", "onUpNext")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    music = m.global.music
    if not m.top.active
        if m.watching
            for each field in ["track", "at", "state", "shuffled", "sleep", "positionMs", "message"]
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
        music.observeFieldScoped("shuffled", "showButtons")
        music.observeFieldScoped("sleep", "onSleep")
        music.observeFieldScoped("positionMs", "onPosition")
        music.observeFieldScoped("message", "showStatus")
        m.watching = true
    end if
    onTrack()
    onAt()
    onMusicState()
    onSleep()
    onPosition()
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

' Stopped (the queue played out, or Stop), the screen goes.
sub onMusicState()
    if m.global.music.state = "stopped"
        if m.top.active then uiNavigate({ back: true })
        return
    end if
    showButtons()
end sub

' Shuffle (or Stop shuffling), Previous, Pause (or Play, paused), Next and
' Stop side by side, each wide enough for either of its words, then the
' sleep timer's button, as wide as what it says.
sub showButtons()
    music = m.global.music
    shuffle = "Shuffle"
    if music.shuffled then shuffle = "Stop shuffling"
    toggle = "Pause"
    if music.state = "paused" then toggle = "Play"
    sleep = "Sleep timer"
    if music.sleep <> invalid then sleep = music.sleep.button
    labels = [shuffle, "Previous", toggle, "Next", "Stop", sleep]
    if FormatJson(labels) = FormatJson(m.labels) then return
    m.labels = labels
    widths = [300, 220, 180, 160, 160, 0]
    chips = []
    for i = 0 to labels.Count() - 1
        chips.Push({ label: labels[i], width: widths[i] })
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

' The sleep timer: its button says what is left. Keep listening takes the
' focus as the last 30 s begin, while the buttons have it.
sub onSleep()
    sleep = m.global.music.sleep
    noticing = sleep <> invalid and sleep.secondsLeft >= 0
    showButtons()
    if noticing and not m.noticing and m.buttons.hasFocus() then m.buttons.focusIndex = 5
    m.noticing = noticing
    showStatus()
end sub

' The line under the album: "Pausing in 12 s" in the sleep timer's last
' 30 s, else what went wrong with a track before.
sub showStatus()
    music = m.global.music
    if m.noticing
        m.message.text = "Pausing in " + urlValue(music.sleep.secondsLeft) + " s"
        m.message.color = "0xA1A9B6FF"
    else
        m.message.text = music.message
        m.message.color = "0xEB6B70FF"
    end if
end sub

sub onButton(event as object)
    music = m.global.music
    shuffle = "shuffle"
    if music.shuffled then shuffle = "unshuffle"
    actions = [shuffle, "previous", "toggle", "next", "stop", "sleep"]
    music.control = actions[event.getData()]
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
