' The server's event stream read from a socket: the request, the answer's
' head and chunks, and its events.

function eventsTestBytes(text as string) as object
    bytes = CreateObject("roByteArray")
    bytes.FromAsciiString(text)
    return bytes
end function

sub test_events_request_only_for_http()
    crlf = Chr(13) + Chr(10)
    request = evRequest("http://nas:7539", "pley_at_1", "pleyRoku/1.0.0 (Roku OS 15.0; Roku Ultra)")
    tkEqual(request.host, "nas")
    tkEqual(request.port, 7539)
    tkEqual(request.text, "GET /api/events HTTP/1.1" + crlf + "Host: nas:7539" + crlf + "Authorization: Bearer pley_at_1" + crlf + "Accept: text/event-stream" + crlf + "Cache-Control: no-cache" + crlf + "User-Agent: pleyRoku/1.0.0 (Roku OS 15.0; Roku Ultra)" + crlf + crlf)
    tkEqual(evRequest("http://nas", "t", "ua").port, 80)
    tkEqual(evRequest("http://[fd00::5]:7539", "t", "ua").host, "fd00::5", "an IPv6 address without its brackets")
    tkInvalid(evRequest("https://pley.example.com", "t", "ua"), "no TLS on Roku's sockets")
    tkInvalid(evRequest("http://nas:7539", "", "ua"), "signed out")
    ' As a receiver of casts (Cast.brs).
    receiver = evRequest("http://nas:7539", "t", "ua", castReceiverQuery("8f2c-41", "Den", true))
    tkEqual(receiver.text.Split(crlf)[0], "GET /api/events?receiver=8f2c-41&device_name=Den&device_kind=tv&casts=item,live HTTP/1.1")
end sub

sub test_events_from_a_chunked_answer_in_pieces()
    crlf = Chr(13) + Chr(10)
    reader = evReader()
    head = "HTTP/1.1 200 OK" + crlf + "Content-Type: text/event-stream" + crlf + "Transfer-Encoding: chunked" + crlf + crlf
    tkEqual(evRead(reader, eventsTestBytes("HTTP/1.1 200")).Count(), 0, "half a head")
    first = "data: {""type"":""user_state_changed"",""item_ids"":[101,100,99],""children"":false}" + Chr(10) + Chr(10)
    chunk = LCase(StrI(Len(first), 16)) + crlf + first + crlf
    events = evRead(reader, eventsTestBytes(Mid(head, 13) + Left(chunk, 20)))
    tkEqual(events.Count(), 0, "a chunk cut short")
    tkTrue(reader.chunked)
    events = evRead(reader, eventsTestBytes(Mid(chunk, 21) + "3" + crlf + ": k" + crlf))
    tkEqual(events.Count(), 1)
    tkEqual(events[0].type, "user_state_changed")
    tkEqual(events[0].item_ids, [101, 100, 99])
    tkFalse(reader.ended)
    evRead(reader, eventsTestBytes("0" + crlf + crlf))
    tkTrue(reader.ended, "the last chunk")
end sub

sub test_events_from_a_plain_answer_and_refusals()
    crlf = Chr(13) + Chr(10)
    reader = evReader()
    events = evRead(reader, eventsTestBytes("HTTP/1.1 200 OK" + crlf + crlf + "event: x" + crlf + "data: {""type"":""watchlist_changed""}" + crlf + crlf + "data: not json" + crlf + crlf))
    tkEqual(events.Count(), 1, "CRLF lines too; data that isn't JSON left out")
    tkTrue(evChangesHome(events[0]))
    refused = evReader()
    evRead(refused, eventsTestBytes("HTTP/1.1 401 Unauthorized" + crlf + crlf))
    tkTrue(refused.failed)
    tkEqual(evHex("1a3;name=x"), 419)
    tkEqual(evHex("zz"), -1)
    tkFalse(evChangesHome({ type: "scan_progress" }))
    tkTrue(evChangesHome({ type: "live_channels_changed" }), "Live TV set up or gone: the menu follows")
    tkEqual(evRetryMs(1), 2000)
    tkEqual(evRetryMs(3), 8000)
    tkEqual(evRetryMs(20), 60000)
end sub
