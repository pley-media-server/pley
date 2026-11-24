' Seek previews from the server's BIF file (docs/API.md, previews.bif): a
' 64-byte header (magic, version, the frame count at byte 12, the interval
' in ms at byte 16), then an index of (frame number, offset) pairs, eight
' bytes each, ending with 0xffffffff and the end's offset, then the JPEGs.
' Frame n shows the video at n × interval. The player draws them itself
' (its own controls hide the Video node's), PreviewTask cutting out the
' frame asked for.

' The four bytes at at, little-endian, as a whole number.
function prvWord(bytes as object, at as integer) as double
    if at + 4 > bytes.Count() then return -1
    return bytes[at] + bytes[at + 1] * 256# + bytes[at + 2] * 65536# + bytes[at + 3] * 16777216#
end function

' The header's { count, intervalMs }, invalid when it isn't a BIF file.
function prvHeader(bytes as object) as dynamic
    if bytes.Count() < 64 then return invalid
    magic = [&h89, &h42, &h49, &h46, &h0D, &h0A, &h1A, &h0A]
    for i = 0 to 7
        if bytes[i] <> magic[i] then return invalid
    end for
    count = prvWord(bytes, 12)
    interval = prvWord(bytes, 16)
    ' A frame every second of a day at most: anything more is a broken file.
    if count <= 0 or count > 86400 then return invalid
    if interval <= 0 then interval = 1000
    return { count: Int(count), intervalMs: Int(interval) }
end function

' The bytes of the index after the header: count + 1 pairs.
function prvIndexLength(header as object) as integer
    return (header.count + 1) * 8
end function

' Where each frame starts, and where the last ends (count + 1 offsets),
' from the index's bytes; invalid when they go backwards.
function prvOffsets(bytes as object, count as integer) as dynamic
    offsets = []
    for i = 0 to count
        offset = prvWord(bytes, i * 8 + 4)
        if offset < 0 then return invalid
        if offsets.Count() > 0 and offset < offsets[offsets.Count() - 1] then return invalid
        offsets.Push(offset)
    end for
    return offsets
end function

' The frame shown at ms: { n, start, length }, invalid without one.
function prvFrame(header as object, offsets as object, ms as integer) as dynamic
    if header = invalid or offsets = invalid then return invalid
    if ms < 0 then ms = 0
    n = Int(ms / header.intervalMs)
    if n > header.count - 1 then n = header.count - 1
    start = offsets[n]
    length = offsets[n + 1] - start
    ' A frame is a small JPEG: anything else is a broken file.
    if length <= 0 or length > 1048576 then return invalid
    return { n: n, start: start, length: length }
end function

' The frame's number for ms, as prvFrame takes it: previews are asked
' for once a frame, not once a key.
function prvFrameNumber(header as object, ms as integer) as integer
    if header = invalid then return -1
    if ms < 0 then ms = 0
    n = Int(ms / header.intervalMs)
    if n > header.count - 1 then n = header.count - 1
    return n
end function
