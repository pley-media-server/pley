' Subtitle sync, as the Android TV player's Sync: text subtitles moved in
' 0.1 s steps from -10 s to +10 s, later when positive, remembered per file
' on this Roku. The Video node draws a sideloaded track as it comes, so the
' server moves the cues (GET /api/files/{id}/subtitles/{index}.vtt with
' offset_ms, docs/API.md) and the player opens its stream again where it
' was, every text track moved. section is a roRegistrySection, or a
' stand-in with the same methods in tests.

function syncStepMs() as integer
    return 100
end function

function syncMaxMs() as integer
    return 10000
end function

' How many files' offsets are kept, the latest: the registry is small.
function syncKeep() as integer
    return 50
end function

' offsetMs rounded to a step and kept within 10 s either way.
function syncClamp(offsetMs as dynamic) as integer
    ms = httpNumber(offsetMs, 0)
    rounded = Int(Abs(ms) / syncStepMs() + 0.5) * syncStepMs()
    if rounded > syncMaxMs() then rounded = syncMaxMs()
    if ms < 0 then return -rounded
    return rounded
end function

' "+0.3 s", "-1.2 s", "0.0 s" ("+0,3 s" in French and Spanish).
function syncLabel(offsetMs as integer) as string
    tenths = Int(Abs(offsetMs) / 100 + 0.5)
    text = i18nNumber(tenths / 10, 1) + " s"
    if offsetMs > 0 then return "+" + text
    if offsetMs < 0 then return "-" + text
    return text
end function

' A text track's address with its cues moved offsetMs, as the server has
' it (the address itself for 0); one moved already is moved from scratch.
function syncTrackUrl(url as string, offsetMs as integer) as string
    base = syncBaseUrl(url)
    if offsetMs = 0 or base = "" then return base
    separator = "?"
    if Instr(1, base, "?") > 0 then separator = "&"
    return base + separator + "offset_ms=" + offsetMs.ToStr()
end function

' A track's address without the offset this app put on it, always last.
function syncBaseUrl(url as string) as string
    at = Instr(1, url, "offset_ms=")
    if at < 2 then return url
    return Left(url, at - 2)
end function

' contentStream's subtitleTracks with their cues moved offsetMs.
function syncTracks(tracks as object, offsetMs as integer) as object
    moved = []
    for each track in tracks
        copy = {}
        copy.Append(track)
        copy.TrackName = syncTrackUrl(httpString(track.TrackName), offsetMs)
        moved.Push(copy)
    end for
    return moved
end function

' What a file's offset is remembered under: its server and its id.
function syncKey(server as string, fileId as dynamic) as string
    return server + "|" + Int(httpNumber(fileId, 0)).ToStr()
end function

' The offset remembered for key, 0 when none is.
function syncOffset(section as object, key as string) as integer
    for each pair in syncPairs(section)
        if pair[0] = key then return pair[1]
    end for
    return 0
end function

' Remembers offsetMs for key (0 forgets it), the latest last, the oldest
' going once syncKeep() are kept; false when the registry couldn't be
' written.
function syncRemember(section as object, key as string, offsetMs as integer) as boolean
    kept = []
    for each pair in syncPairs(section)
        if pair[0] <> key then kept.Push(pair)
    end for
    if offsetMs <> 0 then kept.Push([key, syncClamp(offsetMs)])
    while kept.Count() > syncKeep()
        kept.Shift()
    end while
    if not section.Write("offsets", FormatJson(kept)) then return false
    return section.Flush()
end function

' The [key, offset] pairs kept, oldest first.
function syncPairs(section as object) as object
    pairs = []
    if not section.Exists("offsets") then return pairs
    for each pair in httpArray(httpJson(section.Read("offsets")))
        if GetInterface(pair, "ifArray") <> invalid and pair.Count() = 2
            key = httpString(pair[0])
            if key <> "" then pairs.Push([key, syncClamp(pair[1])])
        end if
    end for
    return pairs
end function
