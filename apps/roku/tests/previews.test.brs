' A BIF of count frames, interval ms apart, each frame length bytes.
function previewsTestBif(count as integer, interval as integer, length as integer) as object
    bytes = CreateObject("roByteArray")
    for each b in [&h89, &h42, &h49, &h46, &h0D, &h0A, &h1A, &h0A]
        bytes.Push(b)
    end for
    previewsTestWord(bytes, 0)
    previewsTestWord(bytes, count)
    previewsTestWord(bytes, interval)
    while bytes.Count() < 64
        bytes.Push(0)
    end while
    offset = 64 + (count + 1) * 8
    for n = 0 to count - 1
        previewsTestWord(bytes, n)
        previewsTestWord(bytes, offset + n * length)
    end for
    previewsTestWord(bytes, &hFFFFFFFF)
    previewsTestWord(bytes, offset + count * length)
    return bytes
end function

sub previewsTestWord(bytes as object, value as dynamic)
    v = value
    if v < 0 then v = v + 4294967296#
    for i = 0 to 3
        bytes.Push(Int(v - Int(v / 256) * 256))
        v = Int(v / 256)
    end for
end sub

' The index's bytes: what follows the header.
function previewsTestIndex(bytes as object) as object
    out = CreateObject("roByteArray")
    for i = 64 to bytes.Count() - 1
        out.Push(bytes[i])
    end for
    return out
end function

sub test_previews_header()
    bif = previewsTestBif(3, 10000, 500)
    header = prvHeader(bif)
    tkEqual(header, { count: 3, intervalMs: 10000 })
    tkEqual(prvIndexLength(header), 32)
    bad = previewsTestBif(3, 10000, 500)
    bad[1] = 0
    tkInvalid(prvHeader(bad), "not a BIF")
    short = CreateObject("roByteArray")
    tkInvalid(prvHeader(short))
end sub

sub test_previews_frames()
    bif = previewsTestBif(3, 10000, 500)
    header = prvHeader(bif)
    offsets = prvOffsets(previewsTestIndex(bif), header.count)
    tkEqual(offsets, [96, 596, 1096, 1596])
    tkEqual(prvFrame(header, offsets, 0), { n: 0, start: 96, length: 500 })
    tkEqual(prvFrame(header, offsets, 19999), { n: 1, start: 596, length: 500 })
    tkEqual(prvFrame(header, offsets, 999999).n, 2, "the last past the end")
    tkEqual(prvFrameNumber(header, 25000), 2)
    tkEqual(prvFrameNumber(header, -5), 0)
end sub

sub test_previews_reject_a_broken_index()
    bif = previewsTestBif(2, 10000, 500)
    index = previewsTestIndex(bif)
    ' The second frame's offset before the first's.
    index[12] = 0
    index[13] = 0
    tkInvalid(prvOffsets(index, 2))
end sub
