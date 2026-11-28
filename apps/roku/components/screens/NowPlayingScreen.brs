' Now Playing: what the music player (m.global.music) plays, and Up next,
' the queue with the track playing marked (Music.brs lists a long one a
' part at a time): OK on one plays it, and the queue goes on from there.
' Shuffle plays the other tracks after this one in a random order; Stop
' shuffling puts the order back. The remote's play pauses or goes on, fast
' forward and rewind go to the next track or back. Down goes from the
' buttons to Up next, Up from its first track back; Up from the buttons
' goes to the scrubber, where Left and Right move 10 s, as on Android TV.
' Its fields are followed only while this screen shows.

sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.cover = m.top.findNode("cover")
    m.title = m.top.findNode("title")
    m.line = m.top.findNode("line")
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
    m.lower = m.top.findNode("lower")
    m.upNextTitle.text = t("player.upNext.title")
    ' A small grey heading, as Android TV's.
    m.upNextTitle.font = uiType("label")
    m.watching = false
    m.labels = []
    ' The buttons' names, under each round button while it has the focus.
    m.words = {
        shuffle: t("player.nowPlaying.shuffle"),
        unshuffle: t("player.nowPlaying.stopShuffling"),
        previousTrack: t("player.nowPlaying.previous"),
        pause: t("player.nowPlaying.pause"),
        play: t("common.actions.play"),
        nextTrack: t("player.nowPlaying.next"),
        stopAll: t("player.nowPlaying.stop")
    }
    ' Up next: the queue it lists (its queueId) and its tracks, read once,
    ' the part of it listed ({ first, last }, invalid before any), its
    ' rows, and the track marked as playing (-1 for none).
    m.queueId = -1
    m.tracks = []
    m.part = invalid
    m.rows = CreateObject("roSGNode", "ContentNode")
    m.marked = -1
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
            for each field in ["track", "at", "state", "shuffled", "positionMs", "message"]
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
        music.observeFieldScoped("positionMs", "onPosition")
        music.observeFieldScoped("message", "showStatus")
        m.watching = true
    end if
    onTrack()
    onAt()
    onMusicState()
    showStatus()
    onPosition()
    m.buttons.setFocus(true)
end sub

sub onTrack()
    track = m.global.music.track
    if track = invalid then return
    m.title.text = track.title
    parts = []
    if track.artist <> "" then parts.Push(track.artist)
    if track.album <> "" then parts.Push(track.album)
    m.line.text = parts.Join(" · ")
    placeControls()
    if m.cover.uri <> track.image
        agent = m.top.getScene().getHttpAgent()
        m.cover.setHttpAgent(agent)
        m.cover.uri = track.image
        m.backdrop.setHttpAgent(agent)
        m.backdrop.uri = track.image
    end if
end sub

' The scrubber and the buttons 32 px under the track's lines (lower by a
' title's second line), and as many rows of Up next as end above the
' title-safe area's foot (1026).
sub placeControls()
    lines = 116 + 16 + 8 + 36
    for each label in [m.title, m.line]
        lines += label.boundingRect().height
    end for
    dy = Int(lines + 32 - 330)
    if dy < 0 then dy = 0
    if m.lower.translation[1] <> dy then m.lower.translation = [0, dy]
    rows = Int((1026 - 620 - dy + 6) / 102)
    if rows < 2 then rows = 2
    if rows > 4 then rows = 4
    if m.upNext.numRows <> rows then m.upNext.numRows = rows
end sub

' Another track, or another queue: Up next marks the track playing. Moved
' on within the part listed, only the mark moves; another queue, or a
' track outside that part or near its end, lists the part around it. The
' track playing comes into view, unless someone is looking down the list:
' what has the focus keeps it.
sub onAt()
    music = m.global.music
    at = music.at
    m.queue.text = t("player.nowPlaying.position", { at: at + 1, total: music.queueSize })
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
    nodesFill(fresh, { number: row.number, title: row.title, subtitle: row.subtitle, duration: row.duration, current: current })
    m.rows.replaceChild(fresh, index)
end sub

' Stopped (the queue played out, or Stop), the screen goes; stopped from
' another device or by the server's administrator, the scene says so over
' the screen before, as after a video.
sub onMusicState()
    music = m.global.music
    if music.state = "stopped"
        if not m.top.active then return
        if music.stoppedBy = ""
            uiNavigate({ back: true })
            return
        end if
        lines = music.stoppedBy.Split(Chr(10))
        message = []
        if lines.Count() > 1 then message.Push(lines[1])
        uiNavigate({ back: true, message: { title: lines[0], lines: message } })
        return
    end if
    showButtons()
end sub

' Shuffle (or Stop shuffling), Previous, Pause (or Play, paused), Next
' and Stop, as Android TV's Now Playing: round icons, each named under it
' while focused.
sub showButtons()
    music = m.global.music
    words = m.words
    shuffle = words.shuffle
    if music.shuffled then shuffle = words.unshuffle
    toggle = words.pause
    if music.state = "paused" then toggle = words.play
    labels = [shuffle, words.previousTrack, toggle, words.nextTrack, words.stopAll]
    if FormatJson(labels) = FormatJson(m.labels) then return
    m.labels = labels
    playIcon = "pause"
    if music.state = "paused" then playIcon = "play"
    icons = ["shuffle", "skip-back", playIcon, "skip-forward", "square"]
    chips = []
    for i = 0 to labels.Count() - 1
        chips.Push({ label: "", icon: icons[i], name: labels[i], width: 0 })
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

' The line under the album: what went wrong with a track before.
sub showStatus()
    m.message.text = m.global.music.message
end sub

sub onButton(event as object)
    music = m.global.music
    shuffle = "shuffle"
    if music.shuffled then shuffle = "unshuffle"
    actions = [shuffle, "previous", "toggle", "next", "stop"]
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
