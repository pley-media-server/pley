' Picture subtitles the app draws over the video, rather than having the
' server burn them in, so the file plays as it is and HDR stays HDR: PGS
' only (VobSub and DVB are still burned in). A keyed session's
' GET /api/media/{key}/subtitles-{index}.json?at={ms} lists a track's
' pictures, { complete, width, height, cues: [{ start_ms, end_ms, x, y,
' width, height, image }] }: each cue a PNG beside the list, shown at its
' box in the plane (width × height) from start_ms until end_ms, on the
' file's times. The server reads the pictures around `at` first, so the
' first answers can hold only those: while complete is false the player
' asks again, from where it is. An older server answers 404 and a
' transcoder too old 503: the pictures are burned in then.

' The picture subtitle codecs this app draws.
function picCodecs() as object
    return ["hdmv_pgs_subtitle"]
end function

' How long before a track that isn't whole yet is asked for again (ms), as
' the other TVs do.
function picAgainMs() as integer
    return 3000
end function

' The most cues kept of a track: a film's PGS track has a few thousand.
function picMaxCues() as integer
    return 20000
end function

' How often the Video node tells its position while pictures show (s): a
' picture comes and goes within this of its time.
function picNotifySeconds() as float
    return 0.2
end function

' Whether codec (ffprobe's name) is in list.
function picListed(list as object, codec as string) as boolean
    for each entry in list
        if entry = codec then return true
    end for
    return false
end function

' The file's subtitle stream at index, or invalid.
function picStream(file as dynamic, index as integer) as dynamic
    data = httpObject(file)
    if data = invalid or index < 0 then return invalid
    for each stream in httpArray(data.streams)
        if GetInterface(stream, "ifAssociativeArray") <> invalid and httpString(stream.kind) = "subtitle" and httpNumber(stream.index, -2) = index then return stream
    end for
    return invalid
end function

' Whether a subtitle stream (as GET /api/items/{id} lists a file's) is
' pictures this app draws.
function picDrawn(stream as dynamic) as boolean
    data = httpObject(stream)
    if data = invalid or httpString(data.kind) <> "subtitle" then return false
    if GetInterface(data.text_based, "ifBoolean") = invalid or data.text_based then return false
    return picListed(picCodecs(), LCase(httpString(data.codec)))
end function

' The track the app draws for a session (info, a PlaybackInfo) of file: the
' person's choice (selected_subtitle) when it is pictures this app draws
' and the server burned nothing in; -1 otherwise, and once the server
' turned out not to make pictures (unavailable).
function picTrack(info as dynamic, file as dynamic, unavailable as boolean) as integer
    if unavailable then return -1
    session = httpObject(info)
    if session = invalid or httpTrue(session.burn_subtitles) then return -1
    data = httpObject(file)
    if data = invalid then return -1
    index = Int(httpNumber(data.selected_subtitle, -1))
    if picDrawn(picStream(data, index)) then return index
    return -1
end function

' The folder of a keyed session's media, from its address
' (PlaybackInfo.url, "/api/media/pley_mk_…/file.mkv", or the same after
' server): "/api/media/pley_mk_…/", where its pictures are too. "" for an
' address without a key.
function picFolder(url as dynamic, server as string) as string
    path = httpString(url)
    if server <> "" and Left(path, Len(server)) = server then path = Mid(path, Len(server) + 1)
    re = CreateObject("roRegex", "^(/api/media/pley_mk_[A-Za-z0-9_-]+/)[^/?#]+(\?[^/#]*)?$", "")
    match = re.Match(path)
    if match.Count() < 2 then return ""
    return match[1]
end function

' The address (after the server) of track index's list, asked for at atMs,
' where the player is (ms of the file's time).
function picListPath(folder as string, index as integer, atMs as dynamic) as string
    at = Int(httpNumber(atMs, 0))
    if at < 0 then at = 0
    return folder + "subtitles-" + index.ToStr() + ".json?at=" + at.ToStr()
end function

