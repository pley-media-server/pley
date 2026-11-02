"""Draws the watched mark of tiles into images/watched.png, and the focus
of buttons (images/button-focus.9.png, and where it was as a button group
lost it, images/button-footprint.9.png): nine-patch pictures, a rounded
box in the app's accent or its raised surface, as its lists have. The
Live TV guide's programmes sit on images/guide-cell.9.png, and what has
aired on images/guide-past.9.png; images/spinner.png turns while a channel
tunes, and images/round.png makes a person's photo round. The
icons and splash screens are pley's logo, rendered with the other apps'
icons by scripts/render-icons.mjs at the repository's root. Standard
library only.

    python scripts/make_images.py
"""

import math
import os
import struct
import zlib

DARK = (0x16, 0x19, 0x1E)  # the app's background (components/shared/Ui.brs)
ACCENT = (0x38, 0xBD, 0xF8)  # the app's accent
SURFACE3 = (0x2E, 0x34, 0x40)  # its raised surface
SURFACE2 = (0x26, 0x2B, 0x35)  # a guide's programme
PAST = (0x1E, 0x22, 0x2A, 0x99)  # the guide's time gone, over the screen

# A button's box: its size inside the nine-patch's border, and its corners.
BOX = 32
RADIUS = 8

# The watched mark, drawn at this size on tiles (components/items/Tile.brs).
MARK_SIZE = 40

# The spinner a BusySpinner turns (components/screens/LivePlayerScreen).
SPINNER_SIZE = 64

# The mask of round pictures, a person's photo (components/screens/
# PersonScreen), drawn at this size.
ROUND_SIZE = 160


def segment_distance(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


def render_mark(size):
    """RGBA rows of the watched mark: a check on an accent disc."""
    check = [(0.27, 0.52), (0.43, 0.68), (0.74, 0.36)]
    half_stroke = 0.065
    samples = 4
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            disc = ink = 0
            for sy in range(samples):
                for sx in range(samples):
                    x = (col + (sx + 0.5) / samples) / size
                    y = (row + (sy + 0.5) / samples) / size
                    if math.hypot(x - 0.5, y - 0.5) <= 0.5:
                        disc += 1
                        near = min(segment_distance(x, y, *check[i], *check[i + 1]) for i in range(len(check) - 1))
                        if near <= half_stroke:
                            ink += 1
            total = samples * samples
            cover = ink / disc if disc else 0
            for channel in range(3):
                line.append(round(ACCENT[channel] * (1 - cover) + DARK[channel] * cover))
            line.append(round(255 * disc / total))
        rows.append(bytes(line))
    return rows


def render_box(color, radius=RADIUS):
    """RGBA rows of a nine-patch (Roku's .9.png): a rounded box of color
    (with its alpha, if it has one), in a border of one pixel whose black
    marks say what stretches (top and left)."""
    alpha = color[3] if len(color) > 3 else 255
    color = color[:3]
    size = BOX + 2
    samples = 4
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            if row in (0, size - 1) or col in (0, size - 1):
                edge = max(radius, 1)
                marked = (row == 0 and edge < col < size - 1 - edge) or (col == 0 and edge < row < size - 1 - edge)
                line.extend((0, 0, 0, 255) if marked else (0, 0, 0, 0))
                continue
            inside = 0
            for sy in range(samples):
                for sx in range(samples):
                    x = col - 1 + (sx + 0.5) / samples
                    y = row - 1 + (sy + 0.5) / samples
                    dx = max(radius - x, 0, x - (BOX - radius))
                    dy = max(radius - y, 0, y - (BOX - radius))
                    if math.hypot(dx, dy) <= radius:
                        inside += 1
            line.extend(color)
            line.append(round(alpha * inside / (samples * samples)))
        rows.append(bytes(line))
    return rows


def render_spinner(size):
    """RGBA rows of a ring in the accent, three quarters drawn, fading
    from its head to its tail."""
    samples = 4
    outer = 0.5
    inner = 0.38
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            cover = 0.0
            for sy in range(samples):
                for sx in range(samples):
                    x = (col + (sx + 0.5) / samples) / size - 0.5
                    y = (row + (sy + 0.5) / samples) / size - 0.5
                    r = math.hypot(x, y)
                    if inner <= r <= outer:
                        turn = (math.atan2(y, x) / (2 * math.pi)) % 1.0
                        if turn <= 0.75:
                            cover += 0.25 + 0.75 * turn / 0.75
            line.extend(ACCENT)
            line.append(round(255 * cover / (samples * samples)))
        rows.append(bytes(line))
    return rows


def render_disc(size):
    """RGBA rows of a white disc as wide as the picture, its edge smoothed:
    the mask that makes a picture round (a MaskGroup's maskUri)."""
    samples = 4
    rows = []
    for row in range(size):
        line = bytearray()
        for col in range(size):
            inside = 0
            for sy in range(samples):
                for sx in range(samples):
                    x = (col + (sx + 0.5) / samples) / size - 0.5
                    y = (row + (sy + 0.5) / samples) / size - 0.5
                    if math.hypot(x, y) <= 0.5:
                        inside += 1
            line.extend((255, 255, 255))
            line.append(round(255 * inside / (samples * samples)))
        rows.append(bytes(line))
    return rows


def write_png(path, width, height, rows):
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + row for row in rows)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as out:
        out.write(png)


def main():
    folder = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "images")
    os.makedirs(folder, exist_ok=True)
    write_png(os.path.join(folder, "watched.png"), MARK_SIZE, MARK_SIZE, render_mark(MARK_SIZE))
    print("images/watched.png", MARK_SIZE, "x", MARK_SIZE)
    boxes = (
        ("button-focus.9.png", ACCENT, RADIUS),
        ("button-footprint.9.png", SURFACE3, RADIUS),
        ("guide-cell.9.png", SURFACE2, 6),
        ("guide-past.9.png", PAST, 0),
    )
    for name, color, radius in boxes:
        write_png(os.path.join(folder, name), BOX + 2, BOX + 2, render_box(color, radius))
        print("images/" + name, BOX + 2, "x", BOX + 2)
    write_png(os.path.join(folder, "spinner.png"), SPINNER_SIZE, SPINNER_SIZE, render_spinner(SPINNER_SIZE))
    print("images/spinner.png", SPINNER_SIZE, "x", SPINNER_SIZE)
    write_png(os.path.join(folder, "round.png"), ROUND_SIZE, ROUND_SIZE, render_disc(ROUND_SIZE))
    print("images/round.png", ROUND_SIZE, "x", ROUND_SIZE)


if __name__ == "__main__":
    main()
