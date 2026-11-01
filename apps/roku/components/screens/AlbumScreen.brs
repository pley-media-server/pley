' An album (GET /api/items/{id}, its tracks /children): Play, Shuffle and
' OK on a track hand its tracks to the music player and open Now Playing.
' params: { id, title (shown until the server answers), trackId (the
' track to focus, from a search) }.

sub init()
    m.backdrop = m.top.findNode("backdrop")
    m.cover = m.top.findNode("cover")
    m.title = m.top.findNode("title")
    m.line = m.top.findNode("line")
    m.buttons = m.top.findNode("buttons")
    m.tracks = m.top.findNode("tracks")
    m.message = m.top.findNode("message")
    m.started = false
    m.album = invalid
    m.queue = []
    m.lastFocus = m.buttons
    m.buttons.buttons = ["Play", "Shuffle"]
    m.buttons.observeField("buttonSelected", "onButton")
    m.tracks.observeField("itemSelected", "onTrack")
    m.top.observeField("active", "onActive")
end sub

sub onActive()
    if not m.top.active
        m.lastFocus = m.buttons
        if m.tracks.isInFocusChain() then m.lastFocus = m.tracks
        return
    end if
    uiChangesSince()
    if m.started
        if m.buttons.visible then m.lastFocus.setFocus(true)
        return
    end if
    m.started = true
    m.title.text = httpSentence(m.top.params.title)
    m.top.setFocus(true)
    path = "/api/items/" + urlValue(m.top.params.id)
    uiGet(path, "onAlbum")
    uiRequest({ method: "GET", path: path + "/children", build: "tracks" }, "onTracks")
end sub

' Tracks have no watched marks or My List: only Add to playlist is
' offered here, which changes nothing shown.
sub onTitleChanged(change as object)
    m.lastChange = change
end sub

sub onAlbum(event as object)
    answer = event.getData()
    if not answer.ok or answer.data = invalid
        if not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    m.album = answer.data
    m.title.text = httpSentence(m.album.title)
    showLine()
    image = contentTileImage(m.album, m.global.session.server, false, 420)
    agent = m.top.getScene().getHttpAgent()
    m.cover.setHttpAgent(agent)
    m.cover.uri = image
    m.backdrop.setHttpAgent(agent)
    m.backdrop.uri = image
end sub

sub showLine()
    if m.album <> invalid then m.line.text = musicAlbumLine(m.album, m.queue)
end sub

sub onTracks(event as object)
    answer = event.getData()
    if not answer.ok
        if not uiFailed(answer) then m.message.text = answer.message
        return
    end if
    m.queue = answer.data.queue
    m.tracks.content = event.getRoSGNode().content
    showLine()
    if m.queue.Count() = 0
        m.message.text = "This album has no tracks."
        return
    end if
    m.message.visible = false
    m.buttons.visible = true
    wanted = httpNumber(m.top.params.trackId, 0)
    at = -1
    for i = 0 to m.queue.Count() - 1
        if m.queue[i].itemId = wanted then at = i
    end for
    if not m.top.isInFocusChain() then return
    if at >= 0
        m.tracks.jumpToItem = at
        m.tracks.setFocus(true)
    else
        m.buttons.setFocus(true)
    end if
end sub

' The album from its track at, or shuffled: from any track, the others
' after it in a random order. The music player keeps the album's order
' for Previous and Stop shuffling.
sub play(at as integer, shuffled as boolean)
    if m.queue.Count() = 0 then return
    if shuffled then at = Rnd(m.queue.Count()) - 1
    m.global.music.play = { queue: m.queue, at: at, shuffled: shuffled }
    uiNavigate({ to: "nowPlaying" })
end sub

sub onButton(event as object)
    play(0, event.getData() = 1)
end sub

sub onTrack(event as object)
    play(event.getData(), false)
end sub

' Right from the buttons goes to the tracks, left back; the Options key on
' a track adds it to a playlist.
function onKeyEvent(key as string, press as boolean) as boolean
    if not press then return false
    hasTracks = m.queue.Count() > 0
    if key = "right" and m.buttons.isInFocusChain() and hasTracks
        m.tracks.setFocus(true)
        return true
    end if
    if key = "left" and m.tracks.isInFocusChain()
        m.buttons.setFocus(true)
        return true
    end if
    if key = "options" and hasTracks
        if m.tracks.isInFocusChain()
            track = m.queue[m.tracks.itemFocused]
            uiTileOptions({ itemId: track.itemId, kind: "track", title: track.title }, false)
        else if m.album <> invalid
            uiTileOptions({ itemId: m.album.id, kind: "album", title: httpSentence(m.album.title) }, false)
        end if
        return true
    end if
    return false
end function
