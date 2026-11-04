' Now Playing: what the music player (m.global.music) plays, and Up next,
' the queue with the track playing marked (Music.brs lists a long one a
' part at a time): OK on one plays it, and the queue goes on from there.
' Shuffle plays the other tracks after this one in a random order; Stop
' shuffling puts the order back. The sleep timer's button steps through
' its choices, and in the timer's last 30 s, while a line counts down, it
' is Keep listening. The remote's play pauses or goes on, fast forward and
' rewind go to the next track or back. Down goes from the buttons to Up
' next, Up from its first track back; Up from the buttons goes to the
' scrubber, where Left and Right move 10 s, as on Android TV. Its fields
' are followed only while this screen shows.

sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.cover = m.top.findNode("cover")
    m.title = m.top.findNode("title")
    m.artist = m.top.findNode("artist")
    m.album = m.top.findNode("album")
    m.track = m.top.findNode("track")
    m.fill = m.top.findNode("fill")
    m.scrubber = m.top.findNode("scrubber")
    m.scrubFocus = m.top.findNode("scrubFocus")
    m.elapsed = m.top.findNode("elapsed")
    m.left = m.top.findNode("left")
    m.buttons = m.top.findNode("buttons")
    m.queue = m.top.findNode("queue")
    m.message = m.top.findNode("message")
    m.upNextTitle = m.top.findNode("upNextTitle")
    m.upNext = m.top.findNode("upNext")
    m.watching = false
    m.labels = []
    ' Up next: the queue it lists (its queueId) and its tracks, read once,
    ' the part of it listed ({ first, last }, invalid before any), its
    ' rows, and the track marked as playing (-1 for none).
    m.queueId = -1
    m.tracks = []
    m.part = invalid
    m.rows = CreateObject("roSGNode", "ContentNode")
    m.marked = -1
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

' Another track, or another queue: Up next marks the track playing. Moved
' on within the part listed, only the mark moves; another queue, or a
' track outside that part or near its end, lists the part around it. The
' track playing comes into view, unless someone is looking down the list:
' what has the focus keeps it.
sub onAt()
    music = m.global.music
    at = music.at
    m.queue.text = "Track " + urlValue(at + 1) + " of " + urlValue(music.queueSize)
    if music.queueId <> m.queueId
        m.queueId = music.queueId
        m.tracks = music.queue
        m.part = invalid
    end if
    looking = m.upNext.hasFocus()
    part = musicUpNextPart(at, m.tracks.Count(), m.part)
    if m.part = invalid or part.first <> m.part.first or part.last <> m.part.last
        focused = -1
        if looking and m.part <> invalid then focused = m.part.first + m.upNext.itemFocused
        m.part = part
        m.rows = nodesContent(musicUpNextRows(m.tracks, part.first, part.last, at))
        m.upNext.content = m.rows
        m.marked = at
        if focused < part.first or focused >= part.last then focused = at
        m.upNext.jumpToItem = focused - part.first
    else if at <> m.marked
        markRow(m.marked, false)
        markRow(at, true)
        m.marked = at
        if not looking then m.upNext.jumpToItem = at - part.first
    end if
    shown = m.tracks.Count() > 1
    m.upNextTitle.visible = shown
    m.upNext.visible = shown
    if not shown and m.upNext.hasFocus() then m.buttons.setFocus(true)
end sub

' A row as playing or not: a new node, which the list draws again.
sub markRow(at as integer, current as boolean)
    index = at - m.part.first
    row = m.rows.getChild(index)
    if row = invalid then return
    fresh = CreateObject("roSGNode", "ContentNode")
    nodesFill(fresh, { title: row.title, subtitle: row.subtitle, duration: row.duration, current: current })
    m.rows.replaceChild(fresh, index)
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
    if m.part <> invalid then m.global.music.jump = m.part.first + event.getData()
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
    if m.scrubber.hasFocus()
        if key = "left" or key = "right"
            music = m.global.music
            moveMs = 10000
            if key = "left" then moveMs = -moveMs
            music.seek = music.positionMs + moveMs
            return true
        end if
        if key = "down" or key = "back"
            m.scrubFocus.visible = false
            m.buttons.setFocus(true)
            return true
        end if
        return key = "up"
    end if
    if key = "up" and m.buttons.isInFocusChain() and m.global.music.durationMs > 0
        m.scrubber.setFocus(true)
        m.scrubFocus.visible = true
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
