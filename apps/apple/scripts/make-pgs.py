#!/usr/bin/env python3
"""Writes a small PGS (.sup) subtitle stream for Pley's test media.

FFmpeg can't encode PGS, nor turn text into pictures, so make-test-media.sh
uses this to get a picture subtitle track into an MKV (`-c copy`). Each cue
is a display set that shows one picture (a framed box with bars in it, a
different number of bars per cue), and one that clears it:

    make-pgs.py <out.sup> [width height]

The stream is 1920x1080 unless given; the cues are fixed (see CUES).
"""

import struct
import sys

# (start seconds, end seconds) of each cue.
CUES = [(2.0, 6.0), (12.0, 16.0), (24.0, 28.0)]
BOX_W, BOX_H = 480, 96

# Segment types.
PDS, ODS, PCS, WDS, END = 0x14, 0x15, 0x16, 0x17, 0x80


def segment(kind, pts, data):
    """One segment: "PG", PTS and DTS at 90 kHz, its type and size."""
    ticks = int(round(pts * 90_000))
    return b"PG" + struct.pack(">IIBH", ticks, 0, kind, len(data)) + data


def run(colour, length):
    """A run of `length` pixels of palette entry `colour`, as PGS codes it."""
    if colour != 0 and length < 3:
        return bytes([colour]) * length
    if colour == 0:
        if length < 64:
            return bytes([0, length])
        return bytes([0, 0x40 | (length >> 8), length & 0xFF])
    if length < 64:
        return bytes([0, 0x80 | length, colour])
    return bytes([0, 0xC0 | (length >> 8), length & 0xFF, colour])


def picture(bars):
    """The cue's picture: a white frame, `bars` white bars, and a dark inside."""
    rows = []
    for y in range(BOX_H):
        line = []
        for x in range(BOX_W):
            frame = x < 4 or x >= BOX_W - 4 or y < 4 or y >= BOX_H - 4
            bar = 16 <= y < BOX_H - 16 and (x - 24) % 64 < 24 and 24 <= x < 24 + 64 * bars
            line.append(1 if frame or bar else 2)
        rows.append(line)
    out = bytearray()
    for line in rows:
        start = 0
        while start < len(line):
            end = start
            while end < len(line) and line[end] == line[start] and end - start < 16383:
                end += 1
            out += run(line[start], end - start)
            start = end
        out += b"\x00\x00"
    return bytes(out)


def display_set(pts, number, width, height, rle):
    """A cue's start: composition, window, palette, the picture and its end."""
    x, y = (width - BOX_W) // 2, height - BOX_H - height // 10
    pcs = struct.pack(">HHBHBBBB", width, height, 0x10, number, 0x80, 0, 0, 1)
    pcs += struct.pack(">HBBHH", 0, 0, 0, x, y)
    wds = struct.pack(">BBHHHH", 1, 0, x, y, BOX_W, BOX_H)
    # Entries: id, Y, Cr, Cb, alpha. White and a dark grey, both opaque.
    pds = struct.pack(">BB", 0, 0) + bytes([1, 235, 128, 128, 255, 2, 32, 128, 128, 255])
    body = struct.pack(">HH", BOX_W, BOX_H) + rle
    ods = struct.pack(">HBB", 0, 0, 0xC0) + len(body).to_bytes(3, "big") + body
    return b"".join(
        segment(kind, pts, data) for kind, data in ((PCS, pcs), (WDS, wds), (PDS, pds), (ODS, ods), (END, b""))
    )


def clear_set(pts, number, width, height):
    """A cue's end: a composition with no picture."""
    x, y = (width - BOX_W) // 2, height - BOX_H - height // 10
    pcs = struct.pack(">HHBHBBBB", width, height, 0x10, number, 0x00, 0, 0, 0)
    wds = struct.pack(">BBHHHH", 1, 0, x, y, BOX_W, BOX_H)
    return segment(PCS, pts, pcs) + segment(WDS, pts, wds) + segment(END, pts, b"")


def main():
    if len(sys.argv) not in (2, 4):
        sys.exit("usage: make-pgs.py <out.sup> [width height]")
    width, height = (int(sys.argv[2]), int(sys.argv[3])) if len(sys.argv) == 4 else (1920, 1080)
    # An empty screen at 0 s: FFmpeg starts the stream's time at its first
    # display set, so this keeps the cues at their own times.
    out = bytearray(clear_set(0.0, 0, width, height))
    for n, (start, end) in enumerate(CUES):
        out += display_set(start, n * 2 + 1, width, height, picture(n + 1))
        out += clear_set(end, n * 2 + 2, width, height)
    with open(sys.argv[1], "wb") as f:
        f.write(out)


if __name__ == "__main__":
    main()