' A list (its JSON, parsed) as { complete, width, height, cues }, each cue
' { startMs, endMs, x, y, width, height, image }, image the address of its
' PNG beside the list (with the list's at); invalid for an answer that
' isn't one. Cues stay in order of time without overlap: one that would
' overlap the one before, has no time, lies outside the plane or names
' anything but a PNG in the same folder is left out, and so is any past
' picMaxCues().
function picRead(data as dynamic) as dynamic
    list = httpObject(data)
    if list = invalid then return invalid
    planeWidth = httpNumber(list.width, 0)
    planeHeight = httpNumber(list.height, 0)
    if planeWidth < 1 or planeHeight < 1 or planeWidth > 8192 or planeHeight > 8192 then return invalid
    if GetInterface(list.cues, "ifArray") = invalid then return invalid
    name = CreateObject("roRegex", "^[A-Za-z0-9_-]+\.png(\?at=[0-9]+)?$", "")
    cues = []
    lastEnd = 0
    for each entry in list.cues
        if cues.Count() >= picMaxCues() then exit for
        cue = picCue(httpObject(entry), planeWidth, planeHeight, name)
        if cue <> invalid and cue.startMs >= lastEnd
            cues.Push(cue)
            lastEnd = cue.endMs
        end if
    end for
    return { complete: httpTrue(list.complete), width: Int(planeWidth), height: Int(planeHeight), cues: cues }
end function

' One cue of a list in a plane of planeWidth × planeHeight, or invalid.
function picCue(cue as dynamic, planeWidth as dynamic, planeHeight as dynamic, name as object) as dynamic
    if cue = invalid then return invalid
    startMs = httpNumber(cue.start_ms, -1)
    endMs = httpNumber(cue.end_ms, -1)
    x = httpNumber(cue.x, -1)
    y = httpNumber(cue.y, -1)
    width = httpNumber(cue.width, 0)
    height = httpNumber(cue.height, 0)
    image = httpString(cue.image)
    ' Two days in at most, as the server asks: well within an integer's ms.
    if startMs < 0 or endMs <= startMs or endMs > 172800000 then return invalid
    if x < 0 or y < 0 or width < 1 or height < 1 or x + width > planeWidth or y + height > planeHeight then return invalid
    if not name.IsMatch(image) then return invalid
    return { startMs: Int(startMs), endMs: Int(endMs), x: Int(x), y: Int(y), width: Int(width), height: Int(height), image: image }
end function

' The pictures known (picRead's, or invalid) with a newer answer's: a whole
' track's in their place; else the answer's for the time it covers (from
' its first cue's start to its last's end), and the ones known before and
' after it.
function picMerge(known as dynamic, answer as object) as object
    if known = invalid or answer.complete or known.width <> answer.width or known.height <> answer.height then return answer
    newer = answer.cues
    if newer.Count() = 0 then return { complete: false, width: known.width, height: known.height, cues: known.cues }
    first = newer[0].startMs
    last = newer[newer.Count() - 1].endMs
    cues = []
    for each cue in known.cues
        if cue.endMs <= first then cues.Push(cue)
    end for
    cues.Append(newer)
    for each cue in known.cues
        if cue.startMs >= last then cues.Push(cue)
    end for
    return { complete: false, width: answer.width, height: answer.height, cues: cues }
end function

' The cue showing at positionMs (start <= position < end), or -1.
function picAt(cues as object, positionMs as integer) as integer
    low = 0
    high = cues.Count() - 1
    while low <= high
        middle = (low + high) \ 2
        cue = cues[middle]
        if positionMs < cue.startMs
            high = middle - 1
        else if positionMs >= cue.endMs
            low = middle + 1
        else
            return middle
        end if
    end while
    return -1
end function

' The first cue to start after positionMs, or -1.
function picNext(cues as object, positionMs as integer) as integer
    low = 0
    high = cues.Count()
    while low < high
        middle = (low + high) \ 2
        if cues[middle].startMs <= positionMs
            low = middle + 1
        else
            high = middle
        end if
    end while
    if low < cues.Count() then return low
    return -1
end function

