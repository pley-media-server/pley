' The server's event stream on a socket (lib/Events.brs): connected while
' there is a server and a token, again with each new token (the server
' ends a stream when its token does) and after the stream ends or fails,
' 2 s later and doubling to a minute. Each event goes to `event`, for the
' scene, and `live` says whether the stream is up (while it isn't, the
' scene asks the server itself). An https server's stream isn't followed:
' Roku's sockets have no TLS. A socket that misbehaves is closed and tried
' again, never more: the screens still ask the server as they show.

sub init()
    m.top.functionName = "eventsRun"
end sub

sub eventsRun()
    m.port = CreateObject("roMessagePort")
    m.top.observeField("server", m.port)
    m.top.observeField("token", m.port)
    m.clock = CreateObject("roTimespan")
    m.socket = invalid
    m.reader = invalid
    ' What of the request the socket hasn't taken yet.
    m.unsent = ""
    m.connectedAt = 0
    m.failures = 0
    ' When to connect again (clock ms), -1 for not.
    m.connectAt = -1
    connect()
    while true
        msg = wait(1000, m.port)
        kind = type(msg)
        if kind = "roSGNodeEvent"
            ' Another server or token: at once, with it.
            m.failures = 0
            connect()
        else if kind = "roSocketEvent"
            if m.socket <> invalid and msg.getSocketID() = m.socket.getID()
                if m.unsent <> "" and m.socket.isWritable() then sendSome()
                if m.socket <> invalid and m.socket.isReadable() then readSome()
            end if
        end if
        ' A request the socket didn't take all of is tried again each second
        ' too, should a notice that it can take more not come.
        if m.socket <> invalid and m.unsent <> "" and kind <> "roSocketEvent" then sendSome()
        ' Not taken in 10 s: the socket isn't going anywhere.
        if m.socket <> invalid and m.unsent <> "" and m.clock.TotalMilliseconds() - m.connectedAt > 10000 then later()
        if m.socket = invalid and m.connectAt >= 0 and m.clock.TotalMilliseconds() >= m.connectAt then connect()
    end while
end sub

sub connect()
    close()
    m.connectAt = -1
    request = evRequest(m.top.server, m.top.token, m.top.userAgent)
    if request = invalid then return
    connected = false
    socket = invalid
    try
        address = CreateObject("roSocketAddress")
        address.setHostName(request.host)
        address.setPort(request.port)
        socket = CreateObject("roStreamSocket")
        socket.setMessagePort(m.port)
        socket.setSendToAddress(address)
        socket.notifyReadable(true)
        socket.notifyWritable(true)
        ' A socket with a port connects in the background.
        connected = socket.connect() or socket.eInProgress()
    catch failure
        connected = false
    end try
    if not connected
        if socket <> invalid then socket.close()
        later()
        return
    end if
    ' The request goes as the socket can take it.
    m.socket = socket
    m.reader = evReader()
    m.unsent = request.text
    m.connectedAt = m.clock.TotalMilliseconds()
    sendSome()
end sub

' As much of the request as the socket takes now.
sub sendSome()
    sent = -1
    try
        sent = m.socket.sendStr(m.unsent)
        ' Not connected yet, or full: when it can take more.
        if sent < 0 and (m.socket.eWouldBlock() or m.socket.eAgain() or m.socket.eInProgress()) then sent = 0
    catch failure
        sent = -1
    end try
    if sent < 0
        later()
        return
    end if
    m.unsent = Mid(m.unsent, sent + 1)
    if m.unsent = "" then m.socket.notifyWritable(false)
end sub

' What came: its events go to the scene. Nothing to read on a readable
' socket is the server closing it.
sub readSome()
    received = invalid
    try
        available = m.socket.getCountRcvBuf()
        if available > 16384 then available = 16384
        if available > 0
            buffer = CreateObject("roByteArray")
            buffer[available - 1] = 0
            count = m.socket.receive(buffer, 0, available)
            if count > 0
                received = buffer
                if count < available
                    received = CreateObject("roByteArray")
                    for i = 0 to count - 1
                        received.Push(buffer[i])
                    end for
                end if
            end if
        end if
    catch failure
        received = invalid
    end try
    if received = invalid
        later()
        return
    end if
    events = evRead(m.reader, received)
    if not m.reader.head and not m.reader.failed
        m.failures = 0
        if not m.top.live then m.top.live = true
    end if
    ' The token ran out (the stream ends with it): the scene has the API
    ' task renew it, and the new one connects again.
    if m.reader.status = 401 then m.top.tokenRefused = m.top.token
    for each event in events
        m.top.event = event
    end for
    if m.reader.failed or m.reader.ended then later()
end sub

sub later()
    close()
    m.failures = m.failures + 1
    m.connectAt = m.clock.TotalMilliseconds() + evRetryMs(m.failures)
end sub

sub close()
    if m.socket <> invalid then m.socket.close()
    m.socket = invalid
    m.reader = invalid
    m.unsent = ""
    if m.top.live then m.top.live = false
end sub
