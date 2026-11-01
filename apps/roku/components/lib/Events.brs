' The server's event stream (GET /api/events, Server-Sent Events of
' ServerEvent JSON, docs/API.md), read from a socket of the Roku's own:
' roUrlTransfer gives an answer only once it is whole, and this one never
' ends. HTTP/1.1 by hand, so only for an http:// server: Roku's sockets
' have no TLS, and a server reached by https keeps Home's minute-old rule.
' The request and the parsing, without sockets;
' components/tasks/EventsTask reads.

' The request for server's stream with the access token: { host, port,
' text }, or invalid for an https server, an address that doesn't parse,
' or no token.
function evRequest(server as string, token as string, userAgent as string) as dynamic
    parts = urlParse(server)
    if parts = invalid or parts.scheme <> "http" or token = "" then return invalid
    port = parts.port
    if port = -1 then port = 80
    host = parts.host
    ' An IPv6 address: its brackets go in the Host header, not the socket.
    if Left(host, 1) = "[" then host = Mid(host, 2, Len(host) - 2)
    crlf = Chr(13) + Chr(10)
    lines = [
        "GET /api/events HTTP/1.1",
        "Host: " + Mid(urlOriginOf("http", parts.host, parts.port), 8),
        "Authorization: Bearer " + token,
        "Accept: text/event-stream",
        "Cache-Control: no-cache",
        "User-Agent: " + userAgent
    ]
    return { host: host, port: port, text: lines.Join(crlf) + crlf + crlf }
end function

' A reader of one answer: its head, then its body (chunked or not), then
' its events. failed: not a stream (refused, or not an answer); ended: the
' server ended it.
function evReader() as object
    return { head: true, raw: CreateObject("roByteArray"), status: 0, chunked: false, chunkLeft: 0, needCrlf: false, body: CreateObject("roByteArray"), ended: false, failed: false }
end function

' Reads received (roByteArray) into reader; the events completed by it, as
' objects ([ServerEvent]).
function evRead(reader as object, received as object) as object
    reader.raw.Append(received)
    if reader.head
        at = evFind(reader.raw, Chr(13) + Chr(10) + Chr(13) + Chr(10), 0)
        if at < 0
            ' A head this long isn't pley's.
            if reader.raw.Count() > 16384 then reader.failed = true
            return []
        end if
        lines = evText(reader.raw, 0, at).Split(Chr(13) + Chr(10))
        reader.raw = evTail(reader.raw, at + 4)
        reader.head = false
        words = lines[0].Split(" ")
        if words.Count() >= 2 then reader.status = Val(words[1])
        for each line in lines
            lower = LCase(line)
            if Left(lower, 18) = "transfer-encoding:" and Instr(1, lower, "chunked") > 0 then reader.chunked = true
        end for
        if reader.status <> 200
            reader.failed = true
            return []
        end if
    end if
    if reader.chunked
        evDechunk(reader)
    else
        evKeep(reader, reader.raw, 0, reader.raw.Count())
        reader.raw = CreateObject("roByteArray")
    end if
    return evEvents(reader)
end function

' A chunked body: the chunks' bytes go to the body, as far as they came.
sub evDechunk(reader as object)
    raw = reader.raw
    at = 0
    while true
        if reader.chunkLeft > 0
            take = raw.Count() - at
            if take > reader.chunkLeft then take = reader.chunkLeft
            evKeep(reader, raw, at, at + take)
            at = at + take
            reader.chunkLeft = reader.chunkLeft - take
            if reader.chunkLeft > 0 then exit while
            reader.needCrlf = true
        end if
        if reader.needCrlf
            if raw.Count() - at < 2 then exit while
            at = at + 2
            reader.needCrlf = false
        end if
        lineEnd = evFind(raw, Chr(13) + Chr(10), at)
        if lineEnd < 0 then exit while
        size = evHex(evText(raw, at, lineEnd))
        at = lineEnd + 2
        if size <= 0
            ' The last chunk (or one that isn't): the stream is over.
            reader.ended = true
            exit while
        end if
        reader.chunkLeft = size
    end while
    reader.raw = evTail(raw, at)
end sub

' The events whose blank line has come: each one's data lines, joined, as
' JSON (others, and comments, left out).
function evEvents(reader as object) as object
    events = []
    while true
        at = evFind(reader.body, Chr(10) + Chr(10), 0)
        if at < 0 then exit while
        block = evText(reader.body, 0, at)
        reader.body = evTail(reader.body, at + 2)
        data = []
        for each line in block.Split(Chr(10))
            if Left(line, 5) = "data:"
                value = Mid(line, 6)
                if Left(value, 1) = " " then value = Mid(value, 2)
                data.Push(value)
            end if
        end for
        if data.Count() > 0
            event = httpObject(httpJson(data.Join(Chr(10))))
            if event <> invalid and httpString(event.type) <> "" then events.Push(event)
        end if
    end while
    return events
end function

' The bytes of from from start to upto, carriage returns left out (lines
' end with LF then, whichever the server wrote).
sub evKeep(reader as object, from as object, start as integer, upto as integer)
    for i = start to upto - 1
        if from[i] <> 13 then reader.body.Push(from[i])
    end for
end sub

' Where the ASCII text needle starts in bytes, at or after from; -1 if not.
function evFind(bytes as object, needle as string, from as integer) as integer
    codes = []
    for i = 1 to Len(needle)
        codes.Push(Asc(Mid(needle, i, 1)))
    end for
    for i = from to bytes.Count() - codes.Count()
        found = true
        for j = 0 to codes.Count() - 1
            if bytes[i + j] <> codes[j]
                found = false
                exit for
            end if
        end for
        if found then return i
    end for
    return -1
end function

' The text of bytes from start to upto (UTF-8 kept as it is).
function evText(bytes as object, start as integer, upto as integer) as string
    part = CreateObject("roByteArray")
    for i = start to upto - 1
        part.Push(bytes[i])
    end for
    return part.ToAsciiString()
end function

function evTail(bytes as object, from as integer) as object
    out = CreateObject("roByteArray")
    for i = from to bytes.Count() - 1
        out.Push(bytes[i])
    end for
    return out
end function

' A chunk's size line ("1a3", "1a3;name=value") as a number; -1 if it
' isn't one.
function evHex(text as string) as integer
    digits = LCase(text.Split(";")[0].Trim())
    if digits = "" or Len(digits) > 7 then return -1
    value = 0
    for i = 1 to Len(digits)
        at = Instr(1, "0123456789abcdef", Mid(digits, i, 1))
        if at = 0 then return -1
        value = value * 16 + at - 1
    end for
    return value
end function

' Whether an event changes what Home and the libraries' rows show: watch
' state, My List, a library's titles, the libraries themselves, or Live
' TV's channels (the menu has Live TV only while there are some).
function evChangesHome(event as object) as boolean
    kind = httpString(event.type)
    return kind = "user_state_changed" or kind = "watchlist_changed" or kind = "library_changed" or kind = "libraries_changed" or kind = "live_channels_changed"
end function

' How long before connecting again after the stream ended or failed: 2 s,
' doubling, a minute at most.
function evRetryMs(failures as integer) as integer
    wait = 2000
    for i = 2 to failures
        wait = wait * 2
        if wait >= 60000 then return 60000
    end for
    return wait
end function