' Whether the Video node's position moved to positionMs from lastMs (-1 for
' none yet) as a seek does, rather than by playing: further than the clock
' went from lastAt to now (ms), or back.
function picSeeked(lastMs as integer, lastAt as integer, positionMs as integer, now as integer) as boolean
    if lastMs < 0 then return false
    moved = positionMs - lastMs
    return moved - (now - lastAt) > 3000 or moved < -1000
end function

' Whether a picture whose load failed at failedAt (ms, invalid for never)
' is loaded again now: a few seconds after, as a list is asked again.
function picLoadAgain(failedAt as dynamic, now as integer) as boolean
    if failedAt = invalid then return true
    return now - failedAt >= picAgainMs()
end function

' What the overlay does with pictures (picRead's, or invalid) at
' positionMs, the Video node's, with the subtitle sync syncMs (later when
' positive): { show, preload }, the cue it shows and the next to come,
' which it loads meanwhile (-1 for none).
function picPlan(pictures as dynamic, positionMs as integer, syncMs as integer) as object
    plan = { show: -1, preload: -1 }
    if pictures = invalid then return plan
    at = positionMs - syncMs
    plan.show = picAt(pictures.cues, at)
    plan.preload = picNext(pictures.cues, at)
    return plan
end function

' width × height fitted to a screen of screenWidth × screenHeight, centred:
' { x, y, width, height }, in whole pixels.
function picFit(width as dynamic, height as dynamic, screenWidth as integer, screenHeight as integer) as object
    scale = screenWidth / width
    if screenHeight / height < scale then scale = screenHeight / height
    fitWidth = Int(width * scale + 0.5)
    fitHeight = Int(height * scale + 0.5)
    return { x: Int((screenWidth - fitWidth) / 2), y: Int((screenHeight - fitHeight) / 2), width: fitWidth, height: fitHeight }
end function

' Where the picture of a file's video shows on a screen of screenWidth ×
' screenHeight, fitted to it (picFit); invalid when the file doesn't say
' the video's size. A video of 576 lines or fewer shows as 16:9, as a
' DVD's or an SD Blu-ray's anamorphic picture does: the file only says how
' it is stored.
function picPicture(file as dynamic, screenWidth as integer, screenHeight as integer) as dynamic
    data = httpObject(file)
    if data = invalid then return invalid
    for each stream in httpArray(data.streams)
        if GetInterface(stream, "ifAssociativeArray") <> invalid and httpString(stream.kind) = "video"
            width = httpNumber(stream.width, 0)
            height = httpNumber(stream.height, 0)
            if width < 1 or height < 1 then return invalid
            if height <= 576 then width = height * 16 / 9
            return picFit(width, height, screenWidth, screenHeight)
        end if
    end for
    return invalid
end function

' Where pictures' plane goes, as the web player puts it
' (apps/web/src/player/pictures.ts, planeBox): over the picture
' (picPicture's), which a DVD's or SD DVB's plane (576 lines or fewer)
' stretches over, as the video it came with is stored anamorphic. An HD
' plane of another shape than the picture (a Blu-ray's 1920×1080 over a
' film cropped to 1920×800) keeps its own shape, fitted to the screen: its
' lines go where the disc puts them, in the black bars when there are any.
' So does any plane when the video's size isn't known.
function picPlane(pictures as object, picture as dynamic, screenWidth as integer, screenHeight as integer) as object
    if picture = invalid then return picFit(pictures.width, pictures.height, screenWidth, screenHeight)
    if pictures.height <= 576 then return picture
    shapes = (pictures.width / pictures.height) / (picture.width / picture.height)
    if Abs(shapes - 1) < 0.02 then return picture
    return picFit(pictures.width, pictures.height, screenWidth, screenHeight)
end function

' Where a cue of pictures shows, its plane at plane (picPlane's): { x, y,
' width, height }, in whole pixels.
function picBox(cue as object, pictures as object, plane as object) as object
    scaleX = plane.width / pictures.width
    scaleY = plane.height / pictures.height
    return {
        x: Int(plane.x + cue.x * scaleX + 0.5),
        y: Int(plane.y + cue.y * scaleY + 0.5),
        width: Int(cue.width * scaleX + 0.5),
        height: Int(cue.height * scaleY + 0.5)
    }
end function

' What an answer for a list means, its data read by picRead already (the
' API task's build "pictures"): "ok"; "unavailable" when this server can't
' make pictures of the track, so they are burned in from then on: an older
' server, which knows no such file (404 "unknown file", or "unknown
' playlist file" in a stream), a transcoder too old to copy the track
' (503), a track that isn't PGS (404), or a request it refuses; "stopped"
' for a session ended (410), which the player hears of itself, or a
' refusal (403); "again" for anything else, asked again later: the network,
' a disk asleep, a transcoder out of reach, a session gone for a moment.
' Burning them in would need what is missing there too.
function picAnswerKind(answer as object) as string
    if answer.ok
        data = httpObject(answer.data)
        if data = invalid or GetInterface(data.cues, "ifArray") = invalid then return "again"
        return "ok"
    end if
    status = answer.status
    if status = 410 or status = 403 then return "stopped"
    if status = 400 or status = 405 or status = 501 then return "unavailable"
    ' Told apart by what the server says: the codes are every 404's and 503's.
    message = ""
    body = httpObject(answer.data)
    if body <> invalid then message = LCase(httpString(body.message))
    if status = 404 and (message = "unknown file" or message = "unknown playlist file" or message = "no pgs subtitle track has this index") then return "unavailable"
    if status = 503 and Instr(1, message, "can't copy subtitle tracks") > 0 then return "unavailable"
    return "again"
end function
